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
}
