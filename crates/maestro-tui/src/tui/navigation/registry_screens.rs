//! The per-screen blocks of [`super::registry`], one per registered screen,
//! and the ⌃K palette-only commands (#1255, #1265).
//! Esc is the Global "back" on every screen, so no block redeclares it.

use crossterm::event::KeyCode;

use super::{Action as A, Binding, Chord, Danger, Scope};
use crate::tui::app::TuiMode;

pub(super) fn blocks() -> Vec<Binding> {
    let ch = |c| Chord::key(KeyCode::Char(c));
    let s = |scope, chord, action, label| Binding::new(chord, scope, action, Danger::Safe, label);
    let mut v = Vec::new();
    let log = Scope::Screen(TuiMode::LogViewer(uuid::Uuid::nil()));
    v.extend([
        s(log, ch('j'), A::ScrollDown, "Down"),
        s(log, ch('k'), A::ScrollUp, "Up"),
        s(log, Chord::key(KeyCode::Down), A::ScrollDown, "Down").hidden(),
        s(log, Chord::key(KeyCode::Up), A::ScrollUp, "Up").hidden(),
        s(log, ch('g'), A::Top, "Top"),
        s(log, ch('G'), A::Bottom, "Bottom").exception("vim pair: g top, G bottom"),
    ]);
    let summary = Scope::Screen(TuiMode::SessionSummary);
    v.extend([
        s(summary, ch('j'), A::ScrollDown, "Down"),
        s(summary, ch('k'), A::ScrollUp, "Up"),
        s(summary, Chord::key(KeyCode::Down), A::ScrollDown, "Down").hidden(),
        s(summary, Chord::key(KeyCode::Up), A::ScrollUp, "Up").hidden(),
        s(summary, Chord::key(KeyCode::Enter), A::Expand, "Expand"),
    ]);
    // #1261: the typed kill confirm. Typed text is content, not a
    // binding; Enter kills only once it matches.
    let kill = Scope::Screen(TuiMode::ConfirmKill(uuid::Uuid::nil()));
    v.push(Binding::new(
        Chord::key(KeyCode::Enter),
        kill,
        A::Kill,
        Danger::Destructive,
        "kill",
    ));
    // #1240: Fullscreen and the Agent graph take every key
    // (`input_handler::handle_fullscreen` / `handle_agent_graph`); these are
    // the only ones that act. No kill, pause or dismiss: the Overview
    // selection those act on is not on screen. Hidden: their footers stay
    // the `mode_keymap` lists, which also show the Global `Esc Back`.
    let full = Scope::Screen(TuiMode::Fullscreen(uuid::Uuid::nil()));
    v.extend([
        s(full, ch('j'), A::ScrollDown, "Down").hidden(),
        s(full, ch('k'), A::ScrollUp, "Up").hidden(),
        s(full, Chord::key(KeyCode::Down), A::ScrollDown, "Down").hidden(),
        s(full, Chord::key(KeyCode::Up), A::ScrollUp, "Up").hidden(),
    ]);
    let graph = Scope::Screen(TuiMode::AgentGraph);
    v.push(s(graph, ch('g'), A::TogglePanels, "Panels").hidden());
    // #1265: `r` reloads here as on every other screen; run-all is `R` and
    // keeps its confirm (2+ issues open the queue review first). The footer
    // stays `MilestoneScreen::footer_hints`.
    let milestones = Scope::Screen(TuiMode::MilestoneView);
    v.extend([
        s(milestones, ch('r'), A::Reload, "reload"),
        Binding::new(
            ch('R'),
            milestones,
            A::RunAll,
            Danger::Destructive,
            "run all",
        ),
    ]);
    // #1280: the Dashboard cockpit (`screens::dashboard::dispatch`). Its
    // launcher letters are `HomeScreen` keys; Help lists them from
    // `QUICK_ACTIONS`. `⏎` on a pending PR row arms, a second `⏎` writes.
    let dash = Scope::Screen(TuiMode::Dashboard);
    v.extend([
        s(dash, ch('j'), A::ScrollDown, "Down"),
        s(dash, ch('k'), A::ScrollUp, "Up"),
        s(dash, Chord::key(KeyCode::Down), A::ScrollDown, "Down").hidden(),
        s(dash, Chord::key(KeyCode::Up), A::ScrollUp, "Up").hidden(),
        s(dash, Chord::key(KeyCode::Enter), A::Expand, "open · arm PR"),
        Binding::new(ch('X'), dash, A::Kill, Danger::Destructive, "kill"),
        s(dash, ch('l'), A::Expand, "log"),
        Binding::new(
            ch('D'),
            dash,
            A::Dismiss,
            Danger::Destructive,
            "dismiss finished",
        ),
        // `handle_global_shortcuts` runs it while a CI check fails (#695).
        s(dash, ch('e'), A::Expand, "CI error review").hidden(),
    ]);
    // #1280: Overview routes to the Dashboard, so Overview's `P` (retry the
    // selected session's PR, #1265) is gone; ⏎ ⏎ on a PR pipeline row is
    // the retry, behind the armed bar.
    // #1286: Teams overview (`screens::team_wizard`): select a preset,
    // start a flow; ⏎ advances, and on Review & save ⏎ arms, ⏎ writes.
    let teams = Scope::Screen(TuiMode::TeamWizard);
    v.extend([
        s(teams, ch('j'), A::ScrollDown, "Down"),
        s(teams, ch('k'), A::ScrollUp, "Up"),
        s(teams, Chord::key(KeyCode::Down), A::ScrollDown, "Down").hidden(),
        s(teams, Chord::key(KeyCode::Up), A::ScrollUp, "Up").hidden(),
        s(teams, ch('c'), A::Expand, "compose"),
        s(teams, ch('l'), A::Expand, "launch"),
        s(teams, ch('m'), A::Expand, "manage"),
        s(
            teams,
            Chord::key(KeyCode::Enter),
            A::Expand,
            "next · arm save",
        ),
    ]);
    v
}

/// A ⌃K palette command with no global key (#1255). `Network`: diagnostics
/// moved off Home (§02). The rest are the retired Shift globals `Q` `M` `C`
/// `P` (#1265); `CompleteModal` opens Settings (Theme › `modal_on_complete`).
/// `PrPipeline` (#1280) opens the Dashboard on its first pending PR row,
/// where ⏎ ⏎ retries; it writes nothing itself. `AgentGraph` (#1280) was
/// Overview's `g`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PaletteCommand {
    Network,
    TurboquantDashboard,
    CompleteModal,
    ReviewConcerns,
    PrPipeline,
    AgentGraph,
}

/// The key that runs a palette command outside the palette, and the one
/// screen it works on; the palette shows it there only.
pub type KeyOn = Option<(&'static str, TuiMode)>;

pub const PALETTE_COMMANDS: &[(&str, PaletteCommand, KeyOn)] = &[
    ("network", PaletteCommand::Network, None),
    (
        "TurboQuant dashboard",
        PaletteCommand::TurboquantDashboard,
        Some(("Q", TuiMode::Dashboard)),
    ),
    (
        "session-complete modal (Settings)",
        PaletteCommand::CompleteModal,
        None,
    ),
    ("review concerns", PaletteCommand::ReviewConcerns, None),
    (
        "retry PR (Dashboard PR pipeline)",
        PaletteCommand::PrPipeline,
        None,
    ),
    ("agent graph", PaletteCommand::AgentGraph, None),
];
