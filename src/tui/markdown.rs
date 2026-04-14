use std::sync::LazyLock;

use pulldown_cmark::{CodeBlockKind, Event, HeadingLevel, Options, Parser, Tag, TagEnd};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span, Text};
use syntect::easy::HighlightLines;
use syntect::highlighting::ThemeSet;
use syntect::parsing::SyntaxSet;

use crate::tui::theme::Theme;

static SYNTAX_SET: LazyLock<SyntaxSet> = LazyLock::new(SyntaxSet::load_defaults_newlines);
static THEME_SET: LazyLock<ThemeSet> = LazyLock::new(ThemeSet::load_defaults);

/// Render a markdown string into ratatui-styled text, applying colors and
/// modifiers from the given Theme. The `width` parameter is used for
/// width-dependent elements like horizontal rules.
pub fn render_markdown(input: &str, theme: &Theme, width: u16) -> Text<'static> {
    if input.is_empty() || input.trim().is_empty() {
        return Text::default();
    }

    let mut renderer = MarkdownRenderer::new(theme, width);
    let options = Options::ENABLE_STRIKETHROUGH | Options::ENABLE_TABLES;
    let parser = Parser::new_ext(input, options);

    for event in parser {
        renderer.process_event(event);
    }

    // Flush any remaining spans
    renderer.flush_line();

    // Remove trailing blank lines
    while renderer
        .lines
        .last()
        .is_some_and(|l| l.spans.is_empty() || l.spans.iter().all(|s| s.content.trim().is_empty()))
    {
        renderer.lines.pop();
    }

    Text::from(renderer.lines)
}

// ---------------------------------------------------------------------------
// Internal renderer state machine
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy)]
enum ListKind {
    Bullet,
    Ordered(u64),
}

struct MarkdownRenderer<'t> {
    theme: &'t Theme,
    width: u16,
    lines: Vec<Line<'static>>,
    active_spans: Vec<Span<'static>>,
    bold: bool,
    italic: bool,
    in_code_block: bool,
    code_block_lang: Option<String>,
    in_heading: bool,
    in_link: bool,
    list_stack: Vec<ListKind>,
    item_prefix_pending: bool,
}

impl<'t> MarkdownRenderer<'t> {
    fn new(theme: &'t Theme, width: u16) -> Self {
        Self {
            theme,
            width,
            lines: Vec::new(),
            active_spans: Vec::new(),
            bold: false,
            italic: false,
            in_code_block: false,
            code_block_lang: None,
            in_heading: false,
            in_link: false,
            list_stack: Vec::new(),
            item_prefix_pending: false,
        }
    }

    fn flush_line(&mut self) {
        if !self.active_spans.is_empty() {
            let spans = std::mem::take(&mut self.active_spans);
            self.lines.push(Line::from(spans));
        }
    }

    fn push_blank_line(&mut self) {
        self.lines.push(Line::default());
    }

    fn current_style(&self) -> Style {
        let mut style = Style::default().fg(self.theme.text_primary);
        if self.in_heading {
            style = style
                .fg(self.theme.accent_info)
                .add_modifier(Modifier::BOLD);
        } else if self.in_link {
            style = style.fg(self.theme.accent_info);
        }
        if self.bold {
            style = style.add_modifier(Modifier::BOLD);
        }
        if self.italic {
            style = style.add_modifier(Modifier::ITALIC);
        }
        style
    }

