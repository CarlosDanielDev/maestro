use crate::session::types::SessionStatus;
use crate::tui::app::{SessionView, TuiMode};

use super::keymap::{
    FKeyAction, FKeyRelevance, InlineHint, KeyBindingGroup, ModeKeyMap, global_keybindings,
};

/// Keys of `mode`'s inline hints that do nothing right now, so the footer
/// leaves them out. #1384: Compact Home has no Actions panel, so `Tab` has
/// nowhere to go.
pub fn hidden_hint_keys(mode: TuiMode, home_actions_visible: bool) -> &'static [&'static str] {
    match mode {
        TuiMode::Landing if !home_actions_visible => &["Tab"],
        _ => &[],
    }
}

/// Build the `ModeKeyMap` for a given `TuiMode`.
///
/// `screen_bindings` should come from the active screen's `KeymapProvider::keybindings()`
/// (empty slice for modes that don't implement `Screen`).
///
/// `_agent_graph_enabled` is unused since #1292: the old `[g] Graph`
/// hint is gone (the graph is Dashboard › Deps › files, `g g`).
pub fn mode_keymap(
    mode: TuiMode,
    session_status: Option<SessionStatus>,
    screen_bindings: &[KeyBindingGroup],
    _agent_graph_enabled: bool,
) -> ModeKeyMap {
    let has_session = session_status.is_some();
    let is_terminal = session_status.is_some_and(|s| s.is_terminal());
    let is_running = matches!(session_status, Some(SessionStatus::Running));

    let (mode_label, fkey_vis, hints): (&str, FKeyVis, &[InlineHint]) = match mode {
        TuiMode::Session {
            view: SessionView::Detail,
            ..
        } => (
            "Detail",
            // The view keys are on the strip (#1288).
            FKeyVis::SessionAware,
            &[
                InlineHint {
                    key: "Esc",
                    action: "Back",
                    priority: 0,
                },
                InlineHint {
                    key: "w",
                    action: "Switch",
                    priority: 1,
                },
                InlineHint {
                    key: "X",
                    action: "Kill",
                    priority: 2,
                },
            ],
        ),
        TuiMode::Session {
            view: SessionView::Output,
            ..
        } => (
            "Fullscreen",
            FKeyVis::Minimal,
            &[
                InlineHint {
                    key: "Esc",
                    action: "Back",
                    priority: 0,
                },
                InlineHint {
                    key: "j/k",
                    action: "Scroll",
                    priority: 1,
                },
                InlineHint {
                    key: "End",
                    action: "Follow",
                    priority: 2,
                },
            ],
        ),
        // #1289: Cost and Tokens are Dashboard views; #1292: so is the
        // Agent graph (Deps › files).
        TuiMode::Dashboard => (
            "Dashboard",
            FKeyVis::DashboardLike,
            &[
                InlineHint {
                    key: "i",
                    action: "Issues",
                    priority: 0,
                },
                InlineHint {
                    key: "m",
                    action: "Milestones",
                    priority: 1,
                },
                InlineHint {
                    key: "r",
                    action: "Prompt",
                    priority: 2,
                },
                InlineHint {
                    key: "s",
                    action: "Sessions",
                    priority: 3,
                },
                InlineHint {
                    key: "a",
                    action: "Adapt",
                    priority: 4,
                },
                InlineHint {
                    key: "t",
                    action: "Tokens",
                    priority: 5,
                },
            ],
        ),
        TuiMode::IssueBrowser => (
            "Issue Browser",
            FKeyVis::Minimal,
            &[
                InlineHint {
                    key: "Enter",
                    action: "Launch",
                    priority: 0,
                },
                InlineHint {
                    key: "/",
                    action: "Filter",
                    priority: 1,
                },
                InlineHint {
                    key: "Space",
                    action: "Select",
                    priority: 2,
                },
                InlineHint {
                    key: "Esc",
                    action: "Back",
                    priority: 3,
                },
            ],
        ),
        TuiMode::MilestoneView => (
            "Milestones",
            FKeyVis::Minimal,
            &[
                InlineHint {
                    key: "Enter",
                    action: "Select",
                    priority: 0,
                },
                InlineHint {
                    key: "i",
                    action: "Issues",
                    priority: 1,
                },
                InlineHint {
                    key: "Esc",
                    action: "Back",
                    priority: 2,
                },
            ],
        ),
        TuiMode::PromptInput => (
            "Prompt Input",
            FKeyVis::Minimal,
            &[
                InlineHint {
                    key: "Enter",
                    action: "Submit",
                    priority: 0,
                },
                InlineHint {
                    key: "Ctrl+J",
                    action: "Newline",
                    priority: 1,
                },
                InlineHint {
                    key: "Esc",
                    action: "Cancel",
                    priority: 2,
                },
            ],
        ),
        TuiMode::Settings => (
            "Settings",
            FKeyVis::Minimal,
            &[
                InlineHint {
                    key: "Tab",
                    action: "Next",
                    priority: 0,
                },
                InlineHint {
                    key: "Enter",
                    action: "Toggle",
                    priority: 1,
                },
                InlineHint {
                    key: "Esc",
                    action: "Back",
                    priority: 2,
                },
            ],
        ),
        TuiMode::Interaction => (
            "Interaction",
            FKeyVis::Minimal,
            &[
                InlineHint {
                    key: "Esc",
                    action: "Back",
                    priority: 0,
                },
                InlineHint {
                    key: "Up/Down",
                    action: "Scroll",
                    priority: 1,
                },
            ],
        ),
        TuiMode::DependencyGraph => (
            "Dependencies",
            FKeyVis::DashboardLike,
            &[
                InlineHint {
                    key: "Esc",
                    action: "Back",
                    priority: 0,
                },
                InlineHint {
                    key: "Tab",
                    action: "Cycle Views",
                    priority: 1,
                },
            ],
        ),
        TuiMode::CompletionSummary => (
            "Completion Summary",
            FKeyVis::Minimal,
            &[
                InlineHint {
                    key: "o",
                    action: "Open PR",
                    priority: 0,
                },
                InlineHint {
                    key: "s",
                    action: "Dismiss",
                    priority: 1,
                },
                InlineHint {
                    key: "i",
                    action: "Browse",
                    priority: 2,
                },
                InlineHint {
                    key: "r",
                    action: "New Prompt",
                    priority: 3,
                },
                InlineHint {
                    key: "l",
                    action: "Logs",
                    priority: 4,
                },
                InlineHint {
                    key: "d",
                    action: "Dashboard",
                    priority: 5,
                },
                InlineHint {
                    key: "q",
                    action: "Quit",
                    priority: 6,
                },
            ],
        ),
        TuiMode::Session {
            view: SessionView::Events,
            ..
        } => (
            "Call Log",
            FKeyVis::Minimal,
            &[
                InlineHint {
                    key: "Esc",
                    action: "Back",
                    priority: 0,
                },
                InlineHint {
                    key: "Enter",
                    action: "Expand",
                    priority: 1,
                },
                InlineHint {
                    key: "j/k",
                    action: "Move",
                    priority: 2,
                },
                InlineHint {
                    key: "g/G",
                    action: "Top/Bot",
                    priority: 3,
                },
            ],
        ),
        TuiMode::Session {
            view: SessionView::Log,
            ..
        } => (
            "Log Viewer",
            FKeyVis::Minimal,
            &[], // hints: `registry::footer_hints`
        ),
        TuiMode::SessionSummary => (
            "Session Summary",
            FKeyVis::Minimal,
            &[], // hints: `registry::footer_hints`
        ),
        TuiMode::SessionSwitcher => (
            "Session Switcher",
            FKeyVis::Minimal,
            &[
                InlineHint {
                    key: "Enter",
                    action: "Select",
                    priority: 0,
                },
                InlineHint {
                    key: "Esc",
                    action: "Cancel",
                    priority: 1,
                },
            ],
        ),
        TuiMode::ConfirmKill(_) => (
            "Confirm Kill",
            FKeyVis::Minimal,
            &[], // hints: `registry::footer_hints` (#1261)
        ),
        TuiMode::ConfirmExit => (
            "Confirm Exit",
            FKeyVis::Minimal,
            &[
                InlineHint {
                    key: "y",
                    action: "Yes",
                    priority: 0,
                },
                InlineHint {
                    key: "n",
                    action: "No",
                    priority: 1,
                },
                InlineHint {
                    key: "Esc",
                    action: "Cancel",
                    priority: 2,
                },
            ],
        ),
        TuiMode::QueueConfirmation => (
            "Queue Confirmation",
            FKeyVis::Minimal,
            &[
                InlineHint {
                    key: "Enter",
                    action: "Confirm",
                    priority: 0,
                },
                InlineHint {
                    key: "Esc",
                    action: "Cancel",
                    priority: 1,
                },
            ],
        ),
        // #1287: the run loop. `h` / `S` and the card keys are drawn in
        // its Run bar and card, only while they work.
        TuiMode::QueueExecution => (
            "Queue Execution",
            FKeyVis::Minimal,
            &[InlineHint {
                key: "Esc",
                action: "Back · keeps running",
                priority: 0,
            }],
        ),
        // #1287: the hollow decision card (Enter confirms the pick).
        TuiMode::HollowRetry => (
            "Hollow Retry",
            FKeyVis::Minimal,
            &[
                InlineHint {
                    key: "↑↓",
                    action: "Pick",
                    priority: 0,
                },
                InlineHint {
                    key: "Enter",
                    action: "Confirm",
                    priority: 1,
                },
                InlineHint {
                    key: "Esc",
                    action: "Later",
                    priority: 2,
                },
            ],
        ),
        TuiMode::ContinuousPause => (
            "Continuous Pause",
            FKeyVis::Minimal,
            &[
                InlineHint {
                    key: "↑↓",
                    action: "Pick",
                    priority: 0,
                },
                InlineHint {
                    key: "Enter",
                    action: "Confirm",
                    priority: 1,
                },
            ],
        ),
        TuiMode::AdaptWizard => (
            "Adapt Wizard",
            FKeyVis::Minimal,
            &[
                InlineHint {
                    key: "Enter",
                    action: "Next",
                    priority: 0,
                },
                InlineHint {
                    key: "Esc",
                    action: "Back",
                    priority: 1,
                },
            ],
        ),
        TuiMode::PrReview => (
            "PR Review",
            FKeyVis::Minimal,
            &[
                InlineHint {
                    key: "Enter",
                    action: "Select",
                    priority: 0,
                },
                InlineHint {
                    key: "Esc",
                    action: "Back",
                    priority: 1,
                },
            ],
        ),
        TuiMode::ReleaseNotes => (
            "Release Notes",
            FKeyVis::Minimal,
            &[InlineHint {
                key: "Esc",
                action: "Back",
                priority: 0,
            }],
        ),
        TuiMode::Sanitize => (
            "Code health",
            FKeyVis::Minimal,
            &[InlineHint {
                key: "Esc",
                action: "Back",
                priority: 0,
            }],
        ),
        TuiMode::AdaptFollowUp => (
            "Next iteration paths",
            FKeyVis::Minimal,
            &[
                InlineHint {
                    key: "Enter",
                    action: "Execute",
                    priority: 0,
                },
                InlineHint {
                    key: "Esc",
                    action: "Dismiss",
                    priority: 1,
                },
            ],
        ),
        TuiMode::Landing => (
            "Home",
            FKeyVis::Minimal,
            &[
                InlineHint {
                    key: "Enter",
                    action: "Open",
                    priority: 0,
                },
                InlineHint {
                    key: "j/k",
                    action: "Move",
                    priority: 1,
                },
                InlineHint {
                    key: "Tab",
                    action: "Next panel",
                    priority: 2,
                },
            ],
        ),
        TuiMode::IssueWizard => ("New Issue", FKeyVis::Minimal, &[]),
        TuiMode::MilestoneWizard => ("New Milestone", FKeyVis::Minimal, &[]),
        TuiMode::ProjectStats => ("Project Stats", FKeyVis::Minimal, &[]),
        TuiMode::Prd => ("PRD", FKeyVis::Minimal, &[]),
        TuiMode::Roadmap => ("Roadmap", FKeyVis::Minimal, &[]),
        TuiMode::BypassWarning => ("Bypass Warning", FKeyVis::Minimal, &[]),
        TuiMode::MilestoneHealth => ("Milestone Health", FKeyVis::Minimal, &[]),
        TuiMode::Session {
            view: SessionView::Gates,
            ..
        } => (
            "Gate Output",
            FKeyVis::Minimal,
            &[
                InlineHint {
                    key: "Esc",
                    action: "Back",
                    priority: 0,
                },
                InlineHint {
                    key: "j/k",
                    action: "Scroll",
                    priority: 1,
                },
                InlineHint {
                    key: "G",
                    action: "Bottom",
                    priority: 2,
                },
                InlineHint {
                    key: "g",
                    action: "Top",
                    priority: 3,
                },
            ],
        ),
        TuiMode::CiErrorReview => (
            "CI Error Review",
            FKeyVis::Minimal,
            &[
                InlineHint {
                    key: "Enter",
                    action: "Launch fix",
                    priority: 0,
                },
                InlineHint {
                    key: "Esc",
                    action: "Abort",
                    priority: 1,
                },
                InlineHint {
                    key: "j/k",
                    action: "Scroll",
                    priority: 2,
                },
            ],
        ),
        TuiMode::TeamWizard => (
            "Teams",
            FKeyVis::Minimal,
            &[
                InlineHint {
                    key: "c",
                    action: "Compose",
                    priority: 0,
                },
                InlineHint {
                    key: "l",
                    action: "Launch",
                    priority: 1,
                },
                InlineHint {
                    key: "m",
                    action: "Manage",
                    priority: 2,
                },
                InlineHint {
                    key: "Enter",
                    action: "Confirm",
                    priority: 3,
                },
                InlineHint {
                    key: "Esc",
                    action: "Back",
                    priority: 4,
                },
            ],
        ),
        TuiMode::FleetBoard => (
            "Fleet",
            FKeyVis::Minimal,
            &[
                InlineHint {
                    key: "↑/↓",
                    action: "Select",
                    priority: 0,
                },
                InlineHint {
                    key: "Enter",
                    action: "Open",
                    priority: 1,
                },
                InlineHint {
                    key: "Tab",
                    action: "Sort",
                    priority: 2,
                },
                InlineHint {
                    key: "Esc",
                    action: "Back",
                    priority: 3,
                },
            ],
        ),
        TuiMode::AdoptPicker => (
            "Adopt",
            FKeyVis::Minimal,
            &[
                InlineHint {
                    key: "↑/↓",
                    action: "Select",
                    priority: 0,
                },
                InlineHint {
                    key: "Enter",
                    action: "Adopt",
                    priority: 1,
                },
                InlineHint {
                    key: "Esc",
                    action: "Back",
                    priority: 2,
                },
            ],
        ),
        TuiMode::BudgetPreSpawn { .. } => (
            "Budget gate",
            FKeyVis::Minimal,
            &[
                InlineHint {
                    key: "S",
                    action: "spawn",
                    priority: 0,
                },
                InlineHint {
                    key: "b",
                    action: "raise cap",
                    priority: 1,
                },
                InlineHint {
                    key: "n",
                    action: "drop",
                    priority: 2,
                },
            ],
        ),
    };
    // #1248: a screen registered in the keymap registry takes its footer
    // hints from there.
    let hints = super::registry::footer_hints(mode).unwrap_or(hints);

    let fkeys = build_fkeys(fkey_vis, has_session, is_running, is_terminal);

    let mut help_groups = Vec::new();
    if !screen_bindings.is_empty() {
        help_groups.extend_from_slice(screen_bindings);
    }
    help_groups.extend_from_slice(global_keybindings());
    help_groups.extend(super::keymap::mode_help_groups(mode));

    ModeKeyMap {
        mode_label,
        fkeys,
        hints,
        help_groups,
    }
}

