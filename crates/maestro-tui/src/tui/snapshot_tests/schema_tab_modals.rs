//! Snapshot the Add/Remove modals at 80×24. Locks the visual contract so
//! `theme.branding_bg` cannot leak back into either popup (issue #808).

use insta::assert_snapshot;
use ratatui::{Terminal, backend::TestBackend};

use crate::tui::screens::settings::schema_tab::modals::add_entry::AddEntryModal;
use crate::tui::screens::settings::schema_tab::modals::remove_confirm::RemoveConfirmModal;
use crate::tui::theme::Theme;

#[test]
fn add_entry_modal_renders_80x24() {
    let modal = AddEntryModal::new("Add agent", vec!["gpt4".to_string()]);
    let mut terminal = Terminal::new(TestBackend::new(80, 24)).unwrap();
    let theme = Theme::dark();
    terminal
        .draw(|f| {
            modal.draw(f, f.area(), &theme);
        })
        .unwrap();
    assert_snapshot!(terminal.backend());
}

#[test]
fn remove_confirm_modal_renders_80x24() {
    let modal = RemoveConfirmModal::new("agents.claude");
    let mut terminal = Terminal::new(TestBackend::new(80, 24)).unwrap();
    let theme = Theme::dark();
    terminal
        .draw(|f| {
            modal.draw(f, f.area(), &theme);
        })
        .unwrap();
    assert_snapshot!(terminal.backend());
}
