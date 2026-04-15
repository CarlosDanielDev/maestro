use crate::session::types::SessionStatus;
use crate::tui::app::TuiMode;

use super::keymap::{FKeyRelevance, InlineHint, KeyBindingGroup, ModeKeyMap, global_keybindings};

/// Build the `ModeKeyMap` for a given `TuiMode`.
///
/// `screen_bindings` should come from the active screen's `KeymapProvider::keybindings()`
/// (empty slice for modes that don't implement `Screen`).
pub fn mode_keymap(
    mode: TuiMode,
    session_status: Option<SessionStatus>,
    screen_bindings: &[KeyBindingGroup],
) -> ModeKeyMap {
    let has_session = session_status.is_some();
    let is_terminal = session_status.is_some_and(|s| s.is_terminal());
    let is_running = matches!(session_status, Some(SessionStatus::Running));

    let (mode_label, fkey_vis, hints): (&str, FKeyVis, &[InlineHint]) = match mode {
        TuiMode::Overview => (
            "Overview",
            FKeyVis::SessionAware,
            &[
                InlineHint {
                    key: "Enter",
                    action: "Detail",
                    priority: 0,
                },
                InlineHint {
                    key: "d",
                    action: "Log",
                    priority: 1,
                },
                InlineHint {
                    key: "f",
                    action: "Full",
                    priority: 2,
                },
                InlineHint {
                    key: "w",
                    action: "Switcher",
                    priority: 3,
                },
                InlineHint {
                    key: "Tab",
                    action: "Cycle Views",
                    priority: 4,
                },
            ],
        ),
        TuiMode::Detail(_) => (
            "Detail",
            FKeyVis::SessionAware,
            &[
                InlineHint {
                    key: "Esc",
                    action: "Back",
                    priority: 0,
                },
                InlineHint {
                    key: "f",
                    action: "Full",
                    priority: 1,
                },
                InlineHint {
                    key: "l",
                    action: "Logs",
                    priority: 2,
                },
                InlineHint {
                    key: "k",
                    action: "Kill",
                    priority: 3,
                },
            ],
        ),
        TuiMode::Fullscreen(_) => (
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
            ],
        ),
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
                    action: "Settings",
                    priority: 3,
                },
                InlineHint {
                    key: "a",
                    action: "Adapt",
                    priority: 4,
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
        TuiMode::CostDashboard => (
            "Cost Dashboard",
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
        TuiMode::TokenDashboard => (
            "Token Dashboard",
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
                    key: "i",
                    action: "Browse",
                    priority: 0,
                },
                InlineHint {
                    key: "r",
                    action: "New Prompt",
                    priority: 1,
                },
                InlineHint {
                    key: "d",
                    action: "Dashboard",
                    priority: 2,
                },
                InlineHint {
                    key: "q",
                    action: "Quit",
                    priority: 3,
                },
            ],
        ),
        TuiMode::LogViewer(_) => (
            "Log Viewer",
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
        TuiMode::SessionSummary => (
            "Session Summary",
            FKeyVis::Minimal,
            &[
                InlineHint {
                    key: "Esc",
                    action: "Back",
                    priority: 0,
                },
                InlineHint {
                    key: "j/k",
                    action: "Navigate",
                    priority: 1,
                },
                InlineHint {
                    key: "Enter",
                    action: "Expand",
                    priority: 2,
                },
            ],
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
            &[
                InlineHint {
                    key: "y",
                    action: "Confirm",
                    priority: 0,
                },
                InlineHint {
                    key: "n",
                    action: "Cancel",
                    priority: 1,
                },
            ],
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
        TuiMode::QueueExecution => (
            "Queue Execution",
            FKeyVis::Minimal,
            &[
                InlineHint {
                    key: "Esc",
                    action: "Back",
                    priority: 0,
                },
                InlineHint {
                    key: "r",
                    action: "Retry",
                    priority: 1,
                },
                InlineHint {
                    key: "s",
                    action: "Skip",
                    priority: 2,
                },
            ],
        ),
        TuiMode::HollowRetry => (
            "Hollow Retry",
            FKeyVis::Minimal,
            &[
                InlineHint {
                    key: "Enter",
                    action: "Retry",
                    priority: 0,
                },
                InlineHint {
                    key: "Esc",
                    action: "Cancel",
                    priority: 1,
                },
            ],
        ),
        TuiMode::ContinuousPause => (
            "Continuous Pause",
            FKeyVis::Minimal,
            &[
                InlineHint {
                    key: "r",
                    action: "Retry",
                    priority: 0,
                },
                InlineHint {
                    key: "Esc",
                    action: "Back",
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
            "Sanitize",
            FKeyVis::Minimal,
            &[InlineHint {
                key: "Esc",
                action: "Back",
                priority: 0,
            }],
        ),
        TuiMode::TurboquantDashboard => (
            "TurboQuant A/B",
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
    };

    let fkeys = build_fkeys(fkey_vis, has_session, is_running, is_terminal);

    let mut help_groups = Vec::new();
    if !screen_bindings.is_empty() {
        help_groups.extend_from_slice(screen_bindings);
    }
    help_groups.extend_from_slice(global_keybindings());

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
    match vis {
        FKeyVis::SessionAware => vec![
            FKeyRelevance {
                key: "F1",
                label: "Help",
                visible: true,
                active: true,
            },
            FKeyRelevance {
                key: "F2",
                label: "Summary",
                visible: true,
                active: true,
            },
            FKeyRelevance {
                key: "F3",
                label: "Full",
                visible: true,
                active: has_session,
            },
            FKeyRelevance {
                key: "F4",
                label: "Costs",
                visible: true,
                active: true,
            },
            FKeyRelevance {
                key: "F5",
                label: "Tokens",
                visible: true,
                active: true,
            },
            FKeyRelevance {
                key: "F6",
                label: "Deps",
                visible: true,
                active: true,
            },
            FKeyRelevance {
                key: "F9",
                label: "Pause",
                visible: true,
                active: is_running,
            },
            FKeyRelevance {
                key: "F10",
                label: "Kill",
                visible: true,
                active: has_session && !is_terminal,
            },
            FKeyRelevance {
                key: "^X",
                label: "Exit",
                visible: true,
                active: true,
            },
        ],
        FKeyVis::DashboardLike => vec![
            FKeyRelevance {
                key: "F1",
                label: "Help",
                visible: true,
                active: true,
            },
            FKeyRelevance {
                key: "F4",
                label: "Costs",
                visible: true,
                active: true,
            },
            FKeyRelevance {
                key: "F5",
                label: "Tokens",
                visible: true,
                active: true,
            },
            FKeyRelevance {
                key: "F6",
                label: "Deps",
                visible: true,
                active: true,
            },
            FKeyRelevance {
                key: "^X",
                label: "Exit",
                visible: true,
                active: true,
            },
        ],
        FKeyVis::Minimal => vec![
            FKeyRelevance {
                key: "F1",
                label: "Help",
                visible: true,
                active: true,
            },
            FKeyRelevance {
                key: "^X",
                label: "Exit",
                visible: true,
                active: true,
            },
        ],
    }
}
