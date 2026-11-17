//! `ds::md_block`: markdown for wizard previews, critiques and issue bodies
//! (`mdBlock` in `docs/design-system/index.html`, §04 Kit v2). Bare headings,
//! `[TODO: …]` as a reversed warn chip and `#123` in `accent_identifier`.
//! Thin preset over the one shared renderer, `render_markdown_with`.

use ratatui::text::Text;

use crate::tui::markdown::{HeadingStyle, MarkdownOptions, render_markdown_with};
use crate::tui::theme::Theme;

/// Options `md_block` renders with.
pub const MD_BLOCK: MarkdownOptions = MarkdownOptions {
    heading: HeadingStyle::Bare,
    inline_marks: true,
};

/// Render `input` as an `mdBlock`, wrapped to `width`.
pub fn md_block(input: &str, theme: &Theme, width: u16) -> Text<'static> {
    render_markdown_with(input, theme, width, MD_BLOCK)
}

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::style::Modifier;
    use ratatui::text::Span;

    fn spans(text: &Text<'static>) -> Vec<Span<'static>> {
        text.lines.iter().flat_map(|l| l.spans.clone()).collect()
    }

    fn find<'a>(spans: &'a [Span<'static>], content: &str) -> &'a Span<'static> {
        spans
            .iter()
            .find(|s| s.content == content)
            .unwrap_or_else(|| panic!("no span {content:?} in {spans:?}"))
    }

    #[test]
    fn md_block_todo_chip_and_refs() {
        let theme = Theme::dark();
        let text = md_block(
            "## Acceptance\n\nName it [TODO: the feature] for #123, not C#4.",
            &theme,
            80,
        );
        let all = spans(&text);

        let heading = &text.lines[0].spans[0];
        assert_eq!(heading.content, "Acceptance", "bare heading, no `#`");
        assert_eq!(heading.style.fg, Some(theme.accent_warning));
        assert!(heading.style.add_modifier.contains(Modifier::BOLD));

        let chip = find(&all, " TODO ");
        assert_eq!(chip.style.fg, Some(theme.accent_warning));
        assert!(
            chip.style
                .add_modifier
                .contains(Modifier::REVERSED | Modifier::BOLD),
            "chip is reversed + bold so it survives NO_COLOR: {chip:?}"
        );
        let gap = all
            .iter()
            .position(|s| s.content == " TODO ")
            .map(|i| &all[i + 1])
            .expect("span after chip");
        assert_eq!(gap.content, " ", "plain gap between chip and text");
        assert!(!gap.style.add_modifier.contains(Modifier::REVERSED));
        let body = find(&all, "the feature");
        assert_eq!(body.style.fg, Some(theme.accent_warning));
        assert!(
            !all.iter().any(|s| s.content.contains('[')),
            "brackets are dropped: {all:?}"
        );

        let reference = find(&all, "#123");
        assert_eq!(reference.style.fg, Some(theme.accent_identifier));
        // Body line only, so the heading cannot match by accident.
        assert!(
            text.lines[2]
                .spans
                .iter()
                .filter(|s| s.style.fg == Some(theme.accent_identifier))
                .all(|s| s.content == "#123"),
            "`C#4` is not a ref: {all:?}"
        );

        let line: String = text.lines[2]
            .spans
            .iter()
            .map(|s| s.content.as_ref())
            .collect();
        assert_eq!(line, "Name it  TODO  the feature for #123, not C#4.");
    }
}
