//! Quit-confirm modal copy and layout on `InteractionScreen` (#1242).
//!
//! The modal must say what `y` does (it used to promise the worktree was
//! kept while the teardown wiped it) and stay readable at the 80x24 floor
//! with real-length worktree paths. Tests draw the screen and read the
//! buffer; `FakeClock` keeps them off real time, and no git or disk is
//! touched.

use super::InteractionScreen;
use super::lifecycle::FakeClock;
use std::path::PathBuf;

/// Draw `screen` with the quit modal open on a `w`x`h` terminal and return
/// the rendered buffer as one string.
fn draw_quit_modal_text(screen: &mut InteractionScreen, w: u16, h: u16) -> String {
    use ratatui::{Terminal, backend::TestBackend};
    screen.quit_modal_open = true;
    let mut terminal = Terminal::new(TestBackend::new(w, h)).unwrap();
    let theme = crate::tui::theme::Theme::dark();
    terminal
        .draw(|f| screen.draw_impl(f, f.area(), &theme))
        .unwrap();
    format!("{:?}", terminal.backend())
}

// #1242: the modal must say what `y` does. It used to promise the worktree
// was "kept for manual inspection" while `begin_quit_teardown` wiped it.
#[test]
fn quit_modal_text_matches_teardown() {
    let trusted = InteractionScreen::with_ports(
        42,
        PathBuf::from("/tmp/maestro/issue-42"),
        "feat/issue-42".to_string(),
        PathBuf::from("/tmp/maestro"),
        Box::new(FakeClock::new()),
    );
    let untrusted = InteractionScreen::with_ports(
        42,
        PathBuf::from("."),
        "maestro/issue-42".to_string(),
        PathBuf::new(), // empty root → teardown is skipped
        Box::new(FakeClock::new()),
    );

    for mut screen in [trusted, untrusted] {
        let drawn = draw_quit_modal_text(&mut screen, 120, 40);
        screen.begin_quit_teardown();
        let wipes = screen.take_pending_teardown_dispatch().is_some();

        assert!(
            !drawn.contains("kept for manual inspection"),
            "the modal must not promise the worktree is kept:\n{drawn}"
        );
        assert_eq!(
            drawn.contains("Removes worktree"),
            wipes,
            "modal names the removal iff teardown will run (wipes={wipes}):\n{drawn}"
        );
        if wipes {
            assert!(drawn.contains("/tmp/maestro/issue-42"), "{drawn}");
            assert!(drawn.contains("local branch feat/issue-42"), "{drawn}");
            assert!(drawn.contains("any remote branch or PR stays"), "{drawn}");
            assert!(drawn.contains("Unpushed commits are lost"), "{drawn}");
        } else {
            assert!(drawn.contains("nothing is removed"), "{drawn}");
        }
    }
}

// #1242 judge: at the 80x24 floor, with a real-length worktree path, every
// fact must still be on screen — nothing cut at the modal edge.
#[test]
fn quit_modal_fits_80x24_with_long_path() {
    let path = "/Users/x/projects/maestro/.maestro/worktrees/issue-1242";
    let mut screen = InteractionScreen::with_ports(
        1242,
        PathBuf::from(path),
        "maestro/issue-1242".to_string(),
        PathBuf::from("/Users/x/projects/maestro/.maestro/worktrees"),
        Box::new(FakeClock::new()),
    );

    let drawn = draw_quit_modal_text(&mut screen, 80, 24);

    assert!(drawn.contains(path), "full worktree path:\n{drawn}");
    assert!(
        drawn.contains("local branch maestro/issue-1242"),
        "branch:\n{drawn}"
    );
    assert!(drawn.contains("any remote branch or PR stays"), "{drawn}");
    assert!(drawn.contains("N to keep working"), "{drawn}");
}

// A path wider than the 80-col modal wraps onto more rows instead of being
// cut, and the rows below it still show. So does the RC4 warning line.
#[test]
fn quit_modal_wraps_path_wider_than_80_cols() {
    let root =
        "/Users/someone-with-a-long-name/projects/a-very-long-project-directory/.maestro/worktrees";
    let mut screen = InteractionScreen::with_ports(
        1242,
        PathBuf::from(format!("{root}/issue-1242")),
        "maestro/issue-1242".to_string(),
        PathBuf::from(root),
        Box::new(FakeClock::new()),
    );

    let drawn = draw_quit_modal_text(&mut screen, 80, 24);
    assert!(
        drawn.contains("worktrees/issue-1242"),
        "path tail cut:\n{drawn}"
    );
    assert!(drawn.contains("N to keep working"), "{drawn}");

    screen.quit_loss_acknowledged = true;
    let drawn = draw_quit_modal_text(&mut screen, 80, 24);
    assert!(
        drawn.contains("discard, N to") && drawn.contains("keep working."),
        "RC4 line cut:\n{drawn}"
    );
}
