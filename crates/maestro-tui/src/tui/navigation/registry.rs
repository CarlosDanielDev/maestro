//! Keymap registry (design system §06 Keymap, §07 "Keymap registry"): every
//! binding declared once as `(chord, scope, action, danger, label)`, plus
//! [`check`], which encodes the §06 rules. Dispatch stays in
//! `input_handler.rs`; this is a declaration that tests hold to the rules
//! and that feeds the footer of registered screens ([`footer_hints`]).

use crossterm::event::{KeyCode, KeyModifiers};

use super::keymap::InlineHint;
use crate::tui::app::TuiMode;

/// One key press: a key code and its modifiers. Printable characters are
/// stored as crossterm reports them, without `SHIFT` (`?`, `G`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Chord {
    pub code: KeyCode,
    pub mods: KeyModifiers,
}

impl Chord {
    pub const fn key(code: KeyCode) -> Self {
        Self {
            code,
            mods: KeyModifiers::NONE,
        }
    }

    pub const fn ctrl(c: char) -> Self {
        Self {
            code: KeyCode::Char(c),
            mods: KeyModifiers::CONTROL,
        }
    }

    /// The text a hint shows for this chord (`⌃K`, `Enter`, `G`).
    pub fn label(self) -> &'static str {
        // Static tables, so footer hints stay `&'static str`.
        const PRINTABLE: &str = "!\"#$%&'()*+,-./0123456789:;<=>?@ABCDEFGHIJKLMNOPQRSTUVWXYZ[\\]^_`abcdefghijklmnopqrstuvwxyz{|}~";
        const CTRL: [&str; 26] = [
            "⌃A", "⌃B", "⌃C", "⌃D", "⌃E", "⌃F", "⌃G", "⌃H", "⌃I", "⌃J", "⌃K", "⌃L", "⌃M", "⌃N",
            "⌃O", "⌃P", "⌃Q", "⌃R", "⌃S", "⌃T", "⌃U", "⌃V", "⌃W", "⌃X", "⌃Y", "⌃Z",
        ];
        const FKEYS: [&str; 12] = [
            "F1", "F2", "F3", "F4", "F5", "F6", "F7", "F8", "F9", "F10", "F11", "F12",
        ];
        match self.code {
            KeyCode::Char(c)
                if self.mods.contains(KeyModifiers::CONTROL) && c.is_ascii_lowercase() =>
            {
                CTRL[(c as u8 - b'a') as usize]
            }
            KeyCode::Char(' ') => "Space",
            KeyCode::Char(c) if ('!'..='~').contains(&c) => {
                let i = (c as u8 - b'!') as usize;
                &PRINTABLE[i..=i]
            }
            KeyCode::F(n @ 1..=12) => FKEYS[(n - 1) as usize],
            KeyCode::Enter => "Enter",
            KeyCode::Esc => "Esc",
            KeyCode::Tab => "Tab",
            KeyCode::BackTab => "⇧Tab",
            KeyCode::Up => "↑",
            KeyCode::Down => "↓",
            KeyCode::Left => "←",
            KeyCode::Right => "→",
            _ => "?",
        }
    }

    /// A bare letter (no Ctrl/Alt) and its case: `Some(true)` = uppercase.
    fn bare_letter_case(self) -> Option<bool> {
        match self.code {
            KeyCode::Char(c)
                if c.is_ascii_alphabetic()
                    && !self
                        .mods
                        .intersects(KeyModifiers::CONTROL | KeyModifiers::ALT) =>
            {
                Some(c.is_ascii_uppercase())
            }
            _ => None,
        }
    }
}

/// Where a binding applies. Scopes nest: Global ⊃ Screen ⊃ Panel.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Scope {
    Global,
    Screen(TuiMode),
    Panel(TuiMode, &'static str),
}

impl Scope {
    /// True when `inner` is this scope or nested inside it. Screens compare
    /// by mode variant only, so `LogViewer(id)` matches any id.
    pub fn encloses(self, inner: Scope) -> bool {
        use std::mem::discriminant as d;
        match (self, inner) {
            (Scope::Global, _) => true,
            (Scope::Screen(a), Scope::Screen(b) | Scope::Panel(b, _)) => d(&a) == d(&b),
            (Scope::Panel(a, p), Scope::Panel(b, q)) => d(&a) == d(&b) && p == q,
            _ => false,
        }
    }