    fn heading_prefix(level: HeadingLevel) -> &'static str {
        match level {
            HeadingLevel::H1 => "# ",
            HeadingLevel::H2 => "## ",
            HeadingLevel::H3 => "### ",
            HeadingLevel::H4 => "#### ",
            HeadingLevel::H5 => "##### ",
            HeadingLevel::H6 => "###### ",
        }
    }

    fn wrap_and_push_text(&mut self, text: &str, style: Style) {
        if self.width == 0 || self.in_code_block || self.in_heading {
            self.active_spans.push(Span::styled(text.to_owned(), style));
            return;
        }

        let current_width: usize = self
            .active_spans
            .iter()
            .map(|s| s.content.chars().count())
            .sum();
        let max_width = self.width as usize;

        let mut line_buf = String::new();
        let mut line_len = current_width;

        for word in text.split(' ') {
            let word_len = word.chars().count();
            let sep_len = if line_len > 0 { 1 } else { 0 };

            if line_len + sep_len + word_len > max_width && line_len > current_width {
                if !line_buf.is_empty() {
                    self.active_spans
                        .push(Span::styled(std::mem::take(&mut line_buf), style));
                }
                self.flush_line();
                line_buf.push_str(word);
                line_len = word_len;
            } else {
                if !line_buf.is_empty() {
                    line_buf.push(' ');
                    line_len += 1;
                }
                line_buf.push_str(word);
                line_len += word_len;
            }
        }

        if !line_buf.is_empty() {
            self.active_spans.push(Span::styled(line_buf, style));
        }
    }

    fn list_indent(&self) -> String {
        let depth = self.list_stack.len();
        if depth > 1 {
            "  ".repeat(depth - 1)
        } else {
            String::new()
        }
    }

    fn highlight_code(&mut self, text: &str, lang: &str) {
        let ss = &*SYNTAX_SET;
        let ts = &*THEME_SET;

        let syntax = ss
            .find_syntax_by_token(lang)
            .unwrap_or_else(|| ss.find_syntax_plain_text());

        let Some(highlight_theme) = ts
            .themes
            .get("base16-ocean.dark")
            .or_else(|| ts.themes.values().next())
        else {
            // No syntax themes available — render plain text without highlighting
            for line in text.split('\n') {
                self.lines.push(Line::from(Span::raw(line.to_string())));
            }
            return;
        };

        let mut highlighter = HighlightLines::new(syntax, highlight_theme);

        let text_lines: Vec<&str> = text.split('\n').collect();
        let mut highlighted: Vec<Vec<Span<'static>>> = Vec::with_capacity(text_lines.len());

        for line in &text_lines {
            let mut line_spans = Vec::new();
            if let Ok(ranges) = highlighter.highlight_line(line, ss) {
                for (style, token) in ranges {
                    if !token.is_empty() {
                        let fg = syntect_to_ratatui_color(style.foreground);
                        let mut ratatui_style = Style::default().fg(fg);
                        if style
                            .font_style
                            .contains(syntect::highlighting::FontStyle::BOLD)
                        {
                            ratatui_style = ratatui_style.add_modifier(Modifier::BOLD);
                        }
                        if style
                            .font_style
                            .contains(syntect::highlighting::FontStyle::ITALIC)
                        {
                            ratatui_style = ratatui_style.add_modifier(Modifier::ITALIC);
                        }
                        line_spans.push(Span::styled(token.to_string(), ratatui_style));
                    }
                }
            } else if !line.is_empty() {
                line_spans.push(Span::styled(
                    line.to_string(),
                    Style::default().fg(self.theme.text_secondary),
                ));
            }
            highlighted.push(line_spans);
        }

        // Now append collected spans to self without borrow conflicts.
        for (i, line_spans) in highlighted.into_iter().enumerate() {
            self.active_spans.extend(line_spans);
            if i < text_lines.len() - 1 {
                self.flush_line();
            }
        }
    }

    fn process_event(&mut self, event: Event<'_>) {
        match event {
            // --- Block-level start tags ---
            Event::Start(Tag::Heading { level, .. }) => {
                self.flush_line();
                // Add blank line before heading for visual section separation
                if !self.lines.is_empty() {
                    self.push_blank_line();
                }
                self.in_heading = true;
                let prefix = Self::heading_prefix(level);
                self.active_spans.push(Span::styled(
                    prefix.to_string(),
                    Style::default()
                        .fg(self.theme.accent_info)
                        .add_modifier(Modifier::BOLD),
                ));
            }
            Event::End(TagEnd::Heading(_)) => {
                self.in_heading = false;
                self.flush_line();
                self.push_blank_line();
            }

            Event::Start(Tag::Paragraph) => {
                self.flush_line();
            }
            Event::End(TagEnd::Paragraph) => {
                self.flush_line();
                self.push_blank_line();
            }

            Event::Start(Tag::CodeBlock(kind)) => {
                self.flush_line();
                self.in_code_block = true;
                self.code_block_lang = match kind {
                    CodeBlockKind::Fenced(lang) => {
                        let lang = lang.as_ref().trim().to_string();
                        if lang.is_empty() { None } else { Some(lang) }
                    }
                    CodeBlockKind::Indented => None,
                };
            }
            Event::End(TagEnd::CodeBlock) => {
                self.flush_line();
                self.in_code_block = false;
                self.code_block_lang = None;
                self.push_blank_line();
            }

            Event::Start(Tag::List(start)) => {
                self.flush_line();
                match start {
                    Some(n) => self.list_stack.push(ListKind::Ordered(n)),
                    None => self.list_stack.push(ListKind::Bullet),
                }
            }
            Event::End(TagEnd::List(_)) => {
                self.list_stack.pop();
                if self.list_stack.is_empty() {
                    self.push_blank_line();
                }
            }

            Event::Start(Tag::Item) => {
                self.flush_line();
                self.item_prefix_pending = true;
            }
            Event::End(TagEnd::Item) => {
                self.flush_line();
            }

            // --- Inline start tags ---
            Event::Start(Tag::Strong) => {
                self.bold = true;
            }
            Event::End(TagEnd::Strong) => {
                self.bold = false;
            }

            Event::Start(Tag::Emphasis) => {
                self.italic = true;
            }
            Event::End(TagEnd::Emphasis) => {
                self.italic = false;
            }

            Event::Start(Tag::Link { .. }) => {
                self.in_link = true;
            }
            Event::End(TagEnd::Link) => {
                self.in_link = false;
            }

            // --- Leaf events ---
            Event::Text(text) => {
                if self.in_code_block {
                    let text_str = text.into_string();
                    if let Some(lang) = self.code_block_lang.clone() {
                        self.highlight_code(&text_str, &lang);
                    } else {
                        let style = Style::default().fg(self.theme.text_secondary);
                        let code_lines: Vec<&str> = text_str.split('\n').collect();
                        for (i, code_line) in code_lines.iter().enumerate() {
                            if !code_line.is_empty() {
                                self.active_spans
                                    .push(Span::styled(code_line.to_string(), style));
                            }
                            if i < code_lines.len() - 1 {
                                self.flush_line();
                            }
                        }
                    }
                } else {
                    // Emit list item prefix if pending
                    if self.item_prefix_pending {
                        self.item_prefix_pending = false;
                        let indent = self.list_indent();
                        let prefix_style = Style::default().fg(self.theme.text_secondary);
                        let prefix = match self.list_stack.last_mut() {
                            Some(ListKind::Bullet) => format!("{indent}• "),
                            Some(ListKind::Ordered(n)) => {
                                let p = format!("{indent}{n}. ");
                                *n += 1;
                                p
                            }
                            None => String::new(),
                        };
                        if !prefix.is_empty() {
                            self.active_spans.push(Span::styled(prefix, prefix_style));
                        }
                    }
                    let style = self.current_style();
                    self.wrap_and_push_text(&text.into_string(), style);
                }
            }

            Event::Code(text) => {
                // Emit list item prefix if pending
                if self.item_prefix_pending {
                    self.item_prefix_pending = false;
                    let indent = self.list_indent();
                    let prefix_style = Style::default().fg(self.theme.text_secondary);
                    let prefix = match self.list_stack.last_mut() {
                        Some(ListKind::Bullet) => format!("{indent}• "),
                        Some(ListKind::Ordered(n)) => {
                            let p = format!("{indent}{n}. ");
                            *n += 1;
                            p
                        }
                        None => String::new(),
                    };
                    if !prefix.is_empty() {
                        self.active_spans.push(Span::styled(prefix, prefix_style));
                    }
                }
                let style = Style::default().fg(self.theme.accent_warning);
                self.active_spans
                    .push(Span::styled(text.into_string(), style));
            }

            Event::SoftBreak => {
                self.active_spans.push(Span::raw(" ".to_string()));
            }

            Event::HardBreak => {
                self.flush_line();
            }

            Event::Rule => {
                self.flush_line();
                let w = if self.width == 0 {
                    1
                } else {
                    self.width as usize
                };
                let rule_str: String = "─".repeat(w);
                self.active_spans.push(Span::styled(
                    rule_str,
                    Style::default().fg(self.theme.text_muted),
                ));
                self.flush_line();
                self.push_blank_line();
            }

            // Ignore other events (footnotes, task list markers, etc.)
            _ => {}
        }
    }
}

