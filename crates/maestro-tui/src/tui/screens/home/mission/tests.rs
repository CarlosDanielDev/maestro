use super::*;

const OPEN: ActionGate = ActionGate {
    gh_auth_ok: true,
    has_milestone: Some(true),
};

fn index_of(key: char) -> usize {
    actions().iter().position(|a| a.key == key).unwrap()
}

#[test]
fn actions_are_grouped_with_the_old_letters_and_no_quit() {
    let keys: String = actions().iter().map(|a| a.key).collect();
    assert_eq!(keys, "dfaimrphst");
    assert_eq!(actions()[0].mode, TuiMode::Dashboard);
    assert_eq!(actions()[index_of('h')].mode, TuiMode::MilestoneHealth);
}

#[test]
fn disabled_reasons_name_the_cause() {
    let signed_out = ActionGate {
        gh_auth_ok: false,
        ..OPEN
    };
    for key in ['i', 'm', 'h'] {
        assert_eq!(disabled_reason(key, &signed_out), Some("gh signed out"));
    }
    let none_yet = ActionGate {
        has_milestone: Some(false),
        ..OPEN
    };
    assert_eq!(disabled_reason('h', &none_yet), Some("no milestones yet"));
    let loading = ActionGate {
        has_milestone: None,
        ..OPEN
    };
    assert_eq!(disabled_reason('h', &loading), None, "unknown is not no");
    for a in actions() {
        assert_eq!(disabled_reason(a.key, &OPEN), None, "{}", a.key);
    }
}

#[test]
fn needs_you_has_focus_on_open_and_enter_opens_the_row() {
    let mut s = MissionControl::default();
    assert_eq!(s.focus, HomeFocus::NeedsYou);
    assert_eq!(s.handle_key(KeyCode::Down, 3, &OPEN), MissionCommand::None);
    assert_eq!(
        s.handle_key(KeyCode::Enter, 3, &OPEN),
        MissionCommand::OpenAttention(1)
    );
    s.handle_key(KeyCode::Down, 3, &OPEN);
    s.handle_key(KeyCode::Down, 3, &OPEN);
    assert_eq!(s.attention_selected, 2, "stops at the last row");
    s.handle_key(KeyCode::Char('k'), 3, &OPEN);
    assert_eq!(s.attention_selected, 1);
}

#[test]
fn enter_on_an_empty_queue_does_nothing() {
    let mut s = MissionControl::default();
    assert_eq!(s.handle_key(KeyCode::Enter, 0, &OPEN), MissionCommand::None);
}

#[test]
fn tab_moves_focus_to_actions_and_enter_runs_the_selected_one() {
    let mut s = MissionControl::default();
    s.handle_key(KeyCode::Tab, 2, &OPEN);
    assert_eq!(s.focus, HomeFocus::Actions);
    s.handle_key(KeyCode::Down, 2, &OPEN);
    assert_eq!(
        s.handle_key(KeyCode::Enter, 2, &OPEN),
        MissionCommand::Push(TuiMode::FleetBoard)
    );
    s.handle_key(KeyCode::BackTab, 2, &OPEN);
    assert_eq!(s.focus, HomeFocus::NeedsYou);
}

#[test]
fn tab_stays_on_needs_you_when_actions_are_hidden() {
    let mut s = MissionControl {
        actions_visible: false,
        ..MissionControl::default()
    };
    s.handle_key(KeyCode::Tab, 2, &OPEN);
    assert_eq!(s.focus, HomeFocus::NeedsYou);
}

#[test]
fn letters_run_actions_from_any_panel() {
    let mut s = MissionControl::default();
    assert_eq!(
        s.handle_key(KeyCode::Char('i'), 0, &OPEN),
        MissionCommand::Push(TuiMode::IssueWizard)
    );
    assert_eq!(
        s.handle_key(KeyCode::Char('n'), 0, &OPEN),
        MissionCommand::Push(TuiMode::ReleaseNotes)
    );
    assert_eq!(
        s.handle_key(KeyCode::Char('w'), 0, &OPEN),
        MissionCommand::None
    );
}

#[test]
fn a_disabled_action_is_inert_by_letter_and_by_enter() {
    let gate = ActionGate {
        has_milestone: Some(false),
        ..OPEN
    };
    let mut s = MissionControl::default();
    assert_eq!(
        s.handle_key(KeyCode::Char('h'), 0, &gate),
        MissionCommand::None
    );
    s.handle_key(KeyCode::Tab, 0, &gate);
    s.action_selected = index_of('h');
    assert_eq!(s.handle_key(KeyCode::Enter, 0, &gate), MissionCommand::None);
}
