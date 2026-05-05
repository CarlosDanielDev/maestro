use crate::provider::types::Milestone;
use crate::tui::screens::roadmap::{RoadmapEntry, RoadmapScreen, SemVer};
use crate::tui::theme::Theme;
use insta::assert_snapshot;
use ratatui::{Terminal, backend::TestBackend};

fn roadmap_entry(index: u64) -> RoadmapEntry {
    RoadmapEntry {
        milestone: Milestone {
            number: index,
            title: format!("v0.0.{index}"),
            description: String::new(),
            state: "open".to_string(),
            open_issues: 1,
            closed_issues: 0,
        },
        semver: SemVer {
            major: 0,
            minor: 0,
            patch: index as u32,
        },
        issues: Vec::new(),
    }
}

fn buffer_text(terminal: &Terminal<TestBackend>) -> String {
    let buffer = terminal.backend().buffer();
    let mut output = String::new();
    for y in 0..buffer.area.height {
        for x in 0..buffer.area.width {
            output.push_str(buffer[(x, y)].symbol());
        }
        output.push('\n');
    }
    output
}

#[test]
fn roadmap_scrolls_to_cursor_at_bottom_of_visible_window() {
    let mut terminal = Terminal::new(TestBackend::new(80, 18)).unwrap();
    let theme = Theme::dark();
    let mut screen = RoadmapScreen::new();
    screen.entries = (0..30).map(roadmap_entry).collect();
    screen.cursor = 25;

    terminal
        .draw(|f| {
            crate::tui::screens::roadmap::draw(f, f.area(), &mut screen, &theme, 0);
        })
        .unwrap();

    let output = buffer_text(&terminal);
    assert!(
        output.contains("▶ v0.0.25"),
        "focused roadmap row should be visible:\n{output}"
    );
    assert_snapshot!(terminal.backend());
}

#[test]
fn roadmap_loading_empty_state_uses_spinner() -> anyhow::Result<()> {
    let mut terminal = Terminal::new(TestBackend::new(80, 18))?;
    let theme = Theme::dark();
    let mut screen = RoadmapScreen::new();
    screen.is_loading = true;

    terminal.draw(|f| {
        crate::tui::screens::roadmap::draw(f, f.area(), &mut screen, &theme, 3);
    })?;

    let output = buffer_text(&terminal);
    assert!(
        output.contains("⠸ Fetching milestones from GitHub…"),
        "roadmap loading state should render the tick 3 braille frame:\n{output}"
    );
    assert_snapshot!(terminal.backend());
    Ok(())
}
