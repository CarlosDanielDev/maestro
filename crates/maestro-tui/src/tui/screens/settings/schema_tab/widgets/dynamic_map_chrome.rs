//! Header-area chrome helpers for [`super::dynamic_map_draw`]:
//! nested-editor breadcrumb + tab-chip highlight style.
//!
//! Split out of `dynamic_map_draw.rs` (#908) to keep both files under
//! the 400-LOC guardrail. Pure presentation logic — easy to unit-test
//! in isolation.

use ratatui::style::{Modifier, Style};

use crate::tui::theme::Theme;
use crate::util::formatting::truncate_with_ellipsis;

pub(super) const BREADCRUMB_SEP: &str = " → ";

/// Tab-chip highlight style. Full orange chip when the SubtabStrip is
/// the current chord target; muted underline-only chip otherwise.
///
/// Bright requires BOTH the outer caller treats this widget as the
/// chord target (`focused=true`) AND its own focus is on the
/// SubtabStrip. Caller folds those two bits into a single
/// `chord_target` bool. Mirrors the dim-tab-strip pattern from the
/// sidebar — only one chip on screen "shouts" at a time (#908).
pub(super) fn tab_highlight_style(theme: &Theme, chord_target: bool) -> Style {
    if chord_target {
        Style::default()
            .fg(theme.selection_fg)
            .bg(theme.selection_bg)
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default()
            .fg(theme.text_secondary)
            .add_modifier(Modifier::UNDERLINED)
    }
}

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
    fn tab_highlight_full_chip_when_chord_target() {
        // #908 contrast — bright when caller passes chord_target=true.
        let theme = Theme::dark();
        let style = tab_highlight_style(&theme, true);
        assert_eq!(
            style.bg,
            Some(theme.selection_bg),
            "must paint selection bg"
        );
        assert_eq!(
            style.fg,
            Some(theme.selection_fg),
            "must paint selection fg"
        );
        assert!(
            style.add_modifier.contains(Modifier::BOLD),
            "must be BOLD when chord target"
        );
    }

    #[test]
    fn tab_highlight_dim_underline_when_not_chord_target() {
        // #908 contrast — dim once chord target is elsewhere (focus
        // descended into an EntryField, or widget itself is unfocused).
        let theme = Theme::dark();
        let style = tab_highlight_style(&theme, false);
        assert_eq!(style.bg, None, "must NOT paint bg when not chord target");
        assert_eq!(
            style.fg,
            Some(theme.text_secondary),
            "must use text_secondary fg when not chord target"
        );
        assert!(
            !style.add_modifier.contains(Modifier::BOLD),
            "must NOT be BOLD when not chord target"
        );
        assert!(
            style.add_modifier.contains(Modifier::UNDERLINED),
            "must be UNDERLINED to keep the active-tab signal"
        );
    }

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