    /// True for the screen scope of `mode` (any payload).
    fn is_screen_of(self, mode: TuiMode) -> bool {
        Scope::Screen(mode).encloses(self) && matches!(self, Scope::Screen(_))
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Danger {
    Safe,
    Destructive,
}

/// What a binding does. Names only; the handler lives in dispatch.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Action {
    Palette,
    Quit,
    SwitchTab(u8),
    NextPanel,
    PrevPanel,
    Back,
    Help,
    Filter,
    Commit,
    Upgrade,
    Undo,
    ToggleLogDrawer,
    ToggleBypass,
    ScrollUp,
    ScrollDown,
    Top,
    Bottom,
    Expand,
    Kill,
    CycleAgent,
    /// Agent graph ↔ session panels (#1240).
    TogglePanels,
    /// Fetch the screen's data again (Milestones `r`, #1265).
    Reload,
    /// Launch every open issue of the milestone (Milestones `R`, #1265).
    RunAll,
    /// Dismiss the finished sessions (Dashboard `D`, #1280).
    Dismiss,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Binding {
    pub chord: Chord,
    pub scope: Scope,
    pub action: Action,
    pub danger: Danger,
    /// Footer / help text for the action.
    pub label: &'static str,
    /// Shown in the footer of its screen (aliases like `↑` are not).
    pub footer: bool,
    /// Why this binding may break a case rule (`None` = it may not).
    pub exception: Option<&'static str>,
}

impl Binding {
    pub const fn new(
        chord: Chord,
        scope: Scope,
        action: Action,
        danger: Danger,
        label: &'static str,
    ) -> Self {
        Self {
            chord,
            scope,
            action,
            danger,
            label,
            footer: true,
            exception: None,
        }
    }

    /// Keep it out of the footer (an alias of a shown key).
    pub const fn hidden(mut self) -> Self {
        self.footer = false;
        self
    }

    /// A documented exception to the case rules.
    pub const fn exception(mut self, why: &'static str) -> Self {
        self.exception = Some(why);
        self
    }
}

/// A broken §06 rule.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Violation {
    /// The same chord twice in one scope.
    Duplicate { scope: Scope, chord: Chord },
    /// An inner binding reuses a chord of a scope that encloses it.
    Shadow {
        outer: Scope,
        inner: Scope,
        chord: Chord,
    },
    /// A destructive action on a bare lowercase letter.
    LowercaseDestructive(Action),
    /// A safe action on a bare uppercase letter with no documented exception.
    UppercaseSafe(Action),
}

/// Where the TurboQuant on/off toggle lives since ⌃Q quits (#1260): a
/// Settings field, not a chord. Settings › TurboQuant › Enabled
/// (`turboquant.enabled`, built in `screens/settings/tabs/turboquant.rs`),
/// saved with ⌃S. No binding toggles it; the palette opens its dashboard.
pub const TURBOQUANT_TOGGLE: &str = "Settings › TurboQuant › Enabled";

/// `A` is the last Shift global; `Q` `M` `C` `P` were retired (#1265).
const SHIFT_GLOBAL: &str = "Shift global from before §06, checked before every screen";

/// Every declared binding: the §06 Global table, the other keys
/// `handle_global_shortcuts` takes before any screen, then one block per
/// registered screen. A new screen adds its block here and keeps
/// `keymap_is_poka_yoke_clean` green.
pub fn registry() -> &'static [Binding] {
    use Action as A;
    use Danger::{Destructive, Safe};
    use std::sync::LazyLock;
    static REGISTRY: LazyLock<Vec<Binding>> = LazyLock::new(|| {
        let g = |chord, action, danger, label| {
            Binding::new(chord, Scope::Global, action, danger, label)
        };
        let ch = |c| Chord::key(KeyCode::Char(c));
        // §06 Global. ⌃K opens the palette (#1255). ⌃Q quits
        // (#1260): an idle fleet at once, else through the Confirm Exit hold.
        // The TurboQuant toggle it replaced is [`TURBOQUANT_TOGGLE`] (a
        // Settings field, no chord); `Q` still opens the TurboQuant dashboard.
        let mut v = vec![
            g(Chord::ctrl('k'), A::Palette, Safe, "palette"),
            g(Chord::ctrl('q'), A::Quit, Destructive, "quit"),
        ];
        for (i, (digit, label, _)) in crate::tui::ds::tab_bar::TABS.iter().enumerate() {
            v.push(g(ch(*digit), A::SwitchTab(i as u8 + 1), Safe, label));
        }
        v.extend([
            g(Chord::key(KeyCode::Tab), A::NextPanel, Safe, "next panel"),
            g(
                Chord::key(KeyCode::BackTab),
                A::PrevPanel,
                Safe,
                "previous panel",
            ),
            g(Chord::key(KeyCode::Esc), A::Back, Safe, "back"),
            g(ch('?'), A::Help, Safe, "help"),
            g(Chord::key(KeyCode::F(1)), A::Help, Safe, "help"),
            g(ch('/'), A::Filter, Safe, "filter"),
            g(Chord::ctrl('s'), A::Commit, Safe, "commit"),
            g(ch('u'), A::Upgrade, Safe, "upgrade"),
            g(Chord::ctrl('z'), A::Undo, Safe, "undo"),
            // Taken before any screen today, not in the §06 table.
            g(Chord::ctrl('l'), A::ToggleLogDrawer, Safe, "log"),
            g(Chord::ctrl('b'), A::ToggleBypass, Destructive, "bypass"),
            // #1260: ⌃X and ⌃C always open Confirm Exit; `q` quits only an
            // idle fleet (nothing is lost) and asks otherwise, so it is Safe.
            g(Chord::ctrl('x'), A::Quit, Destructive, "quit"),
            g(Chord::ctrl('c'), A::Quit, Destructive, "quit"),
            g(ch('q'), A::Quit, Safe, "quit").hidden(),
            g(ch('A'), A::CycleAgent, Safe, "next agent").exception(SHIFT_GLOBAL),
        ]);

        v.extend(screens::blocks());
        v
    });
    &REGISTRY
}

