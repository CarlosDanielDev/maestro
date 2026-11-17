//! Executing step of Teams Launch: one line per wave (#1246). A wave runs
//! only when the previous one has finished, so later waves read
//! `waiting on wave n` until `run_team` reaches them.

use super::TeamWizardScreen;
use crate::tui::theme::Theme;
use ratatui::{
    Frame,
    layout::Rect,
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::Paragraph,
};

impl TeamWizardScreen {
    /// `run_team` started wave `wave` (0-based); earlier waves finished.
    pub fn apply_wave_started(&mut self, wave: usize) {
        self.launch.running_wave = Some(wave);
    }

    /// `false` when there is no plan to draw; the caller falls back to the
    /// plain "Dispatching team run…" state.
    pub(super) fn draw_launch_waves(&self, f: &mut Frame, area: Rect, theme: &Theme) -> bool {
        let Some(plan) = self.launch.plan.as_ref().filter(|p| !p.levels.is_empty()) else {
            return false;
        };
        let running = self.launch.running_wave.unwrap_or(0);
        let mut lines = vec![
            Line::from(Span::styled(
                "Each wave starts when the one before it has finished.",
                Style::default().fg(theme.text_secondary),
            )),
            Line::from(""),
        ];
        for (idx, level) in plan.levels.iter().enumerate() {
            let issues: Vec<String> = level.iter().map(|n| format!("#{n}")).collect();
            let (state, color) = if idx < running {
                ("finished".to_string(), theme.accent_success)
            } else if idx == running {
                ("running".to_string(), theme.accent_info)
            } else {
                (format!("waiting on wave {idx}"), theme.text_secondary)
            };
            lines.push(Line::from(vec![
                Span::styled(
                    format!("Wave {}  ", idx + 1),
                    Style::default()
                        .fg(theme.text_primary)
                        .add_modifier(Modifier::BOLD),
                ),
                Span::styled(
                    format!("{:<24}", issues.join(" ")),
                    Style::default().fg(theme.text_primary),
                ),
                Span::styled(state, Style::default().fg(color)),
            ]));
        }
        f.render_widget(
            Paragraph::new(lines).block(theme.styled_block("Executing", false)),
            area,
        );
        true
    }
}
