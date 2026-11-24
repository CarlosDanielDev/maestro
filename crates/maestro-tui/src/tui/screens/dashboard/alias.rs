//! The old modes that are Dashboard views now (split out of `usage.rs`
//! for the file size budget): `CostDashboard` → Cost, `TokenDashboard` →
//! Tokens (#1289), `AgentGraph` → Deps › files (#1292).

use super::Dashboard;
use super::usage::View;
use crate::tui::app::{App, TuiMode};

/// The view an old Usage mode opens. These modes are aliases now.
pub fn alias_view(mode: TuiMode) -> Option<View> {
    match mode {
        TuiMode::CostDashboard => Some(View::Cost),
        TuiMode::TokenDashboard => Some(View::Tokens),
        // #1292: the old Agent graph is the Files lens of Deps.
        TuiMode::AgentGraph => Some(View::Deps),
        _ => None,
    }
}

/// `mode`, or `TuiMode::Dashboard` with `dash` on its view when `mode` is
/// an alias. `App::navigate_to` and `route_alias` both go through it.
pub fn resolve(mode: TuiMode, dash: &mut Dashboard) -> TuiMode {
    match alias_view(mode) {
        Some(view) => {
            dash.view = view;
            if mode == TuiMode::AgentGraph {
                dash.deps.lens = super::deps::Lens::Files;
            }
            TuiMode::Dashboard
        }
        None => mode,
    }
}

/// Where `App::navigate_to(mode)` lands: an alias opens its view, the
/// Dashboard opens on Ops. Going back (nav stack) keeps the last view.
pub fn navigate_target(mode: TuiMode, dash: &mut Dashboard) -> TuiMode {
    if mode == TuiMode::Dashboard {
        dash.view = View::Ops;
    }
    resolve(mode, dash)
}

/// Turn an alias mode set straight on `tui_mode` into the Dashboard view.
pub fn route_alias(app: &mut App) {
    app.tui_mode = resolve(app.tui_mode, &mut app.screen_state.dashboard);
}
