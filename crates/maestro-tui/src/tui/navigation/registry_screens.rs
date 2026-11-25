//! The per-screen blocks of [`super::registry`], one per registered screen,
//! and the ⌃K palette-only commands (#1255, #1265).
//! Esc is the Global "back" on every screen, so no block redeclares it.

use crossterm::event::KeyCode;

use super::{Action as A, Binding, Chord, Danger, Scope};
use crate::tui::app::{SessionView, TuiMode};

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
    // #1288: the session screen. The strip draws `d f l v e` on every
    // view, so they stay out of the footer. `X` kills the session on
    // screen; `End` follows the tail again.
    for (view, _, _) in SessionView::ALL {
        let scope = Scope::Screen(TuiMode::Session {
            id: uuid::Uuid::nil(),
            view,
        });
        v.extend(
            SessionView::ALL
                .iter()
                .map(|&(_, key, label)| s(scope, ch(key), A::OpenView, label).hidden()),
        );
    }
    let detail = Scope::Screen(TuiMode::Detail(uuid::Uuid::nil()));
    v.push(Binding::new(ch('X'), detail, A::Kill, Danger::Destructive, "kill").hidden());
    v.push(s(detail, ch('w'), A::Switcher, "switch session").hidden());
    v.push(s(detail, ch('c'), A::Copy, "copy reply").hidden());
    v.push(s(full, Chord::key(KeyCode::End), A::Bottom, "follow").hidden());
    let events = Scope::Screen(TuiMode::CallLog(uuid::Uuid::nil()));
    v.push(s(events, Chord::key(KeyCode::End), A::Bottom, "follow").hidden());
    // #1265: `r` reloads here as on every other screen. #1269: `R` only
    // opens the Run queue (the plan); spawning needs ⏎ in its launch dialog.
    // The footer stays `MilestoneScreen::footer_hints`.
    let milestones = Scope::Screen(TuiMode::MilestoneView);
    v.extend([
        s(milestones, ch('r'), A::Reload, "reload"),
        s(milestones, ch('R'), A::RunAll, "plan run")
            .exception("uppercase keeps the plan off the `r` reload reflex"),
    ]);
    // #1292: the conflict toast's keys, on the seven tab screens (where
    // `App::conflict_toast_key` acts, only while the toast is visible).
    // Pausing is undone with ⌃Z, so it is Safe. Hidden: the palette would
    // list them where they do nothing; Help and the footer show them.
    for (_, _, tab) in crate::tui::ds::tab_bar::TABS {
        for (c, action, label) in [
            ('p', A::PauseOther, "pause the other agent (conflict toast)"),
            ('o', A::ShowFile, "show the conflict file (conflict toast)"),
        ] {
            let scope = Scope::Screen(tab);
            v.push(Binding::new(Chord::ctrl(c), scope, action, Danger::Safe, label).hidden());
        }
    }
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
        // #1289: the Usage views; the selection is kept across them. The
        // view strip shows these keys, so the footer does not repeat them.
        s(dash, ch('o'), A::Expand, "Ops view").hidden(),
        s(dash, ch('c'), A::Expand, "Cost view").hidden(),
        s(dash, ch('t'), A::Expand, "Tokens view").hidden(),
        // #1292: `g` opens Deps, `g` again switches issues ⇄ files. The
        // lens keys (`← →` columns) are drawn in its own footer.
        s(dash, ch('g'), A::Expand, "Deps view · lens").hidden(),
        s(
            dash,
            Chord::key(KeyCode::Left),
            A::Expand,
            "Deps: agents column",
        )
        .hidden(),
        s(
            dash,
            Chord::key(KeyCode::Right),
            A::Expand,
            "Deps: files column",
        )
        .hidden(),
    ]);
    // #1280: Overview routes to the Dashboard, so Overview's `P` (retry the
    // selected session's PR, #1265) is gone; ⏎ ⏎ on a PR pipeline row is
    // the retry, behind the armed bar.
    // #1286: Teams (`screens::team_wizard`). One label per key across the
    // screen: `l` launches on the home and moves the agent on Compose ›
    // Roles. Tab (tier on Review & save) and Esc are Global keys.
    let teams = Scope::Screen(TuiMode::TeamWizard);
    v.extend([
        s(teams, ch('j'), A::ScrollDown, "move"),
        s(teams, ch('k'), A::ScrollUp, "move"),
        s(teams, Chord::key(KeyCode::Down), A::ScrollDown, "move").hidden(),
        s(teams, Chord::key(KeyCode::Up), A::ScrollUp, "move").hidden(),
        s(teams, ch('c'), A::Expand, "compose (home)"),
        s(teams, ch('l'), A::Expand, "launch (home) · agent → (Roles)"),
        s(teams, ch('m'), A::Expand, "manage (home)"),
        s(teams, ch('h'), A::Expand, "agent ← (Roles)"),
        s(
            teams,
            Chord::key(KeyCode::Left),
            A::Expand,
            "agent ← (Roles)",
        )
        .hidden(),
        s(
            teams,
            Chord::key(KeyCode::Right),
            A::Expand,
            "agent → (Roles)",
        )
        .hidden(),
        s(teams, ch(' '), A::Expand, "bind agent (Roles)"),
        s(
            teams,
            Chord::key(KeyCode::Enter),
            A::Expand,
            "next · ⏎ ⏎ writes on Review & save",
        ),
        // #1290: a failed launch goes back to its plan (never spends); a
        // failed delete retries.
        s(
            teams,
            ch('r'),
            A::Expand,
            "re-plan (launch) · retry (delete)",
        ),
    ]);
    // #1287: the run loop (`screens::run_loop::input`). `h` holds, `S`
    // stops behind a typed confirm; ↑↓ / k j and ⏎ pick and confirm on the decision
    // card. Hidden: the Run bar and the card draw them only while they work.
    let run = Scope::Screen(TuiMode::QueueExecution);
    v.extend([
        s(run, ch('h'), A::Hold, "hold queue").hidden(),
        Binding::new(ch('S'), run, A::Stop, Danger::Destructive, "stop queue…").hidden(),
        s(run, Chord::key(KeyCode::Up), A::ScrollUp, "pick").hidden(),
        s(run, Chord::key(KeyCode::Down), A::ScrollDown, "pick").hidden(),
        s(run, ch('k'), A::ScrollUp, "pick").hidden(),
        s(run, ch('j'), A::ScrollDown, "pick").hidden(),
        s(run, Chord::key(KeyCode::Enter), A::Expand, "confirm").hidden(),
        s(run, ch('v'), A::Expand, "evidence").hidden(),
    ]);
    for mode in [TuiMode::ContinuousPause, TuiMode::HollowRetry] {
        let card = Scope::Screen(mode);
        v.extend([
            s(card, Chord::key(KeyCode::Up), A::ScrollUp, "pick").hidden(),
            s(card, Chord::key(KeyCode::Down), A::ScrollDown, "pick").hidden(),
            s(card, ch('k'), A::ScrollUp, "pick").hidden(),
            s(card, ch('j'), A::ScrollDown, "pick").hidden(),
            s(card, Chord::key(KeyCode::Enter), A::Expand, "confirm").hidden(),
        ]);
    }
    v.push(
        s(
            Scope::Screen(TuiMode::HollowRetry),
            ch('v'),
            A::Expand,
            "evidence",
        )
        .hidden(),
    );
    // #1281: Adapt (`screens::adapt`). `← →` / Space choose Stop after on
    // Configure; ⏎ starts the chosen run there and arms, then writes, on
    // Review. Hidden: the screen's footer draws them per step.
    let adapt = Scope::Screen(TuiMode::AdaptWizard);
    v.extend([
        s(adapt, Chord::key(KeyCode::Left), A::Expand, "stop after").hidden(),
        s(adapt, Chord::key(KeyCode::Right), A::Expand, "stop after").hidden(),
        s(adapt, ch(' '), A::Expand, "stop after").hidden(),
        s(
            adapt,
            Chord::key(KeyCode::Enter),
            A::Expand,
            "start · ⏎ ⏎ writes on Review",
        )
        .hidden(),
    ]);
    v
}

/// A ⌃K palette command with no global key (#1255). `Network`: diagnostics
/// moved off Home (§02). The rest are the retired Shift globals `Q` `M` `C`
/// `P` (#1265); `CompleteModal` opens Settings (Theme › `modal_on_complete`).
/// `PrPipeline` (#1280) opens the Dashboard on its first pending PR row,
/// where ⏎ ⏎ retries; it writes nothing itself. `AgentGraph` (#1280) was
/// Overview's `g`; since #1292 it opens Dashboard › Deps › files.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PaletteCommand {
    Network,
    /// The TurboQuant panel: opens Dashboard › Tokens (#1289).
    Turboquant,
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
        "TurboQuant (Dashboard › Tokens)",
        PaletteCommand::Turboquant,
        Some(("t", TuiMode::Dashboard)),
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
    (
        "agent graph (Deps › files)",
        PaletteCommand::AgentGraph,
        None,
    ),
];
