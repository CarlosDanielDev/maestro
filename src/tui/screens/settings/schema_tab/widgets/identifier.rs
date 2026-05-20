//! Identifier validation for dynamic map / rows entries.
//!
//! Format: `[a-z0-9][a-z0-9_-]{0,62}` (first char alphanumeric, no leading
//! hyphen/underscore). Length ≤ 63. Reserved tokens are also rejected so the
//! id does not collide with TOML-reserved or schema-key vocabulary.

pub(super) const RESERVED_IDENTIFIERS: &[&str] = &[
    "default", "entries", "extends", "kind", "enabled", "session", "global",
];

pub(super) const MAX_ID_LEN: usize = 63;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum IdentifierError {
    Empty,
    InvalidFormat,
    TooLong,
    Collision(String),
    Reserved(String),
}

impl IdentifierError {
    pub fn message(&self) -> String {
        match self {
            Self::Empty => "identifier cannot be empty".into(),
            Self::InvalidFormat => {
                "identifier must use a-z, 0-9, '-', '_' and start alphanumeric".into()
            }
            Self::TooLong => format!("identifier exceeds {} characters", MAX_ID_LEN),
            Self::Collision(id) => format!("identifier '{}' already exists", id),
            Self::Reserved(id) => format!("identifier '{}' is reserved", id),
        }
    }
}

impl std::fmt::Display for IdentifierError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.message())
    }
}

impl std::error::Error for IdentifierError {}

pub fn validate_identifier(candidate: &str, existing_ids: &[&str]) -> Result<(), IdentifierError> {
    if candidate.is_empty() {
        return Err(IdentifierError::Empty);
    }
    if candidate.len() > MAX_ID_LEN {
        return Err(IdentifierError::TooLong);
    }
    let mut bytes = candidate.bytes();
    let first = bytes.next().expect("non-empty checked above");
    if !is_alnum_byte(first) {
        return Err(IdentifierError::InvalidFormat);
    }
    for b in bytes {
        if !is_alnum_byte(b) && b != b'-' && b != b'_' {
            return Err(IdentifierError::InvalidFormat);
        }
    }
    if RESERVED_IDENTIFIERS.contains(&candidate) {
        return Err(IdentifierError::Reserved(candidate.to_string()));
    }
    if existing_ids.contains(&candidate) {
        return Err(IdentifierError::Collision(candidate.to_string()));
    }
    Ok(())
}

fn is_alnum_byte(b: u8) -> bool {
    b.is_ascii_lowercase() || b.is_ascii_digit()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_lowercase_alphanumeric() {
        assert!(validate_identifier("abc123", &[]).is_ok());
    }

    #[test]
    fn accepts_hyphen_and_underscore() {
        assert!(validate_identifier("my-agent_v2", &[]).is_ok());
    }

    #[test]
    fn accepts_single_char() {
        assert!(validate_identifier("a", &[]).is_ok());
    }

    #[test]
    fn rejects_empty_string() {
        assert_eq!(
            validate_identifier("", &[]).unwrap_err(),
            IdentifierError::Empty
        );
    }

    #[test]
    fn rejects_uppercase() {
        assert_eq!(
            validate_identifier("MyAgent", &[]).unwrap_err(),
            IdentifierError::InvalidFormat
        );
    }

    #[test]
    fn rejects_leading_hyphen() {
        assert_eq!(
            validate_identifier("-agent", &[]).unwrap_err(),
            IdentifierError::InvalidFormat
        );
    }

    #[test]
    fn rejects_trailing_hyphen_is_allowed_by_pattern() {
        // Trailing hyphen is allowed by [a-z0-9_-]+ pattern in spec.
        assert!(validate_identifier("agent-", &[]).is_ok());
    }

    #[test]
    fn rejects_leading_underscore() {
        assert_eq!(
            validate_identifier("_agent", &[]).unwrap_err(),
            IdentifierError::InvalidFormat
        );
    }

    #[test]
    fn rejects_space_in_middle() {
        assert_eq!(
            validate_identifier("my agent", &[]).unwrap_err(),
            IdentifierError::InvalidFormat
        );
    }

    #[test]
    fn rejects_dot() {
        assert_eq!(
            validate_identifier("my.agent", &[]).unwrap_err(),
            IdentifierError::InvalidFormat
        );
    }

    #[test]
    fn rejects_collision() {
        assert_eq!(
            validate_identifier("gpt4", &["gpt4", "claude"]).unwrap_err(),
            IdentifierError::Collision("gpt4".into())
        );
    }

    #[test]
    fn rejects_reserved_word_default() {
        assert_eq!(
            validate_identifier("default", &[]).unwrap_err(),
            IdentifierError::Reserved("default".into())
        );
    }

    #[test]
    fn rejects_reserved_word_kind() {
        assert_eq!(
            validate_identifier("kind", &[]).unwrap_err(),
            IdentifierError::Reserved("kind".into())
        );
    }

    #[test]
    fn rejects_too_long() {
        let candidate = "a".repeat(MAX_ID_LEN + 1);
        assert_eq!(
            validate_identifier(&candidate, &[]).unwrap_err(),
            IdentifierError::TooLong
        );
    }

    #[test]
    fn collision_substring_does_not_match() {
        assert!(validate_identifier("gpt", &["gpt4"]).is_ok());
    }

    #[test]
    fn case_sensitive_failure_is_format_first() {
        // Uppercase first fails on InvalidFormat before collision check.
        assert_eq!(
            validate_identifier("Agent", &["agent"]).unwrap_err(),
            IdentifierError::InvalidFormat
        );
    }

    #[test]
    fn message_collision_includes_id() {
        let err = IdentifierError::Collision("foo".into());
        assert!(err.message().contains("foo"));
        assert!(err.message().contains("already"));
    }

    #[test]
    fn message_reserved_includes_id() {
        let err = IdentifierError::Reserved("kind".into());
        assert!(err.message().contains("kind"));
        assert!(err.message().contains("reserved"));
    }
}
