/// Mascot visual states — each maps to a distinct 6-row ASCII art set.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[allow(dead_code)]
pub enum MascotState {
    Idle,
    Conducting,
    Thinking,
    Happy,
    Sleeping,
    Error,
}

impl MascotState {
    /// Whether this state auto-transitions back to Idle after a duration.
    pub fn auto_revert_ms(&self) -> Option<u64> {
        match self {
            Self::Happy => Some(2000),
            _ => None,
        }
    }
}
