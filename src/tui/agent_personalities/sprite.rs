//! 6×6 character grids for each role — spike prototype.
//!
//! See `docs/adr/002-agent-personalities.md` § Sprite Design Language for the
//! full design language. The compile-time `[[char; 6]; 6]` shape enforces that
//! every sprite is exactly 36 cells; no runtime check is needed.

use super::role::Role;

/// A fixed-size 6×6 character grid.
///
/// The newtype is deliberate: variable-size sprites would force the renderer to
/// know each role's bounding box. Keeping the type compile-time-uniform lets the
/// renderer treat every role identically.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Sprite([[char; 6]; 6]);

impl Sprite {
    pub fn rows(&self) -> &[[char; 6]; 6] {
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

/// Lookup the sprite for a given role. Total: 5 sprites × 36 cells = 180 cells
/// of static data.
pub fn glyph_for_role(role: Role) -> Sprite {
    match role {
        Role::Implementer => IMPLEMENTER,
        Role::Orchestrator => ORCHESTRATOR,
        Role::Reviewer => REVIEWER,
        Role::Docs => DOCS,
        Role::DevOps => DEVOPS,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn orchestrator_and_implementer_differ() {
        let a = glyph_for_role(Role::Orchestrator);
        let b = glyph_for_role(Role::Implementer);
        assert_ne!(
            a, b,
            "the two prototype sprites must be visually distinguishable"
        );
    }

    #[test]
    fn devops_has_fanged_fringe() {
        let s = glyph_for_role(Role::DevOps);
        let row5 = s.rows()[5];
        assert!(
            row5.contains(&'▼'),
            "DevOps row 5 must include the fang glyph"
        );
    }
}
