use super::*;
use crate::provider::types::Issue;
use crate::tui::screens::{IssueBrowserScreen, Screen, ScreenAction};
use crate::tui::theme::Theme;
use ratatui::{Terminal, backend::TestBackend};

fn key(code: KeyCode) -> Event {
    Event::Key(KeyEvent::new(code, KeyModifiers::NONE))
}

fn budget(spent: f64) -> LaunchBudget {
    LaunchBudget {
        model: "opus".into(),
        per_session_usd: 5.0,
        total_usd: 50.0,
        alert_threshold_pct: 80,
        spent,
    }
}

fn issue(number: u64, title: &str) -> Issue {
    Issue {
        number,
        title: title.to_string(),
        body: String::new(),
        labels: vec![],
        state: "open".to_string(),
        html_url: format!("https://github.com/owner/repo/issues/{number}"),
        milestone: None,
        assignees: vec![],
    }
}

fn single(spent: f64) -> LaunchDialog {
    let mut d = LaunchDialog::new(vec![(42, "Fix crash".into())], LaunchMode::Autonomous);
    d.set_budget(budget(spent));
    d
}

fn render_screen(screen: &mut IssueBrowserScreen) -> String {
    let mut terminal = Terminal::new(TestBackend::new(100, 32)).unwrap();
    let theme = Theme::dark();
    terminal.draw(|f| screen.draw(f, f.area(), &theme)).unwrap();
    format!("{}", terminal.backend())
}

#[test]
fn launch_mode_flags() {
    assert_eq!(LaunchMode::Autonomous.flags(), (true, false));
    assert_eq!(LaunchMode::ChatUntilPr.flags(), (true, true));
    assert_eq!(LaunchMode::ChatOpen.flags(), (false, true));
    // Three modes, three distinct flag pairs: no option without effect.
    let all: std::collections::HashSet<_> = LaunchMode::ALL.iter().map(|m| m.flags()).collect();
    assert_eq!(all.len(), 3);
}

#[test]
fn launch_mode_from_defaults_keeps_behavior_launch_working() {
    assert_eq!(
        LaunchMode::from_defaults((true, false)),
        LaunchMode::Autonomous
    );
    assert_eq!(
        LaunchMode::from_defaults((false, false)),
        LaunchMode::Autonomous
    );
    assert_eq!(
        LaunchMode::from_defaults((true, true)),
        LaunchMode::ChatUntilPr
    );
    assert_eq!(
        LaunchMode::from_defaults((false, true)),
        LaunchMode::ChatOpen
    );
    for mode in LaunchMode::ALL {
        assert_eq!(LaunchMode::from_defaults(mode.flags()), mode);
    }
}

#[test]
fn launch_dialog_block_disables() {
    // 49.80 spent + 0.50 projected (10% of the $5 per-session cap) >= $50.
    let mut d = single(49.8);
    assert_eq!(
        d.block_reason().as_deref(),
        Some("over the budget cap ($50.30 of $50.00)")
    );
    assert_eq!(d.handle_input(&key(KeyCode::Enter)), LaunchAction::None);
    d.focus = LaunchFocus::Mode;
    assert_eq!(d.handle_input(&key(KeyCode::Enter)), LaunchAction::None);
    // Esc still leaves.
    assert_eq!(d.handle_input(&key(KeyCode::Esc)), LaunchAction::Cancel);

    // The screen shows the lock and its reason, and Enter launches nothing.
    let mut screen = IssueBrowserScreen::new(vec![issue(42, "Fix crash")]);
    screen.set_launch_budget(budget(49.8));
    screen.open_launch_overlay_for_selected();
    let text = render_screen(&mut screen);
    assert!(
        text.contains("⊘ locked: over the budget cap ($50.30 of $50.00)"),
        "{text}"
    );
    let action = screen.handle_input(
        &key(KeyCode::Enter),
        crate::tui::navigation::InputMode::Insert,
    );
    assert_eq!(action, ScreenAction::None);
    assert!(
        screen.launch_dialog.is_some(),
        "a blocked Enter keeps the dialog open"
    );
}

#[test]
fn launch_dialog_warn_still_launches() {
    // 39.50 + 0.50 = 40.00 = 80% of $50: Warn, not Block.
    let mut d = single(39.5);
    assert!(d.block_reason().is_none());
    assert_eq!(
        d.handle_input(&key(KeyCode::Enter)),
        LaunchAction::Confirm(None)
    );
}

#[test]
fn launch_dialog_block_counts_every_session() {
    // One session fits (48.60 + 0.50), three do not (48.60 + 1.50 >= 50).
    let mut d = LaunchDialog::new(
        vec![(1, "a".into()), (2, "b".into()), (3, "c".into())],
        LaunchMode::Autonomous,
    );
    d.set_budget(budget(48.6));
    assert!(d.block_reason().is_some(), "3 x $0.50 over the cap");
    d.unified_pr = true;
    assert!(d.block_reason().is_none(), "one unified session fits");
}

