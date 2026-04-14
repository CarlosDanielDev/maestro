//! Icon registry: centralized Nerd Font / ASCII icon definitions.
//! Config: `tui.ascii_icons = true` in maestro.toml
//! Env override: `MAESTRO_ASCII_ICONS=1`
//!
//! Usage: `icons::get(IconId::ChevronRight)` returns the correct variant
//! based on the current mode (Nerd Font or ASCII).

use std::sync::atomic::{AtomicBool, Ordering};

static ASCII_MODE: AtomicBool = AtomicBool::new(false);
static INITIALIZED: AtomicBool = AtomicBool::new(false);

/// Initialize icon mode from config. Call once at startup.
pub fn init_from_config(ascii_icons: bool) {
    ASCII_MODE.store(ascii_icons, Ordering::Relaxed);
    INITIALIZED.store(true, Ordering::Relaxed);
}

/// Returns true if Nerd Font icons should be used.
/// Checks (in order): config flag, MAESTRO_ASCII_ICONS env var.
pub fn use_nerd_font() -> bool {
    if INITIALIZED.load(Ordering::Relaxed) {
        return !ASCII_MODE.load(Ordering::Relaxed);
    }
    // Fallback: env var check (for lib crate / before config loads)
    use_nerd_font_from_env(|k| std::env::var(k).ok())
}

// Testable version: accepts an env-var reader closure.
pub(crate) fn use_nerd_font_from_env(get_env: impl Fn(&str) -> Option<String>) -> bool {
    get_env("MAESTRO_ASCII_ICONS")
        .map(|v| v != "1")
        .unwrap_or(true)
}

// ── Icon Registry ──────────────────────────────────────────────────────

/// Identifies every icon used in the TUI, grouped by semantic category.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[allow(dead_code)] // Registry covers all icons; some only used in files not yet migrated
pub enum IconId {
    // Navigation
    ChevronRight,
    ChevronDown,
    ArrowRight,
    ArrowLeft,
    ArrowUp,
    ArrowDown,
    AngleLeft,
    AngleRight,

    // Status
    CheckCircle,
    CheckCircleFill,
    XCircle,
    Circle,
    DotFill,
    Skip,
    Hourglass,
    Warning,
    Play,
    Pause,
    Sync,
    Skull,
    Alert,
    Refresh,
    Wrench,
    GitPr,
    GitMerge,
    Search,
    IssueOpened,
    Milestone,

    // UI Chrome
    GaugeFilled,
    GaugeEmpty,
    Selector,
    SeparatorV,
    SeparatorH,
    Fisheye,

    // Indicators
    CheckboxOn,
    CheckboxOff,
    Expand,
    Collapse,

    // Header Metrics
    Agents,
    Cost,
    Clock,
}

/// A Nerd Font / ASCII icon pair.
pub struct IconPair {
    pub nerd: &'static str,
    pub ascii: &'static str,
}

impl IconPair {
    const fn new(nerd: &'static str, ascii: &'static str) -> Self {
        Self { nerd, ascii }
    }
}

