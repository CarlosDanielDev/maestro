use super::*;
use crate::tui::screens::Screen;
use crate::tui::screens::issue_browser::{
    FilterMode, IssueBrowserScreen, IssuePromptOverlay, LaunchFocus,
};
use crate::tui::theme::Theme;
use insta::assert_snapshot;

#[test]
fn issue_browser_with_issues() {
    let mut terminal = test_terminal();
    let theme = Theme::dark();
    let mut screen = IssueBrowserScreen::new(vec![
        make_gh_issue(1, "Add login flow"),
        make_gh_issue(2, "Fix database crash"),
        make_gh_issue(3, "Add logout endpoint"),
    ]);

    terminal
        .draw(|f| {
            screen.draw(f, f.area(), &theme);
        })
        .unwrap();

    assert_snapshot!(terminal.backend());
}

#[test]
fn issue_browser_empty_list() {
    let mut terminal = test_terminal();
    let theme = Theme::dark();
    let mut screen = IssueBrowserScreen::new(vec![]);

    terminal
        .draw(|f| {
            screen.draw(f, f.area(), &theme);
        })
        .unwrap();

    assert_snapshot!(terminal.backend());
}

#[test]
fn issue_browser_loading_state() {
    let mut terminal = test_terminal();
    let theme = Theme::dark();
    let mut screen = IssueBrowserScreen::new(vec![]);
    screen.loading = true;

    terminal
        .draw(|f| {
            screen.draw(f, f.area(), &theme);
        })
        .unwrap();

    assert_snapshot!(terminal.backend());
}

#[test]
fn issue_browser_multi_select() {
    let mut terminal = test_terminal();
    let theme = Theme::dark();
    let mut screen = IssueBrowserScreen::new(vec![
        make_gh_issue(1, "Add login flow"),
        make_gh_issue(2, "Fix database crash"),
        make_gh_issue(3, "Add logout endpoint"),
    ]);
    screen.selected_set.insert(1);
    screen.selected_set.insert(3);

    terminal
        .draw(|f| {
            screen.draw(f, f.area(), &theme);
        })
        .unwrap();

    assert_snapshot!(terminal.backend());
}

#[test]
fn issue_browser_filter_active() {
    let mut terminal = test_terminal();
    let theme = Theme::dark();
    let mut screen = IssueBrowserScreen::new(vec![
        make_gh_issue(1, "Add login flow"),
        make_gh_issue(2, "Fix database crash"),
        make_gh_issue(3, "Add logout endpoint"),
    ]);
    screen.filter_mode = FilterMode::Label;
    screen.filter_text = "Add".to_string();

    terminal
        .draw(|f| {
            screen.draw(f, f.area(), &theme);
        })
        .unwrap();

    assert_snapshot!(terminal.backend());
}

#[test]
fn issue_browser_prompt_overlay_empty() {
    let mut terminal = test_terminal();
    let theme = Theme::dark();
    let mut screen = IssueBrowserScreen::new(vec![
        make_gh_issue(1, "Add login flow"),
        make_gh_issue(2, "Fix database crash"),
    ]);
    screen.prompt_overlay = Some(IssuePromptOverlay {
        editor: IssuePromptOverlay::make_editor(""),
        selected_issues: vec![(1, "Add login flow".to_string())],
        unified_pr: false,
        focus: LaunchFocus::Prompt,
        produce_pr: true,
        interaction: false,
    });

    terminal
        .draw(|f| {
            screen.draw(f, f.area(), &theme);
        })
        .unwrap();

    assert_snapshot!(terminal.backend());
}

#[test]
fn issue_browser_prompt_overlay_with_text() {
    let mut terminal = test_terminal();
    let theme = Theme::dark();
    let mut screen = IssueBrowserScreen::new(vec![
        make_gh_issue(1, "Add login flow"),
        make_gh_issue(2, "Fix database crash"),
    ]);
    screen.prompt_overlay = Some(IssuePromptOverlay {
        editor: IssuePromptOverlay::make_editor("focus on error handling"),
        selected_issues: vec![(1, "Add login flow".to_string())],
        unified_pr: false,
        focus: LaunchFocus::Prompt,
        produce_pr: true,
        interaction: false,
    });

    terminal
        .draw(|f| {
            screen.draw(f, f.area(), &theme);
        })
        .unwrap();

    assert_snapshot!(terminal.backend());
}

#[test]
fn issue_browser_launch_options_default() {
    let mut terminal = test_terminal();
    let theme = Theme::dark();
    let mut screen = IssueBrowserScreen::new(vec![
        make_gh_issue(1, "Add login flow"),
        make_gh_issue(2, "Fix database crash"),
    ]);
    screen.prompt_overlay = Some(IssuePromptOverlay {
        editor: IssuePromptOverlay::make_editor(""),
        selected_issues: vec![(1, "Add login flow".to_string())],
        unified_pr: false,
        focus: LaunchFocus::Prompt,
        produce_pr: true,
        interaction: false,
    });

    terminal
        .draw(|f| {
            screen.draw(f, f.area(), &theme);
        })
        .unwrap();

    assert_snapshot!(terminal.backend());
}

