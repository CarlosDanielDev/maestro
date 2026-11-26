//! Open the Dashboard on one view (split out of `usage.rs` for the file
//! size budget). The old Cost and Tokens screens are its `c` and `t`
//! views (#1289), the old Agent graph is Deps › files (#1292); #1294
//! removed their `TuiMode` aliases.

use super::Dashboard;
use super::usage::View;
use crate::tui::app::{App, TuiMode};

/// Where `App::navigate_to(mode)` lands: the Dashboard opens on Ops.
/// Going back (nav stack) keeps the last view.
pub fn navigate_target(mode: TuiMode, dash: &mut Dashboard) -> TuiMode {
    if mode == TuiMode::Dashboard {
        dash.view = View::Ops;
    }
    mode
}

/// Navigate to the Dashboard and show `view` (`$`, `t`, F-keys, ⌃K).
pub fn open_view(app: &mut App, view: View) {
    app.navigate_to(TuiMode::Dashboard);
    app.screen_state.dashboard.view = view;
}

/// The ⌃K Agent graph entry: Deps on the files lens, or the Dashboard
/// when `views.agent_graph_enabled` is off.
pub fn open_agent_graph(app: &mut App) {
    if !app.is_agent_graph_enabled() {
        app.navigate_to(TuiMode::Dashboard);
        return;
    }
    open_view(app, View::Deps);
    app.screen_state.dashboard.deps.lens = super::deps::Lens::Files;
}
