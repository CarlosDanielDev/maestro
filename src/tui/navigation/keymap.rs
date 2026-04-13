use crate::session::types::SessionStatus;
use crate::tui::app::TuiMode;

/// A single keybinding declaration for documentation and help overlay.
#[derive(Debug, Clone)]
pub struct KeyBinding {
    /// Human-readable key label (e.g., "j/Down", "Tab", "Ctrl+s").
    pub key: &'static str,
    /// Short description of what this binding does.
    pub description: &'static str,
}

/// A group of keybindings with a section label, for help overlay rendering.
#[derive(Debug, Clone)]
pub struct KeyBindingGroup {
    pub title: &'static str,
    pub bindings: Vec<KeyBinding>,
}

/// Trait for screens/components that declare their keybindings.
/// This drives the context-sensitive help overlay.
pub trait KeymapProvider {
    fn keybindings(&self) -> Vec<KeyBindingGroup>;
}

/// Return the global keybindings that apply regardless of active screen.
pub fn global_keybindings() -> Vec<KeyBindingGroup> {
    vec![
        KeyBindingGroup {
            title: "Navigation",
            bindings: vec![
                KeyBinding {
                    key: "Tab",
                    description: "Cycle focus between panes",
                },
                KeyBinding {
                    key: "Esc",
                    description: "Return to previous screen / Close help",
                },
                KeyBinding {
                    key: "Enter",
                    description: "Open detail / Execute action",
                },
                KeyBinding {
                    key: "1-9",
                    description: "Jump to session detail by index",
                },
                KeyBinding {
                    key: "? / F1",
                    description: "Toggle help overlay",
                },
            ],
        },
        KeyBindingGroup {
            title: "Views",
            bindings: vec![
                KeyBinding {
                    key: "f / F3",
                    description: "Full-screen view for selected session",
                },
                KeyBinding {
                    key: "$ / F4",
                    description: "Cost dashboard view",
                },
                KeyBinding {
                    key: "t / F5",
                    description: "Token dashboard view",
                },
                KeyBinding {
                    key: "Tab / F6",
                    description: "Cycle view mode",
                },
            ],
        },
        KeyBindingGroup {
            title: "Session Control",
            bindings: vec![
                KeyBinding {
                    key: "p / F9",
                    description: "Pause all running sessions (SIGSTOP)",
                },
                KeyBinding {
                    key: "r",
                    description: "Resume all paused sessions (SIGCONT)",
                },
                KeyBinding {
                    key: "k / F10",
                    description: "Kill selected session",
                },
                KeyBinding {
                    key: "d",
                    description: "Dismiss notification banner",
                },
            ],
        },
        KeyBindingGroup {
            title: "Scrolling",
            bindings: vec![
                KeyBinding {
                    key: "Up/Down",
                    description: "Scroll agent panel output",
                },
                KeyBinding {
                    key: "Shift+Up/Down",
                    description: "Scroll activity log",
                },
                KeyBinding {
                    key: "Mouse wheel",
                    description: "Scroll focused panel",
                },
            ],
        },
        KeyBindingGroup {
            title: "General",
            bindings: vec![
                KeyBinding {
                    key: "S / F2",
                    description: "Session summary",
                },
                KeyBinding {
                    key: "q / Ctrl+c / ^X",
                    description: "Quit maestro",
                },
            ],
        },
    ]
}

#[derive(Debug, Clone)]
pub struct FKeyRelevance {
    pub key: &'static str,
    pub label: &'static str,
    pub visible: bool,
    pub active: bool,
}

#[derive(Debug, Clone)]
pub struct InlineHint {
    pub key: &'static str,
    pub action: &'static str,
    /// Lower = shown first when truncating at narrow widths.
    pub priority: u8,
}

