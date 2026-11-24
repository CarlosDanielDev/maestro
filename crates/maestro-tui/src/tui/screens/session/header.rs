//! The session screen header (#1288): the first three lines of the
//! Dashboard detail pane (`screens/dashboard/detail.rs`), word for word.
//! `session_header_matches_dashboard_pane` draws both and fails if they
//! drift.

use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};

use crate::session::types::Session;
use crate::tui::ds::money::money;
use crate::tui::ds::status_chip::status_chip_for_mode;
use crate::tui::ds::stepper::{Steps, stepper_line};
use crate::tui::screens::dashboard::rows::pipeline_stage;
use crate::tui::screens::sanitize_for_terminal;
use crate::tui::theme::Theme;

const STEPS: [&str; 5] = ["run", "gates", "push", "PR", "CI"];

/// Status · model · elapsed · cost · retry, the title, and the pipeline.
pub fn header(
    s: &Session,
    now: chrono::DateTime<chrono::Utc>,
    frame_width: u16,
    width: u16,
    theme: &Theme,
    nerd_font: bool,
) -> Vec<Line<'static>> {
    let text = Style::new().fg(theme.text_primary);
    let muted = Style::new().fg(theme.text_muted);
    let dot = || Span::styled(" · ", muted);
    // `None` (`$?`) for an unpriced model, the `Session::cost_display` rule.
    let cost = (!(s.cost_unknown && s.cost_usd <= 0.0)).then_some(s.cost_usd);
    let title = s.issue_title.clone().unwrap_or_else(|| s.prompt.clone());
    let status = Line::from(vec![
        Span::raw(" "),
        status_chip_for_mode(s.status, theme, nerd_font),
        Span::raw("   "),
        Span::styled(
            sanitize_for_terminal(&s.model),
            Style::new().fg(theme.accent_info),
        ),
        dot(),
        Span::styled(crate::tui::app::mission::elapsed(s, now), text),
        dot(),
        Span::styled(money(cost), text),
        Span::styled(" · retry ", muted),
        Span::styled(s.retry_count.to_string(), text),
    ]);
    let title = Line::from(Span::styled(
        format!(" {}", sanitize_for_terminal(&title)),
        text.add_modifier(Modifier::BOLD),
    ));
    let (stage, failed) = pipeline_stage(s.status);
    let errors: Vec<usize> = failed.into_iter().collect();
    let steps = Steps {
        labels: &STEPS,
        current: stage,
        errors: &errors,
    };
    let label = " PIPELINE ";
    let room = width.saturating_sub(label.len() as u16);
    let mut pipeline = vec![Span::styled(label, muted.add_modifier(Modifier::BOLD))];
    pipeline.extend(stepper_line(steps, frame_width, room, theme).spans);
    vec![status, title, Line::from(pipeline)]
}
