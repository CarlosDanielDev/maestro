pub mod focus;
pub mod keymap;

/// Vim-style input mode for the entire application.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub enum InputMode {
    /// Normal mode: single-key shortcuts navigate and execute commands.
    #[default]
    Normal,
    /// Insert mode: keystrokes go to a text input field (filter, prompt, etc.).
    Insert,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn input_mode_default_is_normal() {
        let mode = InputMode::default();
        assert_eq!(mode, InputMode::Normal);
    }

    #[test]
    fn input_mode_normal_and_insert_are_not_equal() {
        assert_ne!(InputMode::Normal, InputMode::Insert);
    }

    #[test]
    fn input_mode_clone_produces_equal_value() {
        let a = InputMode::Insert;
        let b = a;
        assert_eq!(a, b);
    }
}
