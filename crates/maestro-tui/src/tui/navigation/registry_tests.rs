use super::*;
use ratatui::{Terminal, backend::TestBackend};

use crate::tui::ds::key_hints::QUIT;
use crate::tui::{make_test_app, ui};

const LOG_VIEWER: Scope = Scope::Screen(TuiMode::LogViewer(uuid::Uuid::nil()));

fn ch(c: char) -> Chord {
    Chord::key(KeyCode::Char(c))
}

fn global(chord: Chord, action: Action) -> Binding {
    Binding::new(chord, Scope::Global, action, Danger::Safe, "x")
}

fn screen(chord: Chord, action: Action, danger: Danger) -> Binding {
    Binding::new(chord, LOG_VIEWER, action, danger, "x")
}

#[test]
fn registry_has_section_06_globals() {
    let mut want = vec![
        (Chord::ctrl('k'), Action::Palette),
        (Chord::ctrl('q'), Action::Quit),
        (Chord::key(KeyCode::Tab), Action::NextPanel),
        (Chord::key(KeyCode::BackTab), Action::PrevPanel),
        (Chord::key(KeyCode::Esc), Action::Back),
        (ch('?'), Action::Help),
        (Chord::key(KeyCode::F(1)), Action::Help),
        (ch('/'), Action::Filter),
        (Chord::ctrl('s'), Action::Commit),
        (ch('u'), Action::Upgrade),
        (Chord::ctrl('z'), Action::Undo),
    ];
    for (i, d) in ('1'..='7').enumerate() {
        want.push((ch(d), Action::SwitchTab(i as u8 + 1)));
    }
    for (chord, action) in want {
        assert!(
            registry()
                .iter()
                .any(|b| b.scope == Scope::Global && b.chord == chord && b.action == action),
            "§06 Global {} → {action:?} missing from the registry",
            chord.label()
        );
    }
}

/// #1260 (and the #1248 judge note): the keys taken before any screen
/// today are declared, `A` as a documented Shift-global exception.
#[test]
fn registry_declares_quit_keys_and_cycle_agent() {
    let has = |chord: Chord, action| {
        registry()
            .iter()
            .any(|b| b.scope == Scope::Global && b.chord == chord && b.action == action)
    };
    for chord in [
        Chord::ctrl('q'),
        Chord::ctrl('x'),
        Chord::ctrl('c'),
        ch('q'),
    ] {
        assert!(has(chord, Action::Quit), "{} → Quit", chord.label());
    }
    assert!(has(ch('A'), Action::CycleAgent));
    let a = registry().iter().find(|b| b.chord == ch('A')).unwrap();
    assert!(
        a.exception.is_some(),
        "A breaks the uppercase rule on purpose"
    );
}

/// #1260: ⌃Q is Quit only; the TurboQuant toggle is the Settings field
/// [`TURBOQUANT_TOGGLE`], so no chord toggles TurboQuant (#1265: no chord
/// opens its dashboard either; the palette does).
#[test]
fn registry_turboquant_toggle_is_a_settings_field() {
    let q = registry().iter().filter(|b| b.chord == Chord::ctrl('q'));
    assert!(q.clone().count() == 1 && q.into_iter().all(|b| b.action == Action::Quit));
    assert!(!registry().iter().any(|b| b.label.contains("TurboQuant")));
    assert_eq!(TURBOQUANT_TOGGLE, "Settings › TurboQuant › Enabled");
}

#[test]
fn registry_tab_labels_match_tab_bar() {
    for (i, (_, label, _)) in crate::tui::ds::tab_bar::TABS.iter().enumerate() {
        let b = registry()
            .iter()
            .find(|b| b.action == Action::SwitchTab(i as u8 + 1))
            .expect("tab binding");
        assert_eq!(b.label, *label);
    }
}

#[test]
fn keymap_is_poka_yoke_clean() {
    assert_eq!(check(registry()), vec![]);
}

#[test]
fn keymap_is_poka_yoke_clean_fails_on_duplicate() {
    let bad = [
        screen(ch('j'), Action::ScrollDown, Danger::Safe),
        screen(ch('j'), Action::Top, Danger::Safe),
    ];
    assert_eq!(
        check(&bad),
        vec![Violation::Duplicate {
            scope: LOG_VIEWER,
            chord: ch('j')
        }]
    );
}

#[test]
fn keymap_is_poka_yoke_clean_fails_on_shadow() {
    let bad = [
        global(ch('/'), Action::Filter),
        screen(ch('/'), Action::Top, Danger::Safe),
    ];
    assert_eq!(
        check(&bad),
        vec![Violation::Shadow {
            outer: Scope::Global,
            inner: LOG_VIEWER,
            chord: ch('/')
        }]
    );
}