/// What the ⌃K palette lists on `mode`: Global plus `mode`'s screen block,
/// the first binding per action (no `F1` after `?`), no hidden alias, no ⌃K.
pub fn actions_on(bindings: &[Binding], mode: TuiMode) -> Vec<&Binding> {
    let mut out: Vec<&Binding> = Vec::new();
    for b in bindings {
        let here = b.scope == Scope::Global || b.scope.is_screen_of(mode);
        let new = !out.iter().any(|o| o.action == b.action);
        if here && new && b.footer && b.action != Action::Palette {
            out.push(b);
        }
    }
    out
}

/// Every §06 rule `bindings` breaks; empty means clean. A chord seen twice
/// is reported once, at its second binding.
pub fn check(bindings: &[Binding]) -> Vec<Violation> {
    let mut out = Vec::new();
    for (i, b) in bindings.iter().enumerate() {
        for prev in bindings[..i].iter().filter(|p| p.chord == b.chord) {
            if prev.scope == b.scope {
                out.push(Violation::Duplicate {
                    scope: b.scope,
                    chord: b.chord,
                });
            } else if prev.scope.encloses(b.scope) || b.scope.encloses(prev.scope) {
                let (outer, inner) = if prev.scope.encloses(b.scope) {
                    (prev.scope, b.scope)
                } else {
                    (b.scope, prev.scope)
                };
                out.push(Violation::Shadow {
                    outer,
                    inner,
                    chord: b.chord,
                });
            }
        }
        match (b.chord.bare_letter_case(), b.danger) {
            (Some(false), Danger::Destructive) => {
                out.push(Violation::LowercaseDestructive(b.action))
            }
            (Some(true), Danger::Safe) if b.exception.is_none() => {
                out.push(Violation::UppercaseSafe(b.action))
            }
            _ => {}
        }
    }
    out
}

/// The footer hints of a registered screen, in registry order. `None` for a
/// mode with no screen-scope bindings.
pub fn footer_hints(mode: TuiMode) -> Option<&'static [InlineHint]> {
    use std::sync::LazyLock;
    static FOOTERS: LazyLock<Vec<(Scope, Vec<InlineHint>)>> = LazyLock::new(|| {
        let mut footers: Vec<(Scope, Vec<InlineHint>)> = Vec::new();
        for b in registry().iter().filter(|b| b.footer) {
            let Scope::Screen(_) = b.scope else { continue };
            let at = match footers.iter().position(|(s, _)| s.encloses(b.scope)) {
                Some(at) => at,
                None => {
                    footers.push((b.scope, Vec::new()));
                    footers.len() - 1
                }
            };
            let hints = &mut footers[at].1;
            hints.push(InlineHint {
                key: b.chord.label(),
                action: b.label,
                priority: hints.len().min(u8::MAX as usize) as u8,
            });
        }
        footers
    });
    FOOTERS
        .iter()
        .find(|(s, _)| s.is_screen_of(mode))
        .map(|(_, hints)| hints.as_slice())
}

#[path = "registry_screens.rs"]
mod screens;
pub use screens::{KeyOn, PALETTE_COMMANDS, PaletteCommand};

#[path = "registry_help.rs"]
mod help;
pub use help::{global_help_group, is_global, screen_help_group, tab_help_group};

#[cfg(test)]
#[path = "registry_tests.rs"]
mod tests;
