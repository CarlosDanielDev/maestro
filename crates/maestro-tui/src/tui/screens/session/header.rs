//! The session screen header (#1288): the first three lines of the
//! Dashboard detail pane, drawn by the pane's own functions
//! (`dashboard::detail::session_header` + `stepper`), so the two cannot
//! drift. `session_header_matches_dashboard_pane` draws both.

use ratatui::text::Line;

use crate::session::types::Session;
use crate::tui::screens::dashboard::detail::{session_header, stepper};
use crate::tui::screens::dashboard::rows::pipeline_stage;
use crate::tui::theme::Theme;

/// Status · model · elapsed · cost · retry, the title, and the pipeline.
pub fn header(
    s: &Session,
    now: chrono::DateTime<chrono::Utc>,
    frame_width: u16,
    width: u16,
    theme: &Theme,
    nerd_font: bool,
) -> Vec<Line<'static>> {
    let mut lines = session_header(s, now, theme, nerd_font);
    let (stage, failed) = pipeline_stage(s.status);
    lines.push(stepper(stage, failed, frame_width, width, theme));
    lines
}
