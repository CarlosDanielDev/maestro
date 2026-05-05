use super::PrReviewScreen;
use super::types::PrReviewStep;
use crate::tui::icons::{self, IconId};
use crate::tui::markdown::render_markdown;
use crate::tui::screens::{draw_keybinds_bar, sanitize_for_terminal};
use crate::tui::theme::Theme;
use crate::tui::widgets::{BrailleSpinner, focused_selection_style};
use ratatui::{
    Frame,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Paragraph, Wrap},
};
use unicode_width::UnicodeWidthStr;

pub fn draw_pr_review_screen(screen: &PrReviewScreen, f: &mut Frame, area: Rect, theme: &Theme) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(3), Constraint::Length(1)])
        .split(area);

    match screen.step {
        PrReviewStep::Loading => draw_loading(screen, f, chunks[0], theme),
        PrReviewStep::PrList => draw_pr_list(screen, f, chunks[0], theme),
        PrReviewStep::PrDetail => draw_pr_detail(screen, f, chunks[0], theme),
        PrReviewStep::SubmitReview => draw_submit_review(screen, f, chunks[0], theme),
        PrReviewStep::Done => draw_done(f, chunks[0], theme),
    }

    let bindings = match screen.step {
        PrReviewStep::Loading => vec![("Esc", "Cancel")],
        PrReviewStep::PrList => vec![("Enter", "View"), ("j/k", "Navigate"), ("Esc", "Back")],
        PrReviewStep::PrDetail => vec![("r", "Review"), ("j/k", "Scroll"), ("Esc", "Back")],
        PrReviewStep::SubmitReview => vec![("Tab", "Type"), ("Enter", "Submit"), ("Esc", "Cancel")],
        PrReviewStep::Done => vec![("Enter/Esc", "Back")],
    };
    draw_keybinds_bar(f, chunks[1], &bindings, theme);
}

