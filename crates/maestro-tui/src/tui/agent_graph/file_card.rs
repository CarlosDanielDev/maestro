//! The one detail panel of the Files lens (§02.20 "File card", #1292):
//! the selected file (who holds the claim, who edited, who read, a
//! recorded conflict) or the selected agent (status, role, its files, and
//! on Wide the role sprite coloured by status).

use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::Style;
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;
use uuid::Uuid;

use super::ladder::Ctx;
use super::model::agent_label;
use super::personalities::{glyph_for_role, role_abbrev};
use crate::tui::ds::fit::ellipsize_line;
use crate::tui::ds::panel::{Focus, panel};
use crate::tui::ds::status_chip::status_chip_for_mode;
use crate::tui::screens::sanitize_for_terminal;

/// The one detail panel: the selected file (who holds the claim, who
/// edited, who read) or the selected agent (status and its files).
pub(super) fn draw(f: &mut Frame, area: Rect, cx: &Ctx<'_>, wide: bool) {
    let muted = cx.muted();
    let text = Style::new().fg(cx.theme.text_primary);
    let names = |ids: &[Uuid]| {
        let v: Vec<String> = ids.iter().map(|id| cx.name(*id)).collect();
        if v.is_empty() {
            "none".into()
        } else {
            v.join(" ")
        }
    };
    let row = (cx.sel.files_focused)
        .then_some(cx.sel.file)
        .flatten()
        .and_then(|p| cx.graph.files.iter().find(|r| r.path == p));
    let (title, lines) = if let Some(row) = row {
        let mut lines = vec![
            Line::from(Span::styled(sanitize_for_terminal(&row.path), text)),
            Line::from(vec![
                Span::styled("holds the claim  ", muted),
                Span::styled(
                    row.holder.map_or_else(|| "nobody".into(), |h| cx.name(h)),
                    text,
                ),
            ]),
            Line::from(vec![
                Span::styled("edited by        ", muted),
                Span::styled(names(&row.editors), text),
            ]),
            Line::from(vec![
                Span::styled("read by          ", muted),
                Span::styled(names(&row.readers), text),
            ]),
        ];
        if row.conflict {
            lines.push(Line::from(Span::styled(
                "✗ conflict recorded · policy in Settings [sessions.conflict]",
                Style::new().fg(cx.theme.status_conflict),
            )));
        }
        ("File", lines)
    } else if let Some(s) = cx.sel.agent.and_then(|id| cx.session(id)) {
        let edits: Vec<&str> = (cx.graph.files.iter())
            .filter(|r| r.editors.contains(&s.id))
            .map(|r| r.path.as_str())
            .collect();
        let mut lines = vec![
            Line::from(vec![
                status_chip_for_mode(s.status, cx.theme, cx.nerd_font),
                Span::styled(format!("  {} ", agent_label(s)), text),
                Span::styled(
                    sanitize_for_terminal(s.issue_title.as_deref().unwrap_or(&s.prompt)),
                    muted,
                ),
            ]),
            Line::from(Span::styled(
                format!("role {} · edits {} files", role_abbrev(s.role), edits.len()),
                muted,
            )),
        ];
        // Wide only: the role sprite, coloured by status (§02.20).
        if wide && cx.nerd_font {
            let fg = Style::new().fg(cx.theme.status_color(s.status));
            lines.extend(glyph_for_role(s.role).rows().iter().map(|row| {
                Line::from(Span::styled(
                    format!(" {}", row.iter().collect::<String>()),
                    fg,
                ))
            }));
        }
        lines.extend(
            edits
                .iter()
                .map(|p| Line::from(Span::styled(format!(" {}", sanitize_for_terminal(p)), text))),
        );
        ("Agent", lines)
    } else {
        let hint = "Select an agent or a file (j/k, ← →)";
        ("Detail", vec![Line::from(Span::styled(hint, muted))])
    };
    let block = panel(title, Focus::Inactive, cx.theme);
    let inner = block.inner(area);
    f.render_widget(block, area);
    let width = inner.width as usize;
    let lines: Vec<Line> = lines
        .into_iter()
        .map(|l| ellipsize_line(l, width))
        .collect();
    f.render_widget(Paragraph::new(lines), inner);
}
