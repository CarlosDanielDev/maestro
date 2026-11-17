//! TabBar (design system §04 `ds::tab_bar`): the row under the status bar on
//! the seven top-level screens. Digits `1`-`7` jump between them; the active
//! tab is reversed, so it reads under NO_COLOR too. Inside a flow the
//! CrumbBar (`ds::crumb_bar`) takes this row instead.

use ratatui::{
    Frame,
    layout::Rect,
    style::{Modifier, Style},
    text::{Line, Span},
};

use super::bar_row::{draw_left_right, key_style};
use crate::tui::app::TuiMode;
use crate::tui::theme::Theme;

/// The seven top-level screens, in key order: `(digit, label, mode)`.
pub const TABS: [(char, &str, TuiMode); 7] = [
    ('1', "Home", TuiMode::Landing),
    ('2', "Dashboard", TuiMode::Dashboard),
    ('3', "Issues", TuiMode::IssueBrowser),
    ('4', "Milestones", TuiMode::MilestoneView),
    ('5', "Roadmap", TuiMode::Roadmap),
    ('6', "Stats", TuiMode::ProjectStats),
    ('7', "PRD", TuiMode::Prd),
];

/// The screen a digit key opens, if it is one of `1`-`7`.
pub fn tab_for_digit(c: char) -> Option<TuiMode> {
    TABS.iter().find(|(k, _, _)| *k == c).map(|(_, _, m)| *m)
}

/// Index into [`TABS`] when `mode` is a top-level screen.
pub fn tab_index(mode: TuiMode) -> Option<usize> {
    TABS.iter().position(|(_, _, m)| *m == mode)
}

/// The tab label for a top-level screen (`Landing` reads `Home`).
pub fn tab_label(mode: TuiMode) -> Option<&'static str> {
    tab_index(mode).map(|i| TABS[i].1)
}

pub fn draw(f: &mut Frame, area: Rect, active: usize, theme: &Theme) {
    let on = Style::default()
        .add_modifier(Modifier::REVERSED)
        .add_modifier(Modifier::BOLD);
    let key = key_style(theme);
    let label = Style::default().fg(theme.text_secondary);
    let mut spans = vec![Span::raw(" ")];
    for (i, (k, name, _)) in TABS.iter().enumerate() {
        let (k_style, name_style) = if i == active { (on, on) } else { (key, label) };
        spans.push(Span::styled(" ", if i == active { on } else { label }));
        spans.push(Span::styled(k.to_string(), k_style));
        spans.push(Span::styled(format!(" {name} "), name_style));
        spans.push(Span::raw(" "));
    }
    let right = Line::from(vec![
        Span::styled("?", key),
        Span::styled(" help ", Style::default().fg(theme.text_muted)),
    ]);
    draw_left_right(f, area, Line::from(spans), right);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn digits_one_to_seven_map_to_the_seven_tabs() {
        let modes: Vec<_> = ('1'..='7').filter_map(tab_for_digit).collect();
        assert_eq!(
            modes,
            vec![
                TuiMode::Landing,
                TuiMode::Dashboard,
                TuiMode::IssueBrowser,
                TuiMode::MilestoneView,
                TuiMode::Roadmap,
                TuiMode::ProjectStats,
                TuiMode::Prd,
            ]
        );
        assert_eq!(tab_for_digit('0'), None);
        assert_eq!(tab_for_digit('8'), None);
    }

    #[test]
    fn flows_are_not_tabs() {
        assert_eq!(tab_index(TuiMode::IssueWizard), None);
        assert_eq!(tab_index(TuiMode::Overview), None);
        assert_eq!(tab_label(TuiMode::Landing), Some("Home"));
    }
}
