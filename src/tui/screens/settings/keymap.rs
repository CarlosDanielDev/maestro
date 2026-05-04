use crate::tui::navigation::keymap::{KeyBinding, KeyBindingGroup, KeymapProvider};

use super::SettingsScreen;

impl KeymapProvider for SettingsScreen {
    fn keybindings(&self) -> Vec<KeyBindingGroup> {
        vec![
            KeyBindingGroup {
                title: "Navigation",
                bindings: vec![
                    KeyBinding {
                        key: "Tab/Shift+Tab",
                        description: "Switch tab",
                    },
                    KeyBinding {
                        key: "↑/↓ or j/k",
                        description: "Navigate fields",
                    },
                    KeyBinding {
                        key: "Esc",
                        description: "Back to Dashboard",
                    },
                ],
            },
            KeyBindingGroup {
                title: "Edit",
                bindings: vec![
                    KeyBinding {
                        key: "Space/Enter",
                        description: "Toggle or begin editing focused field",
                    },
                    KeyBinding {
                        key: "←/→ or h/l",
                        description: "Adjust dropdown / number",
                    },
                    KeyBinding {
                        key: "Enter",
                        description: "Edit text / list field",
                    },
                    KeyBinding {
                        key: "Space",
                        description: "Toggle caveman_mode (effective on next Claude session)",
                    },
                ],
            },
            KeyBindingGroup {
                title: "Actions",
                bindings: vec![
                    KeyBinding {
                        key: "Ctrl+s",
                        description: "Save changes",
                    },
                    KeyBinding {
                        key: "Ctrl+r",
                        description: "Reset all fields",
                    },
                ],
            },
        ]
    }
}
