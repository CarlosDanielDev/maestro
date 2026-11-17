//! CrumbBar (design system "Shell rules", §02.2): replaces the TabBar inside
//! flows (wizards, drill-ins). Shows where you are (`Home › New issue`) and,
//! on the right, a state slot (draft saved, fetch age, write state). With no
//! state it shows `Esc back  ? help`.

use ratatui::{
    Frame,
    layout::Rect,
    style::{Modifier, Style},
    text::{Line, Span},
};

use super::bar_row::{draw_left_right, key_style};
use super::tab_bar::tab_label;
use crate::tui::app::TuiMode;
use crate::tui::breadcrumb::elide_breadcrumbs;
use crate::tui::theme::Theme;

pub const SEP: &str = " › ";

/// A screen's crumb: its tab name when it is a tab (`Landing` reads `Home`),
/// else its breadcrumb label.
pub fn crumb_label(mode: TuiMode) -> &'static str {
    tab_label(mode).unwrap_or_else(|| mode.breadcrumb_label())
}

/// Crumbs for the nav stack (oldest first) followed by the current screen.
pub fn crumbs(stack: &[TuiMode], current: TuiMode) -> Vec<&'static str> {
    stack
        .iter()
        .chain(std::iter::once(&current))
        .map(|m| crumb_label(*m))
        .collect()
}

pub fn draw(f: &mut Frame, area: Rect, crumbs: &[&str], state: Option<Span<'_>>, theme: &Theme) {
    let muted = Style::default().fg(theme.text_muted);
    let key = key_style(theme);
    let right = match state {
        Some(span) => Line::from(vec![span, Span::raw(" ")]),
        None => Line::from(vec![
            Span::styled("Esc", key),
            Span::styled(" back  ", muted),
            Span::styled("?", key),
            Span::styled(" help ", muted),
        ]),
    };
    let room = area.width.saturating_sub(right.width() as u16 + 2);
    let fitted = elide_breadcrumbs(crumbs, room, SEP);
    let last = fitted.len().saturating_sub(1);
    let mut spans = vec![Span::raw(" ")];
    for (i, crumb) in fitted.into_iter().enumerate() {
        if i == last {
            spans.push(Span::styled(
                crumb,
                Style::default()
                    .fg(theme.text_primary)
                    .add_modifier(Modifier::BOLD),
            ));
        } else {
            spans.push(Span::styled(crumb, muted));
            spans.push(Span::styled(SEP, muted));
        }
    }
    draw_left_right(f, area, Line::from(spans), right);
}

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::{Terminal, backend::TestBackend};

    fn row(crumbs: &[&str], state: Option<Span<'_>>, width: u16) -> String {
        let mut t = Terminal::new(TestBackend::new(width, 1)).unwrap();
        let theme = Theme::dark();
        t.draw(|f| draw(f, f.area(), crumbs, state.clone(), &theme))
            .unwrap();
        let buf = t.backend().buffer();
        (0..width).map(|x| buf[(x, 0)].symbol()).collect()
    }

    #[test]
    fn landing_reads_home_and_the_wizard_follows() {
        assert_eq!(
            crumbs(&[TuiMode::Landing], TuiMode::IssueWizard),
            vec!["Home", "New issue"]
        );
    }

    #[test]
    fn state_slot_is_right_aligned() {
        let out = row(&["Home", "New issue"], Some(Span::raw("● draft saved")), 80);
        assert!(out.starts_with(" Home › New issue"), "{out:?}");
        assert!(out.trim_end().ends_with("● draft saved"), "{out:?}");
    }

    #[test]
    fn no_state_shows_back_and_help() {
        let out = row(&["Home", "Settings"], None, 80);
        assert!(out.trim_end().ends_with("Esc back  ? help"), "{out:?}");
    }
}
