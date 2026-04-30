//! Spike renderer: paints a 6×6 sprite (or its ASCII fallback) inside a Rect.
//!
//! See `docs/adr/002-agent-personalities.md` § Sprite Design Language and
//! § ASCII Fallback. The renderer is deliberately simple — one `Paragraph`
//! with six styled `Line`s for the nerd-font path, and a colored `Block` plus
//! a 3-char abbrev for the ASCII path.

use ratatui::Frame;
use ratatui::layout::{Alignment, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Paragraph};

use super::palette::{role_abbrev, role_color};
use super::role::Role;
use super::sprite::glyph_for_role;

/// Render a named sprite for `role` inside `area`.
///
/// `label` is shown above the sprite and should fit in a single row (e.g.
/// `"#536"` or a session id). `use_nerd_font` selects the rendering path:
/// `true` → 6×6 sprite grid; `false` → 3-char colored abbreviation in a small
/// block.
pub fn draw_named_sprite(f: &mut Frame, area: Rect, role: Role, label: &str, use_nerd_font: bool) {
    let title = format!(" {} — {} ", label, role_abbrev(role));
    let block = Block::default()
        .borders(Borders::ALL)
        .title(title)
        .title_alignment(Alignment::Center);
    let inner = block.inner(area);
    f.render_widget(block, area);

    if use_nerd_font {
        draw_sprite_grid(f, inner, role);
    } else {
        draw_ascii_fallback(f, inner, role);
    }
}

fn draw_sprite_grid(f: &mut Frame, area: Rect, role: Role) {
    let sprite = glyph_for_role(role);
    let style = Style::default()
        .fg(role_color(role))
        .add_modifier(Modifier::BOLD);
    let lines: Vec<Line<'_>> = sprite
        .rows()
        .iter()
        .map(|row| {
            let s: String = row.iter().collect();
            Line::from(Span::styled(s, style))
        })
        .collect();
    let p = Paragraph::new(lines).alignment(Alignment::Center);
    f.render_widget(p, area);
}

fn draw_ascii_fallback(f: &mut Frame, area: Rect, role: Role) {
    let style = Style::default()
        .fg(role_color(role))
        .add_modifier(Modifier::BOLD);
    let line = Line::from(vec![
        Span::raw("["),
        Span::styled(role_abbrev(role), style),
        Span::raw("]"),
    ]);
    let p = Paragraph::new(line).alignment(Alignment::Center);
    f.render_widget(p, area);
}

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::Terminal;
    use ratatui::backend::TestBackend;

    fn render_smoke(role: Role, label: &str, nerd: bool) {
        let backend = TestBackend::new(40, 12);
        let mut terminal = Terminal::new(backend).expect("test backend");
        terminal
            .draw(|f| {
                let area = f.area();
                draw_named_sprite(f, area, role, label, nerd);
            })
            .expect("draw");
    }

    /// Snapshot fidelity is the follow-up's concern; here we just exercise
    /// both branches to catch panics.
    #[test]
    fn nerd_font_rendering_does_not_panic() {
        render_smoke(Role::Orchestrator, "S-orchestr", true);
    }

    #[test]
    fn ascii_fallback_rendering_does_not_panic() {
        render_smoke(Role::Implementer, "S-impl", false);
    }

    /// Buffer-content sanity: in ASCII mode the abbreviation `IMP` must be
    /// present somewhere in the rendered cells.
    #[test]
    fn ascii_fallback_contains_role_abbreviation() {
        let backend = TestBackend::new(20, 5);
        let mut terminal = Terminal::new(backend).expect("test backend");
        terminal
            .draw(|f| {
                let area = f.area();
                draw_named_sprite(f, area, Role::Implementer, "S-impl", false);
            })
            .expect("draw");
        let buffer = terminal.backend().buffer();
        let dump: String = buffer
            .content()
            .iter()
            .map(|c| c.symbol())
            .collect::<Vec<_>>()
            .join("");
        assert!(
            dump.contains("IMP"),
            "ASCII buffer must contain the role abbreviation 'IMP'; got: {:?}",
            dump
        );
    }
}
