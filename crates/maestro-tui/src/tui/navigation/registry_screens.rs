//! The per-screen blocks of [`super::registry`], one per registered screen.
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
    // #1265: `P` retries the PR of the selected Overview session, so it is
    // no longer a global. Hidden: the Overview footer stays its
    // `mode_keymap` list; the palette lists `retry PR` as a command.
    v.push(
        Binding::new(
            ch('P'),
            Scope::Screen(TuiMode::Overview),
            A::RetryPr,
            Danger::Destructive,
            "retry PR",
        )
        .hidden(),
    );
    v
}