fn draw_loading(screen: &PrReviewScreen, f: &mut Frame, area: Rect, theme: &Theme) {
    let block = theme
        .styled_block("PR Review", false)
        .border_style(Style::default().fg(theme.accent_info));
    let inner = block.inner(area);
    f.render_widget(block, area);

    let mut lines = vec![
        Line::from(""),
        BrailleSpinner::render(
            screen.spinner_tick,
            "Fetching open pull requests...",
            true,
            theme,
        ),
    ];

    if let Some(ref err) = screen.error {
        lines.push(Line::from(""));
        lines.push(Line::from(vec![
            Span::styled(
                " ERROR ",
                Style::default()
                    .fg(theme.accent_error)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::raw(" "),
            Span::styled(
                sanitize_for_terminal(err),
                Style::default().fg(theme.accent_error),
            ),
        ]));
    }

    f.render_widget(Paragraph::new(lines), inner);
}

fn draw_pr_list(screen: &PrReviewScreen, f: &mut Frame, area: Rect, theme: &Theme) {
    let title = format!("Pull Requests ({})", screen.prs.len());
    let block = theme
        .styled_block(&title, false)
        .border_style(Style::default().fg(theme.accent_info));
    let inner = block.inner(area);
    f.render_widget(block, area);

    if screen.prs.is_empty() {
        let msg = Paragraph::new(Line::from(vec![Span::raw(
            "  No open pull requests found.",
        )]))
        .style(Style::default().fg(theme.text_secondary));
        f.render_widget(msg, inner);
        return;
    }

    let visible_height = inner.height as usize;
    let scroll = if screen.selected >= visible_height {
        screen.selected - visible_height + 1
    } else {
        0
    };

    for (i, pr) in screen
        .prs
        .iter()
        .enumerate()
        .skip(scroll)
        .take(visible_height)
    {
        let y = inner.y + (i - scroll) as u16;
        if y >= inner.y + inner.height {
            break;
        }
        let row_area = Rect::new(inner.x, y, inner.width, 1);
        let is_selected = i == screen.selected;

        let prefix = "  ".to_string();
        let draft_tag = if pr.draft { " [DRAFT]" } else { "" };
        let title = sanitize_for_terminal(&pr.title);

        let row_style = if is_selected {
            focused_selection_style(theme)
        } else {
            Style::default().fg(theme.text_primary)
        };
        let number_style = if is_selected {
            row_style
        } else {
            Style::default().fg(theme.accent_info)
        };
        let muted_style = if is_selected {
            row_style
        } else {
            Style::default().fg(theme.text_secondary)
        };
        let draft_style = if is_selected {
            row_style
        } else {
            Style::default().fg(theme.accent_warning)
        };
        let number = format!("#{}", pr.number);
        let author = format!(" @{}", sanitize_for_terminal(&pr.author));
        let stats = format!(" +{} -{}", pr.additions, pr.deletions);
        let draft = draft_tag.to_string();
        let content_width = prefix.width()
            + number.as_str().width()
            + 1
            + title.as_str().width()
            + author.as_str().width()
            + stats.as_str().width()
            + draft.as_str().width();
        let trailing = inner.width.saturating_sub(content_width as u16) as usize;

        let mut spans = vec![
            Span::styled(prefix, row_style),
            Span::styled(number, number_style),
            Span::styled(" ", row_style),
            Span::styled(title, row_style),
            Span::styled(author, muted_style),
            Span::styled(stats, muted_style),
            Span::styled(draft, draft_style),
        ];
        if trailing > 0 {
            spans.push(Span::styled(" ".repeat(trailing), row_style));
        }
        let line = Line::from(spans);
        f.render_widget(Paragraph::new(line), row_area);
    }

    if let Some(ref err) = screen.error {
        let err_line = Line::from(vec![Span::styled(
            format!(" Error: {} ", sanitize_for_terminal(err)),
            Style::default().fg(theme.accent_error),
        )]);
        let err_area = Rect::new(
            inner.x,
            inner.y + inner.height.saturating_sub(1),
            inner.width,
            1,
        );
        f.render_widget(Paragraph::new(err_line), err_area);
    }
}

fn draw_pr_detail(screen: &PrReviewScreen, f: &mut Frame, area: Rect, theme: &Theme) {
    let pr = match screen.current_pr {
        Some(ref pr) => pr,
        None => return,
    };

    let title = format!("PR #{}: {}", pr.number, sanitize_for_terminal(&pr.title));
    let block = theme
        .styled_block(&title, false)
        .border_style(Style::default().fg(theme.accent_info));
    let inner = block.inner(area);
    f.render_widget(block, area);

    // Split into metadata header and body
    let meta_height = 4u16;
    let body_start = inner.y + meta_height;
    let body_height = inner.height.saturating_sub(meta_height);

    // Metadata
    let meta_area = Rect::new(inner.x, inner.y, inner.width, meta_height.min(inner.height));
    let draft_str = if pr.draft { " [DRAFT]" } else { "" };
    let merge_str = if pr.mergeable { "yes" } else { "no" };

    let meta_lines = vec![
        Line::from(vec![
            Span::styled("  Author: ", Style::default().fg(theme.text_secondary)),
            Span::styled(
                sanitize_for_terminal(&pr.author),
                Style::default().fg(theme.text_primary),
            ),
            Span::styled(
                draft_str.to_string(),
                Style::default().fg(theme.accent_warning),
            ),
        ]),
        Line::from(vec![
            Span::styled("  Branch: ", Style::default().fg(theme.text_secondary)),
            Span::styled(
                sanitize_for_terminal(&pr.head_branch),
                Style::default().fg(theme.accent_info),
            ),
            Span::styled(
                format!(" {} ", icons::get(IconId::ArrowRight)),
                Style::default().fg(theme.text_secondary),
            ),
            Span::styled(
                sanitize_for_terminal(&pr.base_branch),
                Style::default().fg(theme.text_primary),
            ),
        ]),
        Line::from(vec![
            Span::styled("  Diff:   ", Style::default().fg(theme.text_secondary)),
            Span::styled(
                format!("+{}", pr.additions),
                Style::default().fg(theme.accent_success),
            ),
            Span::raw(" "),
            Span::styled(
                format!("-{}", pr.deletions),
                Style::default().fg(theme.accent_error),
            ),
            Span::styled(
                format!("  {} files", pr.changed_files),
                Style::default().fg(theme.text_secondary),
            ),
            Span::styled(
                format!("  Mergeable: {}", merge_str),
                Style::default().fg(theme.text_secondary),
            ),
        ]),
        Line::from(vec![Span::styled(
            "  ─".to_string() + &"─".repeat(inner.width.saturating_sub(4) as usize),
            Style::default().fg(theme.border_inactive),
        )]),
    ];
    f.render_widget(Paragraph::new(meta_lines), meta_area);

    // Body with markdown rendering
    if body_height > 0 {
        let body_area = Rect::new(
            inner.x + 1,
            body_start,
            inner.width.saturating_sub(2),
            body_height,
        );
        let body = sanitize_for_terminal(&pr.body);
        let rendered = render_markdown(&body, theme, body_area.width);
        let paragraph = Paragraph::new(rendered)
            .scroll((screen.scroll_offset, 0))
            .wrap(Wrap { trim: false });
        f.render_widget(paragraph, body_area);
    }
}

fn draw_submit_review(screen: &PrReviewScreen, f: &mut Frame, area: Rect, theme: &Theme) {
    let pr = match screen.current_pr {
        Some(ref pr) => pr,
        None => return,
    };

    let title = format!("Review PR #{}", pr.number);
    let block = theme
        .styled_block(&title, false)
        .border_style(Style::default().fg(theme.accent_info));
    let inner = block.inner(area);
    f.render_widget(block, area);

    let events = [
        crate::provider::types::ReviewEvent::Comment,
        crate::provider::types::ReviewEvent::Approve,
        crate::provider::types::ReviewEvent::RequestChanges,
    ];

    let mut lines = vec![
        Line::from(""),
        Line::from(vec![Span::styled(
            "  Review Type: ",
            Style::default().fg(theme.text_secondary),
        )]),
    ];

    // Show review type selector
    for event in &events {
        let is_selected = *event == screen.form.event;
        let marker = if is_selected {
            format!("{} ", icons::get(IconId::DotFill))
        } else {
            format!("{} ", icons::get(IconId::Circle))
        };
        let style = if is_selected {
            Style::default()
                .fg(theme.accent_success)
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(theme.text_primary)
        };
        lines.push(Line::from(vec![
            Span::raw("    "),
            Span::styled(marker, style),
            Span::styled(event.label(), style),
        ]));
    }

    lines.push(Line::from(""));
    lines.push(Line::from(vec![Span::styled(
        "  Comment: ",
        Style::default().fg(theme.text_secondary),
    )]));

    // Show body text with cursor
    let body_display = if screen.form.body.is_empty() {
        "│".to_string()
    } else {
        format!("{}│", screen.form.body)
    };
    lines.push(Line::from(vec![
        Span::raw("    "),
        Span::styled(body_display, Style::default().fg(theme.text_primary)),
    ]));

    f.render_widget(Paragraph::new(lines), inner);
}

fn draw_done(f: &mut Frame, area: Rect, theme: &Theme) {
    let block = theme
        .styled_block("PR Review", false)
        .border_style(Style::default().fg(theme.accent_success));
    let inner = block.inner(area);
    f.render_widget(block, area);

    let lines = vec![
        Line::from(""),
        Line::from(vec![Span::styled(
            format!(
                "  {} Review submitted successfully!",
                icons::get(IconId::CheckCircle)
            ),
            Style::default()
                .fg(theme.accent_success)
                .add_modifier(Modifier::BOLD),
        )]),
        Line::from(""),
        Line::from(vec![Span::styled(
            "  Press Enter or Esc to return.",
            Style::default().fg(theme.text_secondary),
        )]),
    ];

    f.render_widget(Paragraph::new(lines), inner);
}