#[test]
fn issue_browser_launch_options_toggled() {
    let mut terminal = test_terminal();
    let theme = Theme::dark();
    let mut screen = IssueBrowserScreen::new(vec![
        make_gh_issue(1, "Add login flow"),
        make_gh_issue(2, "Fix database crash"),
    ]);
    screen.prompt_overlay = Some(IssuePromptOverlay {
        editor: IssuePromptOverlay::make_editor(""),
        selected_issues: vec![(1, "Add login flow".to_string())],
        unified_pr: false,
        focus: LaunchFocus::Interaction,
        produce_pr: false,
        interaction: true,
    });

    terminal
        .draw(|f| {
            screen.draw(f, f.area(), &theme);
        })
        .unwrap();

    assert_snapshot!(terminal.backend());
}

#[test]
fn issue_browser_focused_checkbox_row_has_selection_background() {
    // Snapshots capture text only, not style. This asserts the focused row is
    // painted with the shared selection bar (`selection_bg`), the same full-row
    // highlight as the selected issue-list row — not just a marker glyph.
    let mut terminal = test_terminal();
    let theme = Theme::dark();
    let mut screen = IssueBrowserScreen::new(vec![make_gh_issue(1, "Add login flow")]);
    screen.prompt_overlay = Some(IssuePromptOverlay {
        editor: IssuePromptOverlay::make_editor(""),
        selected_issues: vec![(1, "Add login flow".to_string())],
        unified_pr: false,
        focus: LaunchFocus::Interaction,
        produce_pr: true,
        interaction: false,
    });

    terminal
        .draw(|f| {
            screen.draw(f, f.area(), &theme);
        })
        .unwrap();

    let buf = terminal.backend().buffer().clone();
    let found = (0..buf.area.height)
        .any(|y| (0..buf.area.width).any(|x| buf[(x, y)].style().bg == Some(theme.selection_bg)));
    assert!(
        found,
        "focused checkbox row must paint a full selection-background bar"
    );
}

#[test]
fn issue_browser_launch_options_launch_focused() {
    let mut terminal = test_terminal();
    let theme = Theme::dark();
    let mut screen = IssueBrowserScreen::new(vec![
        make_gh_issue(1, "Add login flow"),
        make_gh_issue(2, "Fix database crash"),
    ]);
    screen.prompt_overlay = Some(IssuePromptOverlay {
        editor: IssuePromptOverlay::make_editor(""),
        selected_issues: vec![(1, "Add login flow".to_string())],
        unified_pr: false,
        focus: LaunchFocus::Launch,
        produce_pr: true,
        interaction: false,
    });

    terminal
        .draw(|f| {
            screen.draw(f, f.area(), &theme);
        })
        .unwrap();

    assert_snapshot!(terminal.backend());
}

#[test]
fn issue_browser_multi_overlay_launch_options_default() {
    // #919: the multi-issue overlay renders the same checkbox rows as the
    // single-issue dialog, below the prompt textarea.
    let mut terminal = test_terminal();
    let theme = Theme::dark();
    let mut screen = IssueBrowserScreen::new(vec![
        make_gh_issue(1, "Add login flow"),
        make_gh_issue(2, "Fix database crash"),
    ]);
    screen.prompt_overlay = Some(IssuePromptOverlay {
        editor: IssuePromptOverlay::make_editor(""),
        selected_issues: vec![
            (1, "Add login flow".to_string()),
            (2, "Fix database crash".to_string()),
        ],
        unified_pr: false,
        focus: LaunchFocus::Prompt,
        produce_pr: true,
        interaction: false,
    });

    terminal
        .draw(|f| {
            screen.draw(f, f.area(), &theme);
        })
        .unwrap();

    assert_snapshot!(terminal.backend());
}

#[test]
fn issue_browser_multi_overlay_launch_options_toggled() {
    let mut terminal = test_terminal();
    let theme = Theme::dark();
    let mut screen = IssueBrowserScreen::new(vec![
        make_gh_issue(1, "Add login flow"),
        make_gh_issue(2, "Fix database crash"),
    ]);
    screen.prompt_overlay = Some(IssuePromptOverlay {
        editor: IssuePromptOverlay::make_editor(""),
        selected_issues: vec![
            (1, "Add login flow".to_string()),
            (2, "Fix database crash".to_string()),
        ],
        unified_pr: true,
        focus: LaunchFocus::Interaction,
        produce_pr: false,
        interaction: true,
    });

    terminal
        .draw(|f| {
            screen.draw(f, f.area(), &theme);
        })
        .unwrap();

    assert_snapshot!(terminal.backend());
}