/// Convert a syntect RGBA color to the nearest ratatui Color.
fn syntect_to_ratatui_color(c: syntect::highlighting::Color) -> Color {
    Color::Rgb(c.r, c.g, c.b)
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::style::{Color, Modifier};

    fn theme() -> Theme {
        Theme::dark()
    }

    /// Collect every Span from a Text into a flat Vec for easier assertions.
    fn all_spans(text: &ratatui::text::Text<'static>) -> Vec<ratatui::text::Span<'static>> {
        text.lines
            .iter()
            .flat_map(|line| line.spans.iter().cloned())
            .collect()
    }

    // -----------------------------------------------------------------------
    // Group 1 — Happy path per element type
    // -----------------------------------------------------------------------

    #[test]
    fn empty_input_returns_empty_text() {
        let result = render_markdown("", &theme(), 80);
        assert!(
            result.lines.is_empty(),
            "expected no lines for empty input, got {}",
            result.lines.len()
        );
    }

    #[test]
    fn plain_text_uses_text_primary_color() {
        let result = render_markdown("hello world", &theme(), 80);
        assert!(!result.lines.is_empty(), "expected at least one line");
        let first_span = result.lines[0].spans.first().expect("expected a span");
        assert_eq!(
            first_span.style.fg,
            Some(Color::White),
            "plain text should use text_primary (White)"
        );
        assert!(
            first_span.content.contains("hello world"),
            "span content should contain the input text"
        );
    }

    #[test]
    fn h1_is_bold_and_accent_info() {
        let result = render_markdown("# Hello", &theme(), 80);
        assert!(!result.lines.is_empty(), "expected at least one line");
        let first_span = result.lines[0].spans.first().expect("expected a span");
        assert_eq!(
            first_span.style.fg,
            Some(Color::Cyan),
            "h1 should use accent_info (Cyan)"
        );
        assert!(
            first_span.style.add_modifier.contains(Modifier::BOLD),
            "h1 should be bold"
        );
    }

    #[test]
    fn h2_is_bold_and_accent_info() {
        let result = render_markdown("## SubTitle", &theme(), 80);
        assert!(!result.lines.is_empty());
        let first_span = result.lines[0].spans.first().expect("expected a span");
        assert_eq!(first_span.style.fg, Some(Color::Cyan));
        assert!(first_span.style.add_modifier.contains(Modifier::BOLD));
    }

    #[test]
    fn h3_is_bold_and_accent_info() {
        let result = render_markdown("### Deep", &theme(), 80);
        assert!(!result.lines.is_empty());
        let first_span = result.lines[0].spans.first().expect("expected a span");
        assert_eq!(first_span.style.fg, Some(Color::Cyan));
        assert!(first_span.style.add_modifier.contains(Modifier::BOLD));
    }

    #[test]
    fn code_block_uses_text_secondary_color() {
        let input = "```\nlet x = 1;\n```";
        let result = render_markdown(input, &theme(), 80);
        let code_line = result
            .lines
            .iter()
            .find(|line| line.spans.iter().any(|s| s.content.contains("let x = 1;")))
            .expect("expected a line containing the code body");
        for span in &code_line.spans {
            assert_eq!(
                span.style.fg,
                Some(Color::DarkGray),
                "code block spans should use text_secondary (DarkGray), got span: {:?}",
                span.content
            );
        }
    }

    #[test]
    fn inline_code_uses_accent_warning_color() {
        let result = render_markdown("Use `cargo test` to run", &theme(), 80);
        let spans = all_spans(&result);
        let code_span = spans
            .iter()
            .find(|s| s.content.contains("cargo test"))
            .expect("expected a span containing the inline code content");
        assert_eq!(
            code_span.style.fg,
            Some(Color::Yellow),
            "inline code should use accent_warning (Yellow)"
        );
    }

    #[test]
    fn bold_text_has_bold_modifier() {
        let result = render_markdown("This is **bold** text", &theme(), 80);
        let spans = all_spans(&result);
        let bold_span = spans
            .iter()
            .find(|s| s.content.contains("bold"))
            .expect("expected a span containing 'bold'");
        assert!(
            bold_span.style.add_modifier.contains(Modifier::BOLD),
            "bold text should have BOLD modifier"
        );
    }

    #[test]
    fn italic_text_has_italic_modifier() {
        let result = render_markdown("This is *italic* text", &theme(), 80);
        let spans = all_spans(&result);
        let italic_span = spans
            .iter()
            .find(|s| s.content.contains("italic"))
            .expect("expected a span containing 'italic'");
        assert!(
            italic_span.style.add_modifier.contains(Modifier::ITALIC),
            "italic text should have ITALIC modifier"
        );
    }

    #[test]
    fn unordered_list_item_has_bullet_prefix() {
        let result = render_markdown("- first item", &theme(), 80);
        assert!(!result.lines.is_empty(), "expected at least one line");
        let spans = all_spans(&result);
        let bullet = spans
            .iter()
            .find(|s| s.content.contains('•'))
            .expect("expected bullet span");
        assert_eq!(
            bullet.style.fg,
            Some(Color::DarkGray),
            "bullet prefix should use text_secondary (DarkGray)"
        );
    }

    #[test]
    fn ordered_list_item_has_number_prefix() {
        let result = render_markdown("1. first\n2. second", &theme(), 80);
        let spans = all_spans(&result);
        let first_prefix = spans
            .iter()
            .find(|s| s.content.contains("1."))
            .expect("expected '1.' prefix span");
        assert_eq!(
            first_prefix.style.fg,
            Some(Color::DarkGray),
            "ordered list prefix should use text_secondary (DarkGray)"
        );
    }

    #[test]
    fn link_text_uses_accent_info_color() {
        let result = render_markdown("See [docs](https://example.com) for more", &theme(), 80);
        let spans = all_spans(&result);
        let link_span = spans
            .iter()
            .find(|s| s.content.contains("docs"))
            .expect("expected a span for the link label");
        assert_eq!(
            link_span.style.fg,
            Some(Color::Cyan),
            "link text should use accent_info (Cyan)"
        );
    }

    #[test]
    fn horizontal_rule_uses_text_muted() {
        let result = render_markdown("---", &theme(), 80);
        assert!(!result.lines.is_empty(), "expected at least one line");
        let spans = all_spans(&result);
        let rule_span = spans
            .iter()
            .find(|s| s.content.contains('─'))
            .expect("expected rule span with ─ characters");
        assert_eq!(
            rule_span.style.fg,
            Some(Color::DarkGray),
            "horizontal rule should use text_muted (DarkGray)"
        );
    }

    // -----------------------------------------------------------------------
    // Group 2 — Multi-element / paragraph behavior
    // -----------------------------------------------------------------------

    #[test]
    fn two_paragraphs_separated_by_blank_line() {
        let result = render_markdown("First paragraph\n\nSecond paragraph", &theme(), 80);
        assert!(
            result.lines.len() >= 3,
            "expected at least 3 lines (p1, blank, p2), got {}",
            result.lines.len()
        );
    }

    #[test]
    fn multi_line_code_block_all_lines_secondary_color() {
        let input = "```\nline one\nline two\nline three\n```";
        let result = render_markdown(input, &theme(), 80);
        let interior_lines: Vec<_> = result
            .lines
            .iter()
            .filter(|line| {
                line.spans.iter().any(|s| {
                    s.content.contains("line one")
                        || s.content.contains("line two")
                        || s.content.contains("line three")
                })
            })
            .collect();
        assert_eq!(
            interior_lines.len(),
            3,
            "expected three interior code lines"
        );
        for line in interior_lines {
            for span in &line.spans {
                assert_eq!(
                    span.style.fg,
                    Some(Color::DarkGray),
                    "all code block spans should be DarkGray, offending content: {:?}",
                    span.content
                );
            }
        }
    }

    #[test]
    fn mixed_bold_and_italic_in_same_line() {
        let result = render_markdown("**bold** and *italic*", &theme(), 80);
        let spans = all_spans(&result);
        let has_bold = spans
            .iter()
            .any(|s| s.style.add_modifier.contains(Modifier::BOLD) && s.content.contains("bold"));
        let has_italic = spans.iter().any(|s| {
            s.style.add_modifier.contains(Modifier::ITALIC) && s.content.contains("italic")
        });
        assert!(has_bold, "expected a bold span");
        assert!(has_italic, "expected an italic span");
    }

    #[test]
    fn nested_inline_code_in_plain_sentence() {
        let result = render_markdown("Run `make` now", &theme(), 80);
        let spans = all_spans(&result);

        let run_span = spans
            .iter()
            .find(|s| s.content.contains("Run"))
            .expect("expected 'Run' span");
        assert_eq!(
            run_span.style.fg,
            Some(Color::White),
            "'Run' should be text_primary"
        );

        let code_span = spans
            .iter()
            .find(|s| s.content.contains("make"))
            .expect("expected 'make' span");
        assert_eq!(
            code_span.style.fg,
            Some(Color::Yellow),
            "'make' should be accent_warning"
        );
    }

    #[test]
    fn unordered_list_multiple_items() {
        let result = render_markdown("- alpha\n- beta", &theme(), 80);
        let bullet_count = all_spans(&result)
            .iter()
            .filter(|s| s.content.contains('•'))
            .count();
        assert!(
            bullet_count >= 2,
            "expected at least 2 bullet spans, got {}",
            bullet_count
        );
    }

    // -----------------------------------------------------------------------
    // Group 3 — Edge cases
    // -----------------------------------------------------------------------

    #[test]
    fn whitespace_only_input_returns_empty_or_blank() {
        let result = render_markdown("   \n  \n", &theme(), 80);
        let _ = result;
    }

    #[test]
    fn only_newlines_input() {
        let result = render_markdown("\n\n\n", &theme(), 80);
        let _ = result;
    }

    #[test]
    fn header_with_no_text_after_hash() {
        let result = render_markdown("# ", &theme(), 80);
        let _ = result; // must not panic
    }

    #[test]
    fn very_long_line_does_not_panic() {
        let long_line = "a".repeat(10_000);
        let result = render_markdown(&long_line, &theme(), 80);
        assert!(!result.lines.is_empty(), "expected at least one line");
    }

    #[test]
    fn zero_width_does_not_panic() {
        let result = render_markdown("# Hello", &theme(), 0);
        let _ = result;
    }

    #[test]
    fn unicode_content_preserved() {
        let result = render_markdown("こんにちは 世界", &theme(), 80);
        let spans = all_spans(&result);
        assert!(
            spans.iter().any(|s| s.content.contains("こんにちは")),
            "unicode content should be preserved in spans"
        );
    }

    #[test]
    fn bold_and_link_coexist_without_panic() {
        let result = render_markdown("**[bold link](http://x.com)**", &theme(), 80);
        assert!(!result.lines.is_empty());
    }

    // -----------------------------------------------------------------------
    // Group 4 — Syntax highlighting
    // -----------------------------------------------------------------------

    #[test]
    fn rust_code_block_uses_rgb_colors() {
        let input = "```rust\nlet x = 42;\n```";
        let result = render_markdown(input, &theme(), 80);
        let spans = all_spans(&result);
        // With syntax highlighting, at least one span should use an RGB color
        let has_rgb = spans
            .iter()
            .any(|s| matches!(s.style.fg, Some(Color::Rgb(_, _, _))));
        assert!(
            has_rgb,
            "rust code block should produce RGB-colored spans from syntax highlighting"
        );
    }

    #[test]
    fn python_code_block_uses_rgb_colors() {
        let input = "```python\ndef hello():\n    print('hi')\n```";
        let result = render_markdown(input, &theme(), 80);
        let spans = all_spans(&result);
        let has_rgb = spans
            .iter()
            .any(|s| matches!(s.style.fg, Some(Color::Rgb(_, _, _))));
        assert!(
            has_rgb,
            "python code block should produce RGB-colored spans from syntax highlighting"
        );
    }

    #[test]
    fn javascript_code_block_uses_rgb_colors() {
        let input = "```javascript\nconst x = 42;\n```";
        let result = render_markdown(input, &theme(), 80);
        let spans = all_spans(&result);
        let has_rgb = spans
            .iter()
            .any(|s| matches!(s.style.fg, Some(Color::Rgb(_, _, _))));
        assert!(
            has_rgb,
            "javascript code block should produce RGB-colored spans from syntax highlighting"
        );
    }

    #[test]
    fn unknown_language_falls_back_gracefully() {
        let input = "```nonexistent_lang_xyz\nsome code here\n```";
        let result = render_markdown(input, &theme(), 80);
        // Should not panic, and should produce some output
        assert!(
            !result.lines.is_empty(),
            "unknown language should still produce output"
        );
    }

    #[test]
    fn code_block_without_lang_uses_text_secondary() {
        let input = "```\nplain code\n```";
        let result = render_markdown(input, &theme(), 80);
        let code_line = result
            .lines
            .iter()
            .find(|line| line.spans.iter().any(|s| s.content.contains("plain code")))
            .expect("expected a line containing the code body");
        for span in &code_line.spans {
            assert_eq!(
                span.style.fg,
                Some(Color::DarkGray),
                "code block without lang should use text_secondary (DarkGray)"
            );
        }
    }

    #[test]
    fn highlighted_code_preserves_content() {
        let input = "```rust\nfn main() {}\n```";
        let result = render_markdown(input, &theme(), 80);
        let all_text: String = all_spans(&result)
            .iter()
            .map(|s| s.content.as_ref())
            .collect();
        assert!(
            all_text.contains("fn") && all_text.contains("main"),
            "highlighted code should preserve the source text"
        );
    }

    #[test]
    fn syntax_highlighting_200_lines_performance() {
        // Warm up lazy statics (one-time init cost excluded from benchmark).
        let _ = render_markdown("```rust\nlet x = 1;\n```", &theme(), 80);

        let mut code = String::new();
        for i in 0..200 {
            code.push_str(&format!("let var_{i} = {i};\n"));
        }
        let input = format!("```rust\n{code}```");
        let t = theme();
        let start = std::time::Instant::now();
        let _ = render_markdown(&input, &t, 80);
        let elapsed = start.elapsed();
        // In release mode this completes in <10ms. Debug mode is ~10x slower,
        // so we allow up to 500ms to avoid flaky CI on unoptimized builds.
        assert!(
            elapsed.as_millis() < 500,
            "highlighting 200 lines took {}ms (expected <500ms debug, <50ms release)",
            elapsed.as_millis()
        );
    }

    // -----------------------------------------------------------------------
    // Group 5 — Word-wrapping at narrow widths (#256)
    // -----------------------------------------------------------------------

    #[test]
    fn long_paragraph_wraps_into_multiple_lines_at_narrow_width() {
        let input = "This is a very long paragraph that must wrap at narrow width.";
        let wide = render_markdown(input, &theme(), 80);
        let narrow = render_markdown(input, &theme(), 20);

        let wide_content_lines = wide
            .lines
            .iter()
            .filter(|l| l.spans.iter().any(|s| !s.content.trim().is_empty()))
            .count();
        let narrow_content_lines = narrow
            .lines
            .iter()
            .filter(|l| l.spans.iter().any(|s| !s.content.trim().is_empty()))
            .count();

        assert!(
            narrow_content_lines > wide_content_lines,
            "expected more lines at width=20 ({}) than at width=80 ({})",
            narrow_content_lines,
            wide_content_lines
        );
    }

    #[test]
    fn list_item_with_long_text_wraps_at_narrow_width() {
        let input = "- This is a very long list item that definitely exceeds twenty characters";
        let wide = render_markdown(input, &theme(), 80);
        let narrow = render_markdown(input, &theme(), 20);

        let wide_content_lines = wide
            .lines
            .iter()
            .filter(|l| l.spans.iter().any(|s| !s.content.trim().is_empty()))
            .count();
        let narrow_content_lines = narrow
            .lines
            .iter()
            .filter(|l| l.spans.iter().any(|s| !s.content.trim().is_empty()))
            .count();

        assert!(
            narrow_content_lines > wide_content_lines,
            "expected list item to wrap at width=20: narrow={} lines, wide={} lines",
            narrow_content_lines,
            wide_content_lines
        );
    }

    #[test]
    fn code_block_does_not_wrap_at_narrow_width() {
        let input = "```\nlet very_long_variable_name = some_function_call(argument_one, argument_two);\n```";
        let wide = render_markdown(input, &theme(), 80);
        let narrow = render_markdown(input, &theme(), 20);

        let wide_code_lines = wide
            .lines
            .iter()
            .filter(|l| {
                l.spans
                    .iter()
                    .any(|s| s.content.contains("very_long_variable_name"))
            })
            .count();
        let narrow_code_lines = narrow
            .lines
            .iter()
            .filter(|l| {
                l.spans
                    .iter()
                    .any(|s| s.content.contains("very_long_variable_name"))
            })
            .count();

        assert_eq!(wide_code_lines, 1);
        assert_eq!(
            narrow_code_lines, 1,
            "code block must NOT be wrapped at narrow width"
        );
    }

    #[test]
    fn heading_does_not_wrap_at_narrow_width() {
        let input = "## A Very Long Heading That Exceeds Twenty Characters Easily";
        let wide = render_markdown(input, &theme(), 80);
        let narrow = render_markdown(input, &theme(), 20);

        let wide_heading_lines = wide
            .lines
            .iter()
            .filter(|l| {
                l.spans
                    .iter()
                    .any(|s| s.content.contains("Very Long Heading"))
            })
            .count();
        let narrow_heading_lines = narrow
            .lines
            .iter()
            .filter(|l| {
                l.spans
                    .iter()
                    .any(|s| s.content.contains("Very Long Heading"))
            })
            .count();

        assert_eq!(
            wide_heading_lines, narrow_heading_lines,
            "heading line count must be the same at width=80 and width=20"
        );
    }

    #[test]
    fn performance_10kb_under_5ms() {
        let mut doc = String::with_capacity(10_240);
        for i in 0..50 {
            doc.push_str(&format!("# Heading {}\n\n", i));
            doc.push_str("Plain text with **bold** and *italic* and `code`.\n\n");
            doc.push_str("- item one\n- item two\n\n");
            doc.push_str("```\nlet x = 42;\n```\n\n");
        }
        let t = theme();
        let start = std::time::Instant::now();
        for _ in 0..10 {
            let _ = render_markdown(&doc, &t, 80);
        }
        let elapsed = start.elapsed() / 10;
        assert!(
            elapsed.as_millis() < 5,
            "render_markdown should complete in <5ms, took {}ms",
            elapsed.as_millis()
        );
    }
}
