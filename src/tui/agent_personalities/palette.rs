//! Per-role color and ASCII abbreviation — spike prototype.
//!
//! See `docs/adr/002-agent-personalities.md` § Role Taxonomy and § ASCII
//! Fallback. Keep this file as a single-source-of-truth so a color drift is
//! a one-line edit.

use ratatui::style::Color;

use super::role::Role;

/// Foreground color for the role's sprite.
pub fn role_color(role: Role) -> Color {
    match role {
        Role::Implementer => Color::Green,
        Role::Orchestrator => Color::Yellow,
        Role::Reviewer => Color::Magenta,
        // xterm 256-color orange. ANSI 16-color set has no orange; this index
        // is widely supported in modern terminals and degrades to yellow on
        // 16-color terminals (acceptable since `Docs` and `Orchestrator` only
        // collide on truly ancient terminals where the user's also forced into
        // ASCII fallback anyway).
        Role::Docs => Color::Indexed(208),
        Role::DevOps => Color::Red,
    }
}

/// Three-character ASCII abbreviation, used in the `use_nerd_font() == false`
/// fallback. See ADR § ASCII Fallback for the verdict.
pub fn role_abbrev(role: Role) -> &'static str {
    match role {
        Role::Implementer => "IMP",
        Role::Orchestrator => "ORC",
        Role::Reviewer => "REV",
        Role::Docs => "DOC",
        Role::DevOps => "OPS",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_role_has_a_three_char_abbrev() {
        for role in [
            Role::Implementer,
            Role::Orchestrator,
            Role::Reviewer,
            Role::Docs,
            Role::DevOps,
        ] {
            let abbrev = role_abbrev(role);
            assert_eq!(abbrev.len(), 3, "abbrev for {:?} is not 3 chars", role);
            assert!(
                abbrev.chars().all(|c| c.is_ascii_uppercase()),
                "abbrev for {:?} is not all-uppercase ASCII",
                role
            );
        }
    }

    #[test]
    fn abbreviations_are_unique_across_roles() {
        let abbrevs = [
            role_abbrev(Role::Orchestrator),
            role_abbrev(Role::Implementer),
            role_abbrev(Role::Reviewer),
            role_abbrev(Role::Docs),
            role_abbrev(Role::DevOps),
        ];
        let mut seen = std::collections::HashSet::new();
        for a in abbrevs {
            assert!(seen.insert(a), "duplicate abbreviation: {}", a);
        }
    }

    #[test]
    fn colors_are_distinct_across_roles() {
        let colors = [
            role_color(Role::Orchestrator),
            role_color(Role::Implementer),
            role_color(Role::Reviewer),
            role_color(Role::Docs),
            role_color(Role::DevOps),
        ];
        let mut seen = std::collections::HashSet::new();
        for c in colors {
            // ratatui Color implements Hash; HashSet round-trip catches duplicates.
            assert!(seen.insert(format!("{:?}", c)), "duplicate color: {:?}", c);
        }
    }
}
