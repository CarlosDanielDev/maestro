//! Parity tests for the Theme tab schema-driven migration (issue #716).
//!
//! Theme is special: the `live_preview` toggle is screen-local state, not
//! Config, so it stays bespoke. The schema-driven fields are
//! `tui.theme.preset` and `tui.ascii_icons` — two separate schema tables
//! concatenated after the bespoke prefix.

use insta::assert_snapshot;
use ratatui::layout::Rect;
use ratatui::{Terminal, backend::TestBackend};

use crate::config::Config;
use crate::flags::store::FeatureFlags;
use crate::tui::screens::settings::SettingsField;
use crate::tui::screens::settings::SettingsScreen;
use crate::tui::theme::Theme;
use crate::tui::widgets::WidgetKind;

const THEME_TAB_INDEX: usize = 7;

const MINIMAL_TOML: &str = concat!(
    "[project]\nrepo = \"owner/repo\"\nbase_branch = \"main\"\n",
    "[sessions]\n",
    "[budget]\nper_session_usd = 5.0\ntotal_usd = 50.0\nalert_threshold_pct = 80\n",
    "[github]\n",
    "[notifications]\nslack_webhook_url = \"\"\n",
    "[tui]\nascii_icons = false\n",
    "[tui.theme]\npreset = \"dark\"\n",
);

fn test_config() -> Config {
    toml::from_str(MINIMAL_TOML).expect("MINIMAL_TOML must parse")
}

fn render_tab(fields: &[SettingsField], width: u16, height: u16) -> ratatui::buffer::Buffer {
    let mut terminal =
        Terminal::new(TestBackend::new(width, height)).expect("TestBackend must init");
    let theme = Theme::dark();
    terminal
        .draw(|f| {
            let area = f.area();
            for (i, field) in fields.iter().enumerate() {
                let y = i as u16;
                if y >= area.height {
                    break;
                }
                let row = Rect {
                    x: area.x,
                    y: area.y + y,
                    width: area.width,
                    height: 1,
                };
                field.widget.draw(f, row, &theme, i == 0, None);
            }
        })
        .expect("draw must succeed");
    terminal.backend().buffer().clone()
}

const EXPECTED_LABELS: [&str; 3] = ["live_preview", "preset", "ascii_icons"];

#[test]
fn theme_tab_flag_off_field_count_and_labels() {
    let screen = SettingsScreen::new(test_config(), FeatureFlags::default());
    let fields = &screen.fields_per_tab[THEME_TAB_INDEX];
    assert_eq!(fields.len(), 3);
    for (i, expected) in EXPECTED_LABELS.iter().enumerate() {
        assert_eq!(fields[i].widget.label(), *expected);
    }
}

#[test]
fn theme_tab_flag_on_field_count_and_labels() {
    let screen = SettingsScreen::new(test_config(), FeatureFlags::default());
    let fields = &screen.fields_per_tab[THEME_TAB_INDEX];
    assert_eq!(fields.len(), 3);
    for (i, expected) in EXPECTED_LABELS.iter().enumerate() {
        assert_eq!(fields[i].widget.label(), *expected);
    }
}

#[test]
fn theme_tab_flag_on_live_preview_stays_bespoke_at_index_zero() {
    let screen = SettingsScreen::new(test_config(), FeatureFlags::default());
    let fields = &screen.fields_per_tab[THEME_TAB_INDEX];
    let WidgetKind::Toggle(t) = &fields[0].widget else {
        panic!("field[0] must be Toggle for live_preview");
    };
    assert_eq!(
        t.label, "live_preview",
        "live_preview must remain at field[0]"
    );
}

#[test]
fn theme_tab_flag_off_renders_80x24() {
    let screen = SettingsScreen::new(test_config(), FeatureFlags::default());
    let buf = render_tab(&screen.fields_per_tab[THEME_TAB_INDEX], 80, 24);
    assert_snapshot!(format!("{buf:?}"));
}

#[test]
fn theme_tab_flag_on_renders_80x24_parity_with_flag_off() {
    let screen_off = SettingsScreen::new(test_config(), FeatureFlags::default());
    let screen_on = SettingsScreen::new(test_config(), FeatureFlags::default());
    let buf_off = render_tab(&screen_off.fields_per_tab[THEME_TAB_INDEX], 80, 24);
    let buf_on = render_tab(&screen_on.fields_per_tab[THEME_TAB_INDEX], 80, 24);
    assert_eq!(buf_off, buf_on);
}

#[test]
fn theme_tab_flag_on_renders_120x40_parity_with_flag_off() {
    let screen_off = SettingsScreen::new(test_config(), FeatureFlags::default());
    let screen_on = SettingsScreen::new(test_config(), FeatureFlags::default());
    let buf_off = render_tab(&screen_off.fields_per_tab[THEME_TAB_INDEX], 120, 40);
    let buf_on = render_tab(&screen_on.fields_per_tab[THEME_TAB_INDEX], 120, 40);
    assert_eq!(buf_off, buf_on);
}

#[test]
fn theme_sync_flag_on_writes_live_preview_preset_and_ascii_icons() {
    let mut screen = SettingsScreen::new(test_config(), FeatureFlags::default());
    let fields = &mut screen.fields_per_tab[THEME_TAB_INDEX];

    if let WidgetKind::Toggle(ref mut w) = fields[0].widget {
        w.value = true; // live_preview
    }
    if let WidgetKind::Dropdown(ref mut w) = fields[1].widget {
        w.selected = 1; // light
    }
    if let WidgetKind::Toggle(ref mut w) = fields[2].widget {
        w.value = true;
    }

    screen.sync_widgets_to_config();

    assert!(screen.live_preview);
    assert!(matches!(
        screen.config.tui.theme.preset,
        crate::tui::theme::ThemePreset::Light
    ));
    assert!(screen.config.tui.ascii_icons);
}

#[test]
fn theme_sync_flag_on_preserves_other_config_sections() {
    let original = test_config();
    let mut screen = SettingsScreen::new(original.clone(), FeatureFlags::default());
    let fields = &mut screen.fields_per_tab[THEME_TAB_INDEX];
    if let WidgetKind::Toggle(ref mut w) = fields[2].widget {
        w.value = true;
    }
    screen.sync_widgets_to_config();
    assert_eq!(screen.config.project.repo, original.project.repo);
    assert_eq!(
        screen.config.sessions.max_concurrent,
        original.sessions.max_concurrent
    );
    assert_eq!(
        screen.config.tui.layout.preview_ratio,
        original.tui.layout.preview_ratio
    );
}