#[derive(Debug, Clone)]
pub struct ModeKeyMap {
    pub mode_label: &'static str,
    pub fkeys: Vec<FKeyRelevance>,
    pub hints: Vec<InlineHint>,
    pub help_groups: Vec<KeyBindingGroup>,
}

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

    let (mode_label, fkey_vis, hints) = match mode {
        TuiMode::Overview => (
            "Overview",
            FKeyVis::SessionAware,
            vec![
                InlineHint {
                    key: "Enter",
                    action: "Detail",
                    priority: 0,
                },
                InlineHint {
                    key: "d",
                    action: "Dismiss",
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
            vec![
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
            vec![
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
            vec![
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
            vec![
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
            vec![
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
            vec![
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
            vec![
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
            vec![
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
            vec![
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
            vec![
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
            vec![
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
            vec![
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
            vec![
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
            vec![
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
            vec![
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
        TuiMode::QueueConfirmation => (
            "Queue Confirmation",
            FKeyVis::Minimal,
            vec![
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
            vec![
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
            vec![
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
            vec![
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
            vec![
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
            vec![
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
            vec![InlineHint {
                key: "Esc",
                action: "Back",
                priority: 0,
            }],
        ),
        TuiMode::Sanitize => (
            "Sanitize",
            FKeyVis::Minimal,
            vec![InlineHint {
                key: "Esc",
                action: "Back",
                priority: 0,
            }],
        ),
    };

    let fkeys = build_fkeys(fkey_vis, has_session, is_running, is_terminal);

    let mut help_groups = Vec::new();
    if !screen_bindings.is_empty() {
        help_groups.extend_from_slice(screen_bindings);
    }
    help_groups.extend(global_keybindings());

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

/// Fit visible F-key entries to the given terminal width.
/// Returns entries that fit; if width < 40, labels are dropped.
pub fn fit_fkeys_to_width(fkeys: &[FKeyRelevance], width: u16) -> Vec<(&str, Option<&str>, bool)> {
    let mut result = Vec::new();
    let mut used = 0u16;

    for entry in fkeys.iter().filter(|e| e.visible) {
        let entry_width = if width < 40 {
            entry.key.len() as u16 + 1
        } else {
            entry.key.len() as u16 + 1 + entry.label.len() as u16 + 2
        };

        if used + entry_width > width {
            break;
        }

        let label = if width < 40 { None } else { Some(entry.label) };
        result.push((entry.key, label, entry.active));
        used += entry_width;
    }

    result
}

/// Fit inline hints to the given terminal width.
/// Returns hints that fit within the given width.
/// Hints are assumed to be pre-sorted by priority (lower = first).
pub fn fit_hints_to_width(hints: &[InlineHint], width: u16) -> Vec<(&str, &str)> {
    let mut result = Vec::new();
    let mut used = 0u16;

    for hint in hints {
        let entry_width = hint.key.len() as u16 + hint.action.len() as u16 + 4;
        if used + entry_width > width {
            break;
        }
        if !result.is_empty() {
            used += 2;
        }
        result.push((hint.key, hint.action));
        used += entry_width;
    }

    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn key_binding_has_correct_key_and_description() {
        let binding = KeyBinding {
            key: "Tab",
            description: "Cycle focus",
        };
        assert_eq!(binding.key, "Tab");
        assert_eq!(binding.description, "Cycle focus");
    }

    #[test]
    fn key_binding_group_contains_all_provided_bindings() {
        let group = KeyBindingGroup {
            title: "Navigation",
            bindings: vec![
                KeyBinding {
                    key: "j",
                    description: "Down",
                },
                KeyBinding {
                    key: "k",
                    description: "Up",
                },
            ],
        };
        assert_eq!(group.bindings.len(), 2);
        assert_eq!(group.title, "Navigation");
    }

    #[test]
    fn global_keybindings_returns_non_empty_list() {
        let groups = global_keybindings();
        assert!(!groups.is_empty());
    }

    #[test]
    fn global_keybindings_includes_quit_binding() {
        let groups = global_keybindings();
        let all_bindings: Vec<&KeyBinding> =
            groups.iter().flat_map(|g| g.bindings.iter()).collect();
        let has_quit = all_bindings
            .iter()
            .any(|b| b.description.to_lowercase().contains("quit"));
        assert!(has_quit);
    }

    #[test]
    fn global_keybindings_includes_tab_for_focus_cycling() {
        let groups = global_keybindings();
        let all_bindings: Vec<&KeyBinding> =
            groups.iter().flat_map(|g| g.bindings.iter()).collect();
        let has_tab = all_bindings.iter().any(|b| b.key.contains("Tab"));
        assert!(has_tab);
    }

    #[test]
    fn global_keybindings_include_fkey_labels() {
        let groups = global_keybindings();
        let all_keys: Vec<&str> = groups
            .iter()
            .flat_map(|g| g.bindings.iter())
            .map(|b| b.key)
            .collect();
        assert!(
            all_keys.iter().any(|k| k.contains("F1")),
            "expected F1 in global keybindings, got: {:?}",
            all_keys
        );
    }

    #[test]
    fn keymapprovider_implementor_returns_groups() {
        struct MyScreen;
        impl KeymapProvider for MyScreen {
            fn keybindings(&self) -> Vec<KeyBindingGroup> {
                vec![KeyBindingGroup {
                    title: "Test",
                    bindings: vec![KeyBinding {
                        key: "x",
                        description: "Do X",
                    }],
                }]
            }
        }
        let screen = MyScreen;
        let groups = screen.keybindings();
        assert_eq!(groups.len(), 1);
        assert_eq!(groups[0].bindings[0].key, "x");
    }

    // --- Issue #280: mode_keymap tests ---

    #[test]
    fn mode_keymap_overview_shows_all_fkeys() {
        let km = mode_keymap(TuiMode::Overview, Some(SessionStatus::Running), &[]);
        assert_eq!(km.mode_label, "Overview");
        assert_eq!(km.fkeys.len(), 9);
        assert!(km.fkeys.iter().all(|f| f.visible));
    }

    #[test]
    fn mode_keymap_overview_running_session_activates_pause_kill() {
        let km = mode_keymap(TuiMode::Overview, Some(SessionStatus::Running), &[]);
        let f9 = km.fkeys.iter().find(|f| f.key == "F9").unwrap();
        assert!(f9.active, "F9 Pause should be active when running");
        let f10 = km.fkeys.iter().find(|f| f.key == "F10").unwrap();
        assert!(f10.active, "F10 Kill should be active when running");
    }

    #[test]
    fn mode_keymap_overview_no_session_dims_session_keys() {
        let km = mode_keymap(TuiMode::Overview, None, &[]);
        let f3 = km.fkeys.iter().find(|f| f.key == "F3").unwrap();
        assert!(!f3.active, "F3 Full should be inactive without session");
        let f9 = km.fkeys.iter().find(|f| f.key == "F9").unwrap();
        assert!(!f9.active, "F9 Pause should be inactive without session");
        let f10 = km.fkeys.iter().find(|f| f.key == "F10").unwrap();
        assert!(!f10.active, "F10 Kill should be inactive without session");
    }

    #[test]
    fn mode_keymap_overview_completed_session_dims_pause_kill() {
        let km = mode_keymap(TuiMode::Overview, Some(SessionStatus::Completed), &[]);
        let f9 = km.fkeys.iter().find(|f| f.key == "F9").unwrap();
        assert!(
            !f9.active,
            "F9 Pause should be inactive for completed session"
        );
        let f10 = km.fkeys.iter().find(|f| f.key == "F10").unwrap();
        assert!(
            !f10.active,
            "F10 Kill should be inactive for completed session"
        );
    }

    #[test]
    fn mode_keymap_dashboard_shows_reduced_fkeys() {
        let km = mode_keymap(TuiMode::Dashboard, None, &[]);
        assert_eq!(km.mode_label, "Dashboard");
        assert_eq!(km.fkeys.len(), 5);
        let keys: Vec<&str> = km.fkeys.iter().map(|f| f.key).collect();
        assert!(keys.contains(&"F1"));
        assert!(keys.contains(&"F4"));
        assert!(keys.contains(&"F5"));
        assert!(keys.contains(&"F6"));
        assert!(keys.contains(&"^X"));
        assert!(!keys.contains(&"F9"));
        assert!(!keys.contains(&"F10"));
    }

    #[test]
    fn mode_keymap_settings_shows_minimal_fkeys() {
        let km = mode_keymap(TuiMode::Settings, None, &[]);
        assert_eq!(km.fkeys.len(), 2);
        assert_eq!(km.fkeys[0].key, "F1");
        assert_eq!(km.fkeys[1].key, "^X");
    }

    #[test]
    fn mode_keymap_prompt_input_shows_minimal_fkeys() {
        let km = mode_keymap(TuiMode::PromptInput, None, &[]);
        assert_eq!(km.fkeys.len(), 2);
        assert_eq!(km.fkeys[0].key, "F1");
        assert_eq!(km.fkeys[1].key, "^X");
    }

    #[test]
    fn mode_keymap_includes_screen_bindings_in_help_groups() {
        let screen_bindings = vec![KeyBindingGroup {
            title: "Screen Actions",
            bindings: vec![KeyBinding {
                key: "x",
                description: "Do X",
            }],
        }];
        let km = mode_keymap(TuiMode::Dashboard, None, &screen_bindings);
        assert_eq!(km.help_groups[0].title, "Screen Actions");
        // Global keybindings follow screen bindings
        assert!(km.help_groups.len() > 1);
    }

    #[test]
    fn mode_keymap_help_groups_include_global_keybindings() {
        let km = mode_keymap(TuiMode::Overview, None, &[]);
        let global_titles: Vec<&str> = global_keybindings().iter().map(|g| g.title).collect();
        let km_titles: Vec<&str> = km.help_groups.iter().map(|g| g.title).collect();
        for title in &global_titles {
            assert!(km_titles.contains(title), "missing global group: {}", title);
        }
    }

    #[test]
    fn mode_keymap_overview_has_hints() {
        let km = mode_keymap(TuiMode::Overview, None, &[]);
        assert!(!km.hints.is_empty());
        assert_eq!(km.hints[0].key, "Enter");
        assert_eq!(km.hints[0].action, "Detail");
    }

    #[test]
    fn mode_keymap_dashboard_has_hints() {
        let km = mode_keymap(TuiMode::Dashboard, None, &[]);
        let keys: Vec<&str> = km.hints.iter().map(|h| h.key).collect();
        assert!(keys.contains(&"i"), "Dashboard should have Issues hint");
        assert!(keys.contains(&"r"), "Dashboard should have Prompt hint");
    }

    // --- fit_fkeys_to_width tests ---

    #[test]
    fn fit_fkeys_full_width_includes_all() {
        let km = mode_keymap(TuiMode::Overview, Some(SessionStatus::Running), &[]);
        let fitted = fit_fkeys_to_width(&km.fkeys, 120);
        assert_eq!(fitted.len(), 9);
        for (_, label, _) in &fitted {
            assert!(label.is_some());
        }
    }

    #[test]
    fn fit_fkeys_narrow_drops_labels() {
        let km = mode_keymap(TuiMode::Overview, None, &[]);
        let fitted = fit_fkeys_to_width(&km.fkeys, 35);
        for (_, label, _) in &fitted {
            assert!(label.is_none(), "labels should be None when width < 40");
        }
    }

    #[test]
    fn fit_fkeys_very_narrow_truncates() {
        let km = mode_keymap(TuiMode::Overview, None, &[]);
        let fitted = fit_fkeys_to_width(&km.fkeys, 20);
        assert!(!fitted.is_empty());
        assert!(fitted.len() < 9);
    }

    #[test]
    fn fit_fkeys_dashboard_fewer_entries() {
        let km = mode_keymap(TuiMode::Dashboard, None, &[]);
        let fitted = fit_fkeys_to_width(&km.fkeys, 120);
        assert_eq!(fitted.len(), 5);
    }

    #[test]
    fn fit_fkeys_settings_only_two_entries() {
        let km = mode_keymap(TuiMode::Settings, None, &[]);
        let fitted = fit_fkeys_to_width(&km.fkeys, 120);
        assert_eq!(fitted.len(), 2);
    }

    // --- fit_hints_to_width tests ---

    #[test]
    fn fit_hints_wide_includes_all() {
        let km = mode_keymap(TuiMode::Overview, None, &[]);
        let fitted = fit_hints_to_width(&km.hints, 120);
        assert_eq!(fitted.len(), km.hints.len());
    }

    #[test]
    fn fit_hints_narrow_truncates_gracefully() {
        let km = mode_keymap(TuiMode::Overview, None, &[]);
        let fitted = fit_hints_to_width(&km.hints, 30);
        assert!(!fitted.is_empty());
        assert!(fitted.len() < km.hints.len());
    }

    #[test]
    fn fit_hints_sorted_by_priority() {
        let km = mode_keymap(TuiMode::Overview, None, &[]);
        let fitted = fit_hints_to_width(&km.hints, 120);
        // First hint should be highest priority (lowest number)
        assert_eq!(fitted[0].0, "Enter");
    }

    #[test]
    fn fit_hints_empty_at_zero_width() {
        let km = mode_keymap(TuiMode::Overview, None, &[]);
        let fitted = fit_hints_to_width(&km.hints, 0);
        assert!(fitted.is_empty());
    }
}
