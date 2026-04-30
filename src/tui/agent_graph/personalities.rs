//! Per-role color, abbreviation, and 6×6 sprite for agent-graph nodes.
//!
//! See `docs/adr/002-agent-personalities.md` for the design rationale
//! (§ Sprite Design Language, § Role Taxonomy, § ASCII Fallback).
//!
//! The compile-time `[[char; 6]; 6]` shape on `Sprite` enforces that every
//! sprite is exactly 36 cells, so the renderer treats every role identically
//! without per-role bounding-box logic.

use ratatui::style::Color;

use crate::session::role::Role;

/// A fixed-size 6×6 character grid.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Sprite([[char; 6]; 6]);

impl Sprite {
    pub(crate) fn rows(&self) -> &[[char; 6]; 6] {
        &self.0
    }
}

const ORCHESTRATOR: Sprite = Sprite([
    [' ', '◆', '█', '█', '◆', ' '],
    ['█', '█', '█', '█', '█', '█'],
    ['█', '●', '█', '█', '●', '█'],
    ['█', '█', '█', '█', '█', '█'],
    ['█', '█', '█', '█', '█', '█'],
    ['█', ' ', '█', '█', ' ', '█'],
]);

const IMPLEMENTER: Sprite = Sprite([
    [' ', ' ', '█', '█', ' ', ' '],
    [' ', '█', '█', '█', '█', ' '],
    ['█', '●', '█', '█', '●', '█'],
    ['█', '█', '█', '█', '█', '█'],
    ['█', '█', '█', '█', '█', '█'],
    ['█', ' ', '█', '█', ' ', '█'],
]);

const REVIEWER: Sprite = Sprite([
    [' ', ' ', '█', '█', ' ', ' '],
    [' ', '█', '█', '█', '█', ' '],
    ['█', '▓', '█', '█', '●', '█'],
    ['█', '█', '█', '█', '◆', '█'],
    ['█', '█', '█', '█', '█', '█'],
    ['█', ' ', '█', '█', ' ', '█'],
]);

const DOCS: Sprite = Sprite([
    [' ', ' ', '█', '█', ' ', ' '],
    [' ', '█', '█', '█', '█', ' '],
    ['█', '○', '█', '█', '○', '█'],
    ['█', '█', '█', '█', '█', '█'],
    ['█', '▓', '▓', '▓', '▓', '█'],
    ['█', ' ', '█', '█', ' ', '█'],
]);

const DEVOPS: Sprite = Sprite([
    [' ', ' ', '█', '█', ' ', ' '],
    [' ', '█', '█', '█', '█', ' '],
    ['█', '●', '█', '█', '●', '█'],
    ['█', '█', '█', '█', '█', '█'],
    ['█', '█', '█', '█', '█', '█'],
    ['▼', '█', '▼', '▼', '█', '▼'],
]);

/// Lookup the 6×6 sprite for `role`.
pub(crate) fn glyph_for_role(role: Role) -> Sprite {
    match role {
        Role::Implementer => IMPLEMENTER,
        Role::Orchestrator => ORCHESTRATOR,
        Role::Reviewer => REVIEWER,
        Role::Docs => DOCS,
        Role::DevOps => DEVOPS,
    }
}

/// Foreground color for the role's sprite.
///
/// `Color::Indexed(208)` is the xterm 256-color orange used for `Docs`. The
/// ANSI 16-color palette has no orange; this index is widely supported on
/// modern terminals and degrades to yellow on 16-color terminals.
pub(crate) fn role_color(role: Role) -> Color {
    match role {
        Role::Implementer => Color::Green,
        Role::Orchestrator => Color::Yellow,
        Role::Reviewer => Color::Magenta,
        Role::Docs => Color::Indexed(208),
        Role::DevOps => Color::Red,
    }
}

