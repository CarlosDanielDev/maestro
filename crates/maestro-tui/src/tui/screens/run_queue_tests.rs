use super::*;
use crate::tui::screens::test_helpers::key_event;
use crossterm::event::KeyModifiers;

fn run(waves: Vec<Vec<u64>>) -> RunQueue {
    let titles = waves
        .iter()
        .flatten()
        .map(|&n| (n, format!("Issue {n}")))
        .collect();
    let open = waves.iter().map(Vec::len).sum();
    let plan = RunPlan {
        waves,
        ..Default::default()
    };
    RunQueue::new("v1".into(), open, plan, titles)
}

#[test]
fn enter_without_a_wave_opens_nothing() {
    let mut q = run(vec![]);
    assert_eq!(
        q.handle_input(&key_event(KeyCode::Enter)),
        ScreenAction::None
    );
    assert!(q.launch_dialog.is_none());
}

#[test]
fn esc_goes_back_without_launching() {
    let mut q = run(vec![vec![1, 2]]);
    assert_eq!(q.handle_input(&key_event(KeyCode::Esc)), ScreenAction::Pop);
}

#[test]
fn confirm_one_session_launches_it() {
    let mut q = run(vec![vec![7]]);
    q.handle_input(&key_event(KeyCode::Enter));
    match q.handle_input(&key_event(KeyCode::Enter)) {
        ScreenAction::LaunchSession(c) => assert_eq!(c.issue_number, Some(7)),
        other => panic!("expected LaunchSession, got {other:?}"),
    }
}

#[test]
fn confirm_many_queues_them_without_a_second_review() {
    let mut q = run(vec![vec![1, 2], vec![3]]);
    q.handle_input(&key_event(KeyCode::Enter));
    match q.handle_input(&key_event(KeyCode::Enter)) {
        ScreenAction::LaunchQueue(c) => {
            let n: Vec<_> = c.iter().map(|c| c.issue_number).collect();
            assert_eq!(n, vec![Some(1), Some(2)], "wave 1 only");
        }
        other => panic!("expected LaunchQueue, got {other:?}"),
    }
}

#[test]
fn unified_toggle_launches_one_session() {
    let mut q = run(vec![vec![1, 2]]);
    q.handle_input(&key_event(KeyCode::Enter));
    let ctrl_u = Event::Key(KeyEvent::new(KeyCode::Char('u'), KeyModifiers::CONTROL));
    q.handle_input(&ctrl_u);
    assert!(matches!(
        q.handle_input(&key_event(KeyCode::Enter)),
        ScreenAction::LaunchUnifiedSession(_)
    ));
}

#[test]
fn hold_notes_name_the_reason() {
    let outside = [Hold::BlockedOutside {
        by: 831,
        milestone: Some(61),
    }];
    assert_eq!(hold_note(&outside, false), "[X] #831 (milestone 61)");
    assert_eq!(hold_note(&[Hold::WaitsOnDeferred(5)], false), "waits on #5");
}