/// Maps each IconId to its Nerd Font and ASCII variants.
/// Compiles to a jump table — zero heap allocation, zero runtime cost.
const fn icon_pair(id: IconId) -> IconPair {
    match id {
        // ── Navigation ──────────────────────────────────────────────
        IconId::ChevronRight => IconPair::new("\u{f054}", ">"),
        IconId::ChevronDown => IconPair::new("\u{f078}", "v"),
        IconId::ArrowRight => IconPair::new("\u{f061}", "->"),
        IconId::ArrowLeft => IconPair::new("\u{f060}", "<-"),
        IconId::ArrowUp => IconPair::new("\u{2191}", "^"),
        IconId::ArrowDown => IconPair::new("\u{2193}", "v"),
        IconId::AngleLeft => IconPair::new("\u{f104}", "<"),
        IconId::AngleRight => IconPair::new("\u{f105}", ">"),

        // ── Status ──────────────────────────────────────────────────
        IconId::CheckCircle => IconPair::new("\u{f42e}", "[+]"),
        IconId::CheckCircleFill => IconPair::new("\u{f058}", "[*]"),
        IconId::XCircle => IconPair::new("\u{f467}", "[X]"),
        IconId::Circle => IconPair::new("\u{f4a3}", "( )"),
        IconId::DotFill => IconPair::new("\u{f444}", "(*)"),
        IconId::Skip => IconPair::new("\u{f4a7}", "[-]"),
        IconId::Hourglass => IconPair::new("\u{f251}", "[~]"),
        IconId::Warning => IconPair::new("\u{26A0}", "[!]"),
        IconId::Play => IconPair::new("\u{f40a}", "[>]"),
        IconId::Pause => IconPair::new("\u{f04c}", "[=]"),
        IconId::Sync => IconPair::new("\u{f46a}", "[S]"),
        IconId::Skull => IconPair::new("\u{f2d3}", "[D]"),
        IconId::Alert => IconPair::new("\u{f421}", "[A]"),
        IconId::Refresh => IconPair::new("\u{f363}", "[R]"),
        IconId::Wrench => IconPair::new("\u{f7d9}", "[W]"),
        IconId::GitPr => IconPair::new("\u{f407}", "[P]"),
        IconId::GitMerge => IconPair::new("\u{f419}", "[M]"),
        IconId::Search => IconPair::new("\u{f422}", "[?]"),
        IconId::IssueOpened => IconPair::new("\u{f41b}", ">>"),
        IconId::Milestone => IconPair::new("\u{f43e}", "~~"),

        // ── UI Chrome ───────────────────────────────────────────────
        IconId::GaugeFilled => IconPair::new("\u{2593}", "#"),
        IconId::GaugeEmpty => IconPair::new("\u{2591}", "-"),
        IconId::Selector => IconPair::new("\u{25b8}", ">"),
        IconId::SeparatorV => IconPair::new("\u{2502}", "|"),
        IconId::SeparatorH => IconPair::new("\u{2550}\u{2550}", "=="),
        IconId::Fisheye => IconPair::new("\u{25C9}", "*"),

        // ── Indicators ──────────────────────────────────────────────
        IconId::CheckboxOn => IconPair::new("\u{f46c}", "[x]"),
        IconId::CheckboxOff => IconPair::new("\u{f096}", "[ ]"),
        IconId::Expand => IconPair::new("\u{f054}", ">"),
        IconId::Collapse => IconPair::new("\u{f078}", "v"),

        // ── Header Metrics ─────────────────────────────────────────
        IconId::Agents => IconPair::new("\u{f064d}", "[U]"), // nf-md-account_group
        IconId::Cost => IconPair::new("$", "$"),
        IconId::Clock => IconPair::new("\u{f251}", "[T]"), // nf-fa-hourglass (⏳)
    }
}

/// Returns the correct icon string for the current mode (Nerd Font or ASCII).
pub fn get(id: IconId) -> &'static str {
    get_for_mode(id, use_nerd_font())
}

/// Pure, testable variant of `get()`. Pass the mode explicitly.
pub(crate) fn get_for_mode(id: IconId, nerd_font: bool) -> &'static str {
    let pair = icon_pair(id);
    if nerd_font { pair.nerd } else { pair.ascii }
}

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
        IconId::Milestone,
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
    ];

    // ── Existing tests (mode detection) ─────────────────────────────────

    #[test]
    fn returns_true_when_env_var_absent() {
        assert!(use_nerd_font_from_env(|_| None));
    }

    #[test]
    fn returns_false_when_ascii_icons_set_to_1() {
        assert!(!use_nerd_font_from_env(|_| Some("1".to_string())));
    }

    #[test]
    fn returns_true_when_ascii_icons_set_to_0() {
        assert!(use_nerd_font_from_env(|_| Some("0".to_string())));
    }

    #[test]
    fn init_from_config_sets_ascii_mode() {
        init_from_config(true);
        assert!(!use_nerd_font());
        // Reset for other tests
        init_from_config(false);
    }

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
            (IssueOpened, ">>"),
            (Agents, "[U]"),
            (Cost, "$"),
            (Clock, "[T]"),
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
}