/// Three-character ASCII abbreviation for the icon-mode fallback.
pub(crate) fn role_abbrev(role: Role) -> &'static str {
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
    use std::collections::HashSet;

    const ALL_ROLES: [Role; 5] = [
        Role::Implementer,
        Role::Orchestrator,
        Role::Reviewer,
        Role::Docs,
        Role::DevOps,
    ];

    // --- role_color ---

    #[test]
    fn role_color_implementer_is_green() {
        assert_eq!(role_color(Role::Implementer), Color::Green);
    }

    #[test]
    fn role_color_orchestrator_is_yellow() {
        assert_eq!(role_color(Role::Orchestrator), Color::Yellow);
    }

    #[test]
    fn role_color_reviewer_is_magenta() {
        assert_eq!(role_color(Role::Reviewer), Color::Magenta);
    }

    #[test]
    fn role_color_docs_is_indexed_208() {
        assert_eq!(role_color(Role::Docs), Color::Indexed(208));
    }

    #[test]
    fn role_color_devops_is_red() {
        assert_eq!(role_color(Role::DevOps), Color::Red);
    }

    #[test]
    fn role_colors_are_distinct_across_roles() {
        let mut seen = HashSet::new();
        for role in ALL_ROLES {
            let color = role_color(role);
            assert!(
                seen.insert(format!("{:?}", color)),
                "duplicate color for {:?}",
                role
            );
        }
    }

    // --- role_abbrev ---

    #[test]
    fn role_abbrev_implementer_is_imp() {
        assert_eq!(role_abbrev(Role::Implementer), "IMP");
    }

    #[test]
    fn role_abbrev_orchestrator_is_orc() {
        assert_eq!(role_abbrev(Role::Orchestrator), "ORC");
    }

    #[test]
    fn role_abbrev_reviewer_is_rev() {
        assert_eq!(role_abbrev(Role::Reviewer), "REV");
    }

    #[test]
    fn role_abbrev_docs_is_doc() {
        assert_eq!(role_abbrev(Role::Docs), "DOC");
    }

    #[test]
    fn role_abbrev_devops_is_ops() {
        assert_eq!(role_abbrev(Role::DevOps), "OPS");
    }

    #[test]
    fn role_abbrev_is_three_uppercase_ascii_chars_for_all_roles() {
        for role in ALL_ROLES {
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
    fn role_abbrevs_are_unique_across_roles() {
        let mut seen = HashSet::new();
        for role in ALL_ROLES {
            assert!(
                seen.insert(role_abbrev(role)),
                "duplicate abbreviation for {:?}",
                role
            );
        }
    }

    // --- glyph_for_role ---

    #[test]
    fn glyph_for_role_returns_6x6_grid_for_all_roles() {
        for role in ALL_ROLES {
            let sprite = glyph_for_role(role);
            let rows = sprite.rows();
            assert_eq!(rows.len(), 6, "sprite for {:?} must have 6 rows", role);
            for (i, row) in rows.iter().enumerate() {
                assert_eq!(
                    row.len(),
                    6,
                    "sprite for {:?} row {} must have 6 chars",
                    role,
                    i
                );
            }
        }
    }

    #[test]
    fn glyphs_are_distinct_across_all_roles() {
        let sprites: Vec<Sprite> = ALL_ROLES.iter().map(|&r| glyph_for_role(r)).collect();
        for i in 0..sprites.len() {
            for j in (i + 1)..sprites.len() {
                assert_ne!(
                    sprites[i], sprites[j],
                    "sprites at index {} ({:?}) and {} ({:?}) must differ",
                    i, ALL_ROLES[i], j, ALL_ROLES[j]
                );
            }
        }
    }

    #[test]
    fn devops_sprite_row5_contains_fang_glyph() {
        let sprite = glyph_for_role(Role::DevOps);
        let row5 = sprite.rows()[5];
        assert!(
            row5.contains(&'\u{25BC}'),
            "DevOps row 5 must contain '▼' (fanged fringe)"
        );
    }

    #[test]
    fn orchestrator_sprite_row0_contains_diamond_glyph() {
        let sprite = glyph_for_role(Role::Orchestrator);
        let row0 = sprite.rows()[0];
        assert!(
            row0.contains(&'\u{25C6}'),
            "Orchestrator row 0 must contain '◆' (crown)"
        );
    }
}
