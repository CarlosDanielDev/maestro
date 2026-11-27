//! #1540: `j` and `k` are letters on the Project and Model text fields;
//! only the arrows move focus from them. The Stop after select keeps `j` /
//! `k`. Every test goes through `handle_key` + `ui::draw` at >= 80x24 (the
//! SizeGuard swallows keys below it).

use super::AdaptScreen;
use super::types::AdaptResults;
use crate::tui::app::{App, TuiMode};
use crate::tui::input_handler::handle_key;
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

/// The Adapt screen on Configure, Project focused, path `path`. No cache
/// from the CWD: the fixture sets results and path itself.
fn adapt_app(path: &str) -> App {
    let mut app = crate::tui::make_test_app("adapt-jk-1540");
    app.no_splash = true;
    app.show_mascot = false;
    app.navigate_to(TuiMode::AdaptWizard);
    let s = screen(&mut app);
    s.results = AdaptResults::default();
    s.loaded_from_cache = false;
    s.config.path = path.into();
    app
}

fn screen(app: &mut App) -> &mut AdaptScreen {
    app.screen_state
        .adapt_screen
        .get_or_insert_with(AdaptScreen::new)
}

async fn press(app: &mut App, code: KeyCode) {
    handle_key(app, KeyEvent::new(code, KeyModifiers::NONE)).await;
}

async fn type_str(app: &mut App, text: &str) {
    for c in text.chars() {
        press(app, KeyCode::Char(c)).await;
    }
}

fn shown(app: &mut App, w: u16, h: u16) -> String {
    let mut t = ratatui::Terminal::new(ratatui::backend::TestBackend::new(w, h)).unwrap();
    t.draw(|f| crate::tui::ui::draw(f, app)).unwrap();
    format!("{}", t.backend())
}

#[tokio::test]
async fn project_field_types_j_and_k() {
    // A short relative path that exists nowhere, so the drawn field shows
    // it whole and typing never finds a cache.
    let base = "no-such-dir-1540/work/";
    let mut app = adapt_app(base);
    shown(&mut app, 120, 34);

    type_str(&mut app, "kyte-j").await;

    assert_eq!(screen(&mut app).selected_field, 0, "j / k never move focus");
    assert_eq!(screen(&mut app).config.path, format!("{base}kyte-j"));
    let drawn = shown(&mut app, 120, 34);
    assert!(drawn.contains("work/kyte-j"), "the typed path is drawn:\n{drawn}");

    press(&mut app, KeyCode::Down).await;
    assert_eq!(screen(&mut app).selected_field, 1, "↓ moves from Project");
    press(&mut app, KeyCode::Up).await;
    assert_eq!(screen(&mut app).selected_field, 0, "↑ moves back");
}

#[tokio::test]
async fn model_field_types_j_and_k_stop_after_keeps_jk() {
    let dir = tempfile::TempDir::new().unwrap();
    let mut app = adapt_app(&dir.path().display().to_string());
    shown(&mut app, 80, 24);

    press(&mut app, KeyCode::Down).await;
    assert_eq!(screen(&mut app).selected_field, 1);
    // Stop after is a select: `j` still moves down from it.
    press(&mut app, KeyCode::Char('j')).await;
    assert_eq!(screen(&mut app).selected_field, 2, "j on Stop after moves");

    screen(&mut app).config.model.clear();
    type_str(&mut app, "haiku-jk").await;
    assert_eq!(screen(&mut app).selected_field, 2, "j / k never move focus");
    assert_eq!(screen(&mut app).config.model, "haiku-jk");
    assert!(
        shown(&mut app, 80, 24).contains("haiku-jk"),
        "the typed model is drawn"
    );

    press(&mut app, KeyCode::Up).await;
    assert_eq!(screen(&mut app).selected_field, 1, "↑ moves from Model");
    // `k` on Stop after moves up.
    press(&mut app, KeyCode::Char('k')).await;
    assert_eq!(screen(&mut app).selected_field, 0, "k on Stop after moves");
}

#[tokio::test]
async fn configure_hint_never_shows_jk_on_a_text_field() {
    let dir = tempfile::TempDir::new().unwrap();
    let mut app = adapt_app(&dir.path().display().to_string());
    shown(&mut app, 80, 24);

    for field in [0, 2] {
        while screen(&mut app).selected_field < field {
            press(&mut app, KeyCode::Down).await;
        }
        assert_eq!(screen(&mut app).selected_field, field);

        let footer = shown(&mut app, 80, 24);
        assert!(!footer.contains("j/k"), "field {field} footer:\n{footer}");
        assert!(
            footer.contains("↑↓ Navigate"),
            "field {field} footer:\n{footer}"
        );

        press(&mut app, KeyCode::F(1)).await;
        let help = shown(&mut app, 120, 40);
        assert!(help.contains("Move down"), "Help is open:\n{help}");
        assert!(
            !help.contains("j/Down") && !help.contains("k/Up"),
            "field {field} Help:\n{help}"
        );
        press(&mut app, KeyCode::F(1)).await;
        assert!(app.help_state.is_none(), "Help closed");
    }
}