#[test]
fn keymap_is_poka_yoke_clean_fails_on_panel_shadowing_its_screen() {
    let panel = Scope::Panel(TuiMode::LogViewer(uuid::Uuid::nil()), "body");
    let bad = [
        screen(ch('g'), Action::Top, Danger::Safe),
        Binding::new(ch('g'), panel, Action::Expand, Danger::Safe, "x"),
    ];
    assert_eq!(
        check(&bad),
        vec![Violation::Shadow {
            outer: LOG_VIEWER,
            inner: panel,
            chord: ch('g')
        }]
    );
}

#[test]
fn keymap_is_poka_yoke_clean_allows_same_chord_on_two_screens() {
    let ok = [
        screen(ch('j'), Action::ScrollDown, Danger::Safe),
        Binding::new(
            ch('j'),
            Scope::Screen(TuiMode::SessionSummary),
            Action::ScrollDown,
            Danger::Safe,
            "x",
        ),
    ];
    assert_eq!(check(&ok), vec![]);
}

#[test]
fn keymap_is_poka_yoke_clean_fails_on_lowercase_destructive() {
    let bad = [screen(ch('k'), Action::Quit, Danger::Destructive)];
    assert_eq!(
        check(&bad),
        vec![Violation::LowercaseDestructive(Action::Quit)]
    );
    let ok = [screen(ch('K'), Action::Quit, Danger::Destructive)];
    assert_eq!(check(&ok), vec![]);
}

#[test]
fn keymap_is_poka_yoke_clean_fails_on_uppercase_safe() {
    let bad = [screen(ch('G'), Action::Bottom, Danger::Safe)];
    assert_eq!(check(&bad), vec![Violation::UppercaseSafe(Action::Bottom)]);
    let ok = [screen(ch('G'), Action::Bottom, Danger::Safe).exception("vim pair")];
    assert_eq!(check(&ok), vec![]);
}

#[test]
fn keymap_case_rules_skip_ctrl_chords_and_symbols() {
    let ok = [
        Binding::new(
            Chord::ctrl('q'),
            Scope::Global,
            Action::Quit,
            Danger::Destructive,
            "x",
        ),
        global(ch('?'), Action::Help),
    ];
    assert_eq!(check(&ok), vec![]);
}

#[test]
fn chord_labels() {
    assert_eq!(Chord::ctrl('k').label(), "⌃K");
    assert_eq!(ch('G').label(), "G");
    assert_eq!(ch('?').label(), "?");
    assert_eq!(Chord::key(KeyCode::Enter).label(), "Enter");
    assert_eq!(Chord::key(KeyCode::BackTab).label(), "⇧Tab");
    assert_eq!(Chord::key(KeyCode::F(1)).label(), "F1");
}

fn footer_row(mode: TuiMode) -> String {
    let mut app = make_test_app("keymap-registry");
    app.show_mascot = false;
    app.show_activity_log = false;
    app.no_splash = true;
    app.tui_mode = mode;
    let mut t = Terminal::new(TestBackend::new(80, 24)).unwrap();
    t.draw(|f| ui::draw(f, &mut app)).unwrap();
    let buf = t.backend().buffer();
    let quit = format!("{} {}", QUIT.0, QUIT.1);
    (0..buf.area.height)
        .rev()
        .map(|y| {
            (0..buf.area.width)
                .map(|x| buf[(x, y)].symbol())
                .collect::<String>()
        })
        .find(|row| row.contains(&quit))
        .expect("footer row with the quit hint")
}

#[test]
fn footer_hints_from_registry() {
    for mode in [
        TuiMode::LogViewer(uuid::Uuid::new_v4()),
        TuiMode::SessionSummary,
        TuiMode::ConfirmKill(uuid::Uuid::new_v4()),
    ] {
        let hints = footer_hints(mode).expect("registered screen");
        let want: Vec<(&str, &str)> = registry()
            .iter()
            .filter(|b| b.footer && matches!(b.scope, Scope::Screen(m) if std::mem::discriminant(&m) == std::mem::discriminant(&mode)))
            .map(|b| (b.chord.label(), b.label))
            .collect();
        assert!(!want.is_empty(), "{mode:?} has no footer bindings");
        let got: Vec<(&str, &str)> = hints.iter().map(|h| (h.key, h.action)).collect();
        assert_eq!(got, want);

        let row = footer_row(mode);
        for (key, label) in &want {
            let pair = format!("{key} {label}");
            assert!(
                row.contains(&pair),
                "{mode:?}: {pair:?} not drawn in {row:?}"
            );
        }
    }
}

#[test]
fn footer_hints_from_registry_snapshot() {
    let log = footer_row(TuiMode::LogViewer(uuid::Uuid::new_v4()));
    let summary = footer_row(TuiMode::SessionSummary);
    insta::assert_snapshot!(format!("{}\n{}", log.trim_end(), summary.trim_end()));
}

#[test]
fn footer_hints_none_for_unregistered_screen() {
    assert!(footer_hints(TuiMode::Dashboard).is_none());
}
