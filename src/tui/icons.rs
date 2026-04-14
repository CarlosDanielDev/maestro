//! Icon registry: re-exports from the shared `crate::icons` module.
//! Mode detection: re-exports from `crate::icon_mode`.
//!
//! Config: `tui.ascii_icons = true` in maestro.toml
//! Env override: `MAESTRO_ASCII_ICONS=1`
//!
//! Usage: `icons::get(IconId::ChevronRight)` returns the correct variant
//! based on the current mode (Nerd Font or ASCII).

pub use crate::icons::*;

#[cfg(test)]
pub use crate::icon_mode::init_from_config;
// Re-exported for callers that import mode detection from `tui::icons`.
pub use crate::icon_mode::use_nerd_font;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::session::types::SessionStatus;

    const ALL_ICON_IDS: &[IconId] = &[
        // Navigation
        IconId::ChevronRight,
        IconId::ChevronDown,
        IconId::ArrowRight,
        IconId::ArrowLeft,
        IconId::ArrowUp,
        IconId::ArrowDown,
        IconId::AngleLeft,
        IconId::AngleRight,
        // Status
        IconId::CheckCircle,
        IconId::CheckCircleFill,
        IconId::XCircle,
        IconId::Circle,
        IconId::DotFill,
        IconId::Skip,
        IconId::Hourglass,
        IconId::Warning,
        IconId::Play,
        IconId::Pause,
        IconId::Sync,
        IconId::Skull,
        IconId::Alert,
        IconId::Refresh,
        IconId::Wrench,
        IconId::GitPr,
        IconId::GitMerge,
        IconId::Search,
        IconId::IssueOpened,
        IconId::IssueClosed,
        IconId::Milestone,
        IconId::NeedsReview,
        // UI Chrome
        IconId::GaugeFilled,
        IconId::GaugeEmpty,
        IconId::Selector,
        IconId::SeparatorV,
        IconId::SeparatorH,
        IconId::Fisheye,
        // Indicators
        IconId::CheckboxOn,
        IconId::CheckboxOff,
        IconId::Expand,
        IconId::Collapse,
        // Header Metrics
        IconId::Agents,
        IconId::Cost,
        IconId::Clock,
        // Header Brand
        IconId::Repo,
        IconId::User,
        IconId::Branch,
    ];

    // ── SessionStatus symbol tests ──────────────────────────────────────

    #[test]
    fn nerd_symbol_all_variants_are_nonempty() {
        use SessionStatus::*;
        let variants = [
            Queued,
            Spawning,
            Running,
            Completed,
            GatesRunning,
            NeedsReview,
            Errored,
            Paused,
            Killed,
            Stalled,
            Retrying,
            CiFix,
            NeedsPr,
            ConflictFix,
        ];
        for v in variants {
            assert!(!v.nerd_symbol().is_empty(), "{v:?}.nerd_symbol() is empty");
        }
    }

    #[test]
    fn ascii_symbol_all_variants_are_ascii_bracketed() {
        use SessionStatus::*;
        let variants = [
            Queued,
            Spawning,
            Running,
            Completed,
            GatesRunning,
            NeedsReview,
            Errored,
            Paused,
            Killed,
            Stalled,
            Retrying,
            CiFix,
            NeedsPr,
            ConflictFix,
        ];
        for v in variants {
            let s = v.ascii_symbol();
            assert!(s.is_ascii(), "{v:?}.ascii_symbol() returned non-ASCII: {s}");
            assert!(
                s.starts_with('[') && s.ends_with(']'),
                "{v:?}.ascii_symbol() must be [X] form, got: {s}"
            );
        }
    }

    #[test]
    fn nerd_and_ascii_symbols_are_distinct_for_running() {
        let nerd = SessionStatus::Running.nerd_symbol();
        let ascii = SessionStatus::Running.ascii_symbol();
        assert_ne!(
            nerd, ascii,
            "nerd and ascii symbols must be distinct for Running"
        );
    }

    // ── Icon Registry tests (#286) ──────────────────────────────────────

    #[test]
    fn all_nerd_variants_are_nonempty() {
        for &id in ALL_ICON_IDS {
            let s = get_for_mode(id, true);
            assert!(!s.is_empty(), "{id:?}: nerd variant must not be empty");
        }
    }

    #[test]
    fn all_ascii_variants_are_ascii() {
        for &id in ALL_ICON_IDS {
            let s = get_for_mode(id, false);
            assert!(
                s.is_ascii(),
                "{id:?}: ascii variant contains non-ASCII: {s:?}"
            );
        }
    }

    #[test]
    fn all_ascii_variants_are_nonempty() {
        for &id in ALL_ICON_IDS {
            let s = get_for_mode(id, false);
            assert!(!s.is_empty(), "{id:?}: ascii variant must not be empty");
        }
    }

    #[test]
    fn nerd_and_ascii_are_distinct_for_all_variants() {
        for &id in ALL_ICON_IDS {
            // Cost uses "$" universally — no nerd font alternative needed.
            if matches!(id, IconId::Cost) {
                continue;
            }
            let nerd = get_for_mode(id, true);
            let ascii = get_for_mode(id, false);
            assert_ne!(
                nerd, ascii,
                "{id:?}: nerd and ascii variants must differ (got {nerd:?} for both)"
            );
        }
    }

    #[test]
    fn nerd_spot_checks_known_codepoints() {
        use IconId::*;
        let cases: &[(IconId, &str)] = &[
            (Hourglass, "\u{f251}"),
            (Play, "\u{f40a}"),
            (Pause, "\u{f04c}"),
            (CheckCircle, "\u{f42e}"),
            (XCircle, "\u{f467}"),
            (Search, "\u{f422}"),
            (ChevronRight, "\u{f054}"),
            (GaugeFilled, "\u{2593}"),
            (GaugeEmpty, "\u{2591}"),
            (CheckboxOn, "\u{f46c}"),
            (CheckboxOff, "\u{f096}"),
            (Agents, "\u{f064d}"),
            (Cost, "$"),
            (Clock, "\u{f251}"),
            (IssueOpened, "\u{f0766}"),
            (IssueClosed, "\u{f04d2}"),
            (Milestone, "\u{f0431}"),
            (NeedsReview, "\u{f41b}"),
        ];
        for &(id, expected) in cases {
            assert_eq!(
                get_for_mode(id, true),
                expected,
                "{id:?}: unexpected nerd codepoint"
            );
        }
    }

    #[test]
    fn ascii_spot_checks_known_strings() {
        use IconId::*;
        let cases: &[(IconId, &str)] = &[
            (CheckboxOn, "[x]"),
            (CheckboxOff, "[ ]"),
            (ChevronRight, ">"),
            (GaugeFilled, "#"),
            (GaugeEmpty, "-"),
            (Warning, "[!]"),
            (IssueOpened, "[#]"),
            (IssueClosed, "[+]"),
            (Milestone, "[M]"),
            (Agents, "[U]"),
            (Cost, "$"),
            (Clock, "[T]"),
            (NeedsReview, "[!]"),
        ];
        for &(id, expected) in cases {
            assert_eq!(
                get_for_mode(id, false),
                expected,
                "{id:?}: unexpected ascii fallback"
            );
        }
    }

    #[test]
    fn get_routes_through_global_mode_nerd() {
        init_from_config(false); // nerd mode
        let via_get = get(IconId::Play);
        let via_pure = get_for_mode(IconId::Play, true);
        assert_eq!(
            via_get, via_pure,
            "get() must match get_for_mode(..., true) in nerd mode"
        );
        init_from_config(false); // restore
    }

    #[test]
    fn get_routes_through_global_mode_ascii() {
        init_from_config(true); // ascii mode
        let via_get = get(IconId::Play);
        let via_pure = get_for_mode(IconId::Play, false);
        assert_eq!(
            via_get, via_pure,
            "get() must match get_for_mode(..., false) in ascii mode"
        );
        assert!(
            via_get.is_ascii(),
            "get() in ascii mode must return ASCII-only string"
        );
        init_from_config(false); // restore
    }

    #[test]
    fn expand_collapse_are_semantic_aliases() {
        assert_eq!(
            get_for_mode(IconId::Expand, true),
            get_for_mode(IconId::ChevronRight, true)
        );
        assert_eq!(
            get_for_mode(IconId::Collapse, true),
            get_for_mode(IconId::ChevronDown, true)
        );
    }

    #[test]
    fn icon_pair_returns_both_variants() {
        assert_eq!(get_for_mode(IconId::CheckCircle, true), "\u{f42e}");
        assert_eq!(get_for_mode(IconId::CheckCircle, false), "[+]");
    }

    // ── #308: SessionStatus delegates to icon registry ──────────────────

    #[test]
    fn nerd_symbol_delegates_to_icon_registry_for_all_variants() {
        let cases: &[(SessionStatus, IconId)] = &[
            (SessionStatus::Queued, IconId::Hourglass),
            (SessionStatus::Spawning, IconId::Sync),
            (SessionStatus::Running, IconId::Play),
            (SessionStatus::Completed, IconId::CheckCircle),
            (SessionStatus::GatesRunning, IconId::Search),
            (SessionStatus::NeedsReview, IconId::NeedsReview),
            (SessionStatus::Errored, IconId::XCircle),
            (SessionStatus::Paused, IconId::Pause),
            (SessionStatus::Killed, IconId::Skull),
            (SessionStatus::Stalled, IconId::Alert),
            (SessionStatus::Retrying, IconId::Refresh),
            (SessionStatus::CiFix, IconId::Wrench),
            (SessionStatus::NeedsPr, IconId::GitPr),
            (SessionStatus::ConflictFix, IconId::GitMerge),
        ];
        for &(ref status, icon_id) in cases {
            assert_eq!(
                status.nerd_symbol(),
                get_for_mode(icon_id, true),
                "{status:?}.nerd_symbol() must equal registry get_for_mode({icon_id:?}, true)"
            );
        }
    }

    #[test]
    fn ascii_symbol_queued_remains_q_not_registry_value() {
        assert_eq!(SessionStatus::Queued.ascii_symbol(), "[Q]");
        assert_ne!(SessionStatus::Queued.ascii_symbol(), "[~]");
    }

    #[test]
    fn ascii_symbol_paused_remains_bracket_dash_not_registry_value() {
        assert_eq!(SessionStatus::Paused.ascii_symbol(), "[-]");
        assert_ne!(SessionStatus::Paused.ascii_symbol(), "[=]");
    }

    #[test]
    fn symbol_reads_from_shared_icon_mode_not_local_atomic() {
        use crate::icon_mode;
        icon_mode::init_from_config(true); // ASCII
        assert_eq!(
            SessionStatus::Running.symbol(),
            SessionStatus::Running.ascii_symbol()
        );
        icon_mode::init_from_config(false); // Nerd
        assert_eq!(
            SessionStatus::Running.symbol(),
            SessionStatus::Running.nerd_symbol()
        );
    }
}