#[test]
fn launch_dialog_no_budget_never_blocks() {
    let d = LaunchDialog::new(vec![(1, "a".into())], LaunchMode::Autonomous);
    assert!(d.block_reason().is_none());
}

#[test]
fn launch_dialog_mode_keys() {
    let mut d = single(0.0);
    // Typing goes to the instructions while they have focus.
    d.handle_input(&key(KeyCode::Char('j')));
    assert_eq!(d.editor_text(), "j");
    assert_eq!(d.mode, LaunchMode::Autonomous);
    d.handle_input(&key(KeyCode::Tab));
    assert_eq!(d.focus, LaunchFocus::Mode);
    d.handle_input(&key(KeyCode::Down));
    assert_eq!(d.mode, LaunchMode::ChatUntilPr);
    d.handle_input(&key(KeyCode::Down));
    assert_eq!(d.mode, LaunchMode::ChatOpen);
    d.handle_input(&key(KeyCode::Down));
    assert_eq!(d.mode, LaunchMode::Autonomous);
    d.handle_input(&key(KeyCode::Up));
    assert_eq!(d.mode, LaunchMode::ChatOpen);
    d.handle_input(&key(KeyCode::BackTab));
    assert_eq!(d.focus, LaunchFocus::Instructions);
    assert_eq!(d.editor_text(), "j", "mode keys never type");
}

#[test]
fn launch_dialog_shift_enter_is_newline_and_enter_trims() {
    let mut d = single(0.0);
    for c in "  hi".chars() {
        d.handle_input(&key(KeyCode::Char(c)));
    }
    let shift_enter = Event::Key(KeyEvent::new(KeyCode::Enter, KeyModifiers::SHIFT));
    assert_eq!(d.handle_input(&shift_enter), LaunchAction::None);
    assert_eq!(d.editor_text(), "  hi\n");
    assert_eq!(
        d.handle_input(&key(KeyCode::Enter)),
        LaunchAction::Confirm(Some("hi".into()))
    );
}

#[test]
fn launch_dialog_ctrl_u_toggles_unified_only_for_multi() {
    let ctrl_u = Event::Key(KeyEvent::new(KeyCode::Char('u'), KeyModifiers::CONTROL));
    let mut d = single(0.0);
    d.handle_input(&ctrl_u);
    assert!(!d.unified_pr);
    let mut m = LaunchDialog::new(
        vec![(1, "a".into()), (2, "b".into())],
        LaunchMode::Autonomous,
    );
    assert_eq!(m.sessions(), 2);
    m.handle_input(&ctrl_u);
    assert!(m.unified_pr);
    assert_eq!(m.sessions(), 1);
}

#[test]
fn launch_dialog_single() {
    let mut screen = IssueBrowserScreen::new(vec![issue(42, "Fix crash on resume")]);
    screen.set_launch_budget(budget(12.0));
    screen.open_launch_overlay_for_selected();
    let text = render_screen(&mut screen);
    assert!(text.contains(".maestro/worktrees/issue-42  branch maestro/issue-42"));
    assert!(text.contains("$12.00 of $50.00 spent"));
    insta::assert_snapshot!(text);
}

#[test]
fn launch_dialog_multi() {
    let issues = vec![
        issue(1, "Add login"),
        issue(2, "Fix crash"),
        issue(3, "Add logout"),
    ];
    let mut screen = IssueBrowserScreen::new(issues);
    screen.set_launch_budget(budget(12.0));
    for _ in 0..3 {
        screen.handle_input(
            &key(KeyCode::Char(' ')),
            crate::tui::navigation::InputMode::Normal,
        );
        screen.handle_input(
            &key(KeyCode::Char('j')),
            crate::tui::navigation::InputMode::Normal,
        );
    }
    screen.handle_input(
        &key(KeyCode::Enter),
        crate::tui::navigation::InputMode::Normal,
    );
    let text = render_screen(&mut screen);
    assert!(text.contains("3 worktrees .maestro/worktrees/issue-{1,2,3}"));
    assert!(text.contains("est ≥ $0.50 each"));
    insta::assert_snapshot!(text);
}

/// #1262 ratchet: Issues launches through this dialog; the old overlay type
/// is gone from the crate source (the `rg` check in the issue stays empty).
#[test]
fn issue_prompt_overlay_is_gone() {
    fn walk(dir: &std::path::Path, needle: &str, hits: &mut Vec<String>) {
        for entry in std::fs::read_dir(dir).unwrap().flatten() {
            let path = entry.path();
            if path.is_dir() {
                walk(&path, needle, hits);
            } else if path.extension().is_some_and(|e| e == "rs")
                && std::fs::read_to_string(&path).is_ok_and(|s| s.contains(needle))
            {
                hits.push(path.display().to_string());
            }
        }
    }
    let needle = concat!("IssuePrompt", "Overlay");
    let src = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    let mut hits = Vec::new();
    walk(&src, needle, &mut hits);
    assert!(hits.is_empty(), "{needle} is back in: {hits:?}");
}
