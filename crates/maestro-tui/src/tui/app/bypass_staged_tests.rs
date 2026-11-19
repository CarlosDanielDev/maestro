//! #1411 (judge round 3): a typed CONFIRM for a mode or a non-default
//! agent stages only that entry. The warning and the log must say where
//! it really applies: a non-default agent's mode is read when
//! `maestro run --agent <id>` starts; a mode's when its sessions spawn.

use crossterm::event::KeyCode;

use super::agents_tests::{
    AGENTS_TAB, MODES_TAB, drawn, focus_mode_row, mode_dropdown, press, settings_app, type_confirm,
};
use crate::tui::app::{App, TuiMode};
use crate::tui::screens::settings::SettingsTab;
use crate::tui::screens::settings::schema_tab::widgets::dynamic_map::MapFocus;
use crate::tui::widgets::WidgetKind;

const BYPASS: &str = "bypassPermissions";

fn map_focus(app: &App, tab: usize) -> MapFocus {
    let screen = app.screen_state.settings_screen.as_ref().expect("settings");
    (screen.fields_per_tab[tab].iter())
        .find_map(|f| match &f.widget {
            WidgetKind::DynamicMap(m) => Some(m.focus().clone()),
            _ => None,
        })
        .expect("map")
}

/// Press → on the focused Permission Mode row until the warning opens.
async fn pick_bypass(app: &mut App, tab: usize) {
    for _ in 0..6 {
        if app.tui_mode == TuiMode::BypassWarning || mode_dropdown(app, tab).0 == BYPASS {
            break;
        }
        press(app, KeyCode::Right).await;
    }
    assert_eq!(app.tui_mode, TuiMode::BypassWarning);
}

fn last_bypass_line(app: &App) -> String {
    (app.activity_log.entries().iter().rev())
        .find(|e| e.message.starts_with("Bypass staged"))
        .map(|e| e.message.clone())
        .expect("a staged line")
}

#[tokio::test]
async fn staged_bypass_says_where_it_applies() {
    // A non-default agent: Providers tab, second entry (`worker`). One ↓
    // moves from the Default-provider row onto the map's subtab strip.
    let (mut app, _dir) = settings_app("default");
    press(&mut app, KeyCode::Down).await;
    assert_eq!(map_focus(&app, AGENTS_TAB), MapFocus::SubtabStrip);
    press(&mut app, KeyCode::Char(']')).await;
    focus_mode_row(&mut app, AGENTS_TAB).await;
    pick_bypass(&mut app, AGENTS_TAB).await;
    let screen = drawn(&mut app);
    assert!(screen.contains("agents.worker.permission_mode"), "{screen}");
    assert!(screen.contains("maestro run --agent worker"), "{screen}");
    type_confirm(&mut app).await;
    let line = last_bypass_line(&app);
    assert!(line.contains("maestro run --agent worker"), "got: {line}");
    assert!(!line.contains("its sessions run in bypass"), "got: {line}");
    assert_eq!(app.pool.permission_mode(), "default");

    // A mode: its sessions spawn in bypass after the save.
    let (mut app, _dir) = settings_app("default");
    let screen = app.screen_state.settings_screen.as_mut().expect("settings");
    screen.jump_to_tab(SettingsTab::Modes);
    focus_mode_row(&mut app, MODES_TAB).await;
    pick_bypass(&mut app, MODES_TAB).await;
    let screen = drawn(&mut app);
    assert!(screen.contains("sessions in mode yolo"), "{screen}");
    type_confirm(&mut app).await;
    let line = last_bypass_line(&app);
    assert!(line.contains("sessions in mode yolo"), "got: {line}");
}
