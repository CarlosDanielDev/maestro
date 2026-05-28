//! Nested-editor breadcrumb helper for [`super::dynamic_map_draw`].
//!
//! Split out of `dynamic_map_draw.rs` (#908) to keep both files under
//! the 400-LOC guardrail. Pure string-shaping logic — easy to unit-test
//! in isolation.

use crate::util::formatting::truncate_with_ellipsis;

pub(super) const BREADCRUMB_SEP: &str = " → ";

/// Build the nested-editor breadcrumb when `section_path` carries a
/// parent's dotted path (3+ segments). Returns `None` for top-level
/// widgets so the caller falls back to the normal `<label>:` header.
///
/// Three crumbs: `<section>.<outer_id>`, the inner field key (segments
/// 2+ joined back with `.`), and the active inner row id. Each crumb is
/// truncated to roughly a third of `area_width` so the band stays on
/// one line at any terminal width (#908 per-crumb truncation).
pub(super) fn nested_breadcrumb(
    section_path: &str,
    active_inner_id: Option<&str>,
    area_width: u16,
) -> Option<String> {
    let segs: Vec<&str> = section_path.split('.').collect();
    if segs.len() < 3 {
        return None;
    }
    let outer = format!("{}.{}", segs[0], segs[1]);
    let inner_field = segs[2..].join(".");
    let per_crumb =
        ((area_width as usize).saturating_sub(BREADCRUMB_SEP.chars().count() * 2) / 3).max(3);
    let outer_t = truncate_with_ellipsis(&outer, per_crumb);
    let inner_t = truncate_with_ellipsis(&inner_field, per_crumb);
    match active_inner_id {
        Some(role) if !role.is_empty() => {
            let role_t = truncate_with_ellipsis(role, per_crumb);
            Some(format!(
                "{outer_t}{BREADCRUMB_SEP}{inner_t}{BREADCRUMB_SEP}{role_t}"
            ))
        }
        _ => Some(format!("{outer_t}{BREADCRUMB_SEP}{inner_t}")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn returns_none_for_top_level_section() {
        // #908 — non-nested widgets keep the normal label header.
        assert_eq!(nested_breadcrumb("agents", None, 80), None);
        assert_eq!(nested_breadcrumb("teams", Some("worker-pool"), 80), None);
    }

    #[test]
    fn renders_three_crumbs_for_role_overrides() {
        // #908 — nested role_overrides surfaces all three crumbs.
        let out =
            nested_breadcrumb("teams.worker-pool.role_overrides", Some("reviewer"), 80).unwrap();
        assert!(
            out.contains("teams.worker-pool"),
            "outer crumb missing in {out:?}"
        );
        assert!(
            out.contains("role_overrides"),
            "inner field crumb missing in {out:?}"
        );
        assert!(out.contains("reviewer"), "role crumb missing in {out:?}");
        assert!(
            out.matches(BREADCRUMB_SEP).count() == 2,
            "expected exactly 2 separators, got {out:?}"
        );
    }

    #[test]
    fn omits_role_crumb_when_inner_is_unfocused() {
        // No active role yet (e.g. empty nested map) → two-crumb shape.
        let out = nested_breadcrumb("teams.worker-pool.role_overrides", None, 80).unwrap();
        assert!(out.contains("teams.worker-pool"));
        assert!(out.contains("role_overrides"));
        assert_eq!(out.matches(BREADCRUMB_SEP).count(), 1);
    }

    #[test]
    fn per_crumb_truncates_at_narrow_width() {
        // At 40 cols each crumb gets ~12 chars. "teams.worker-pool"
        // (17 chars) and "role_overrides" (14 chars) both truncate.
        let out =
            nested_breadcrumb("teams.worker-pool.role_overrides", Some("reviewer"), 40).unwrap();
        assert!(
            out.contains("..."),
            "narrow width must truncate at least one crumb, got {out:?}"
        );
    }
}