enum FKeyVis {
    SessionAware,
    DashboardLike,
    Minimal,
}

fn build_fkeys(
    vis: FKeyVis,
    has_session: bool,
    is_running: bool,
    is_terminal: bool,
) -> Vec<FKeyRelevance> {
    // Each FKey entry declares key + label + action together. Adding a
    // new F-key means one `FKeyRelevance::new(...)` here — the dispatch
    // path in `input_handler::dispatch_fkey_action` reads `action` from
    // the same struct, so the bar label and handler cannot drift.
    use FKeyAction::*;
    match vis {
        FKeyVis::SessionAware => vec![
            FKeyRelevance::new("F1", "Help", ToggleHelp),
            FKeyRelevance::new("F2", "Summary", OpenSummary),
            FKeyRelevance::new("F3", "Full", OpenFullscreenSelected).with_active(has_session),
            FKeyRelevance::new("F4", "Costs", OpenCostDashboard),
            FKeyRelevance::new("F5", "Tokens", OpenTokenDashboard),
            FKeyRelevance::new("F6", "Deps", OpenDependencyGraph),
            FKeyRelevance::new("F9", "Pause", PauseAll).with_active(is_running),
            FKeyRelevance::new("F10", "Kill", KillSelected)
                .with_active(has_session && !is_terminal),
            FKeyRelevance::new("^X", "Exit", Exit),
        ],
        FKeyVis::DashboardLike => vec![
            FKeyRelevance::new("F1", "Help", ToggleHelp),
            FKeyRelevance::new("F4", "Costs", OpenCostDashboard),
            FKeyRelevance::new("F5", "Tokens", OpenTokenDashboard),
            FKeyRelevance::new("F6", "Deps", OpenDependencyGraph),
            FKeyRelevance::new("^X", "Exit", Exit),
        ],
        FKeyVis::Minimal => vec![
            FKeyRelevance::new("F1", "Help", ToggleHelp),
            FKeyRelevance::new("^X", "Exit", Exit),
        ],
    }
}
