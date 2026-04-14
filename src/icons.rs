//! Shared icon registry: centralized Nerd Font / ASCII icon definitions.
//!
//! This module lives in the lib crate so both `session::types` and `tui::icons`
//! can access the registry without cross-module dependency violations.

use crate::icon_mode::use_nerd_font;

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
    IssueClosed,
    Milestone,
    NeedsReview,

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

    // Header Brand
    Repo,
    User,
    Branch,
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
        IconId::IssueOpened => IconPair::new("\u{f0766}", "[#]"), // nf-md-circle_outline
        IconId::IssueClosed => IconPair::new("\u{f04d2}", "[+]"), // nf-md-check_circle
        IconId::Milestone => IconPair::new("\u{f0431}", "[M]"),   // nf-md-flag
        IconId::NeedsReview => IconPair::new("\u{f41b}", "[!]"),  // nf-oct-issue_opened

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
        IconId::Clock => IconPair::new("\u{f251}", "[T]"), // nf-fa-hourglass

        // ── Header Brand ──────────────────────────────────────────
        IconId::Repo => IconPair::new("\u{f408}", "(g)"), // nf-oct-repo
        IconId::User => IconPair::new("\u{f007}", "@"),   // nf-fa-user
        IconId::Branch => IconPair::new("\u{f418}", "(b)"), // nf-oct-git_branch
    }
}

/// Returns the correct icon string for the current mode (Nerd Font or ASCII).
pub fn get(id: IconId) -> &'static str {
    get_for_mode(id, use_nerd_font())
}

/// Pure, testable variant of `get()`. Pass the mode explicitly.
pub fn get_for_mode(id: IconId, nerd_font: bool) -> &'static str {
    let pair = icon_pair(id);
    if nerd_font { pair.nerd } else { pair.ascii }
}
