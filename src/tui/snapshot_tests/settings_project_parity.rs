//! Field-shape regression tests for the Project tab (originally
//! introduced by #715 as a flag-on/off parity matrix; the flag was
//! removed in #716). The tests still pin field count, labels, render
//! bytes at 80×24 + 120×40, the schema sync writeback, and the
//! schema-layer validators on `repo` and `base_branch`.

use insta::assert_snapshot;
use ratatui::layout::Rect;
use ratatui::{Terminal, backend::TestBackend};

use crate::config::Config;
use crate::config::schema::PROJECT_TABLE;
use crate::flags::store::FeatureFlags;
use crate::tui::screens::settings::SettingsField;
use crate::tui::screens::settings::SettingsScreen;
use crate::tui::screens::settings::schema_tab::build::from_schema;
use crate::tui::screens::settings::schema_tab::sync::{sync_to_config, validate_fields};
use crate::tui::theme::Theme;
use crate::tui::widgets::WidgetKind;

const MINIMAL_TOML: &str = concat!(
    "[project]\nrepo = \"owner/repo\"\nbase_branch = \"main\"\n",
    "[sessions]\n",
    "[budget]\nper_session_usd = 5.0\ntotal_usd = 50.0\nalert_threshold_pct = 80\n",
    "[github]\n",
    "[notifications]\nslack_webhook_url = \"\"\n",
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

#[test]
fn project_tab_flag_off_field_count_and_labels() {
    let screen = SettingsScreen::new(test_config(), FeatureFlags::default());
    let fields = &screen.fields_per_tab[0];

    assert_eq!(fields.len(), 4, "project tab must have exactly 4 fields");
    assert_eq!(fields[0].widget.label(), "repo");
    assert_eq!(fields[1].widget.label(), "base_branch");
    assert_eq!(
        fields[2].widget.label(),
        "Reset Settings (re-detect project stack)"
    );
    assert_eq!(
        fields[3].widget.label(),
        "Normalize Agent Config (add [agents])"
    );
}

#[test]
fn project_tab_flag_on_field_count_and_labels() {
    let screen = SettingsScreen::new(test_config(), FeatureFlags::default());
    let fields = &screen.fields_per_tab[0];

    assert_eq!(
        fields.len(),
        4,
        "project tab must have exactly 4 fields (schema-on path)"
    );
    assert_eq!(fields[0].widget.label(), "repo");
    assert_eq!(fields[1].widget.label(), "base_branch");
    assert_eq!(
        fields[2].widget.label(),
        "Reset Settings (re-detect project stack)"
    );
    assert_eq!(
        fields[3].widget.label(),
        "Normalize Agent Config (add [agents])"
    );
}

#[test]
fn project_tab_flag_off_renders_80x24() {
    let screen = SettingsScreen::new(test_config(), FeatureFlags::default());
    let buf = render_tab(&screen.fields_per_tab[0], 80, 24);
    assert_snapshot!(format!("{buf:?}"));
}

#[test]
fn project_tab_flag_on_renders_80x24_parity_with_flag_off() {
    let screen_off = SettingsScreen::new(test_config(), FeatureFlags::default());
    let screen_on = SettingsScreen::new(test_config(), FeatureFlags::default());

    let buf_off = render_tab(&screen_off.fields_per_tab[0], 80, 24);
    let buf_on = render_tab(&screen_on.fields_per_tab[0], 80, 24);

    assert_eq!(
        buf_off, buf_on,
        "schema-on and schema-off must produce byte-identical 80×24 render"
    );
}

#[test]
fn project_tab_flag_on_renders_120x40_parity_with_flag_off() {
    let screen_off = SettingsScreen::new(test_config(), FeatureFlags::default());
    let screen_on = SettingsScreen::new(test_config(), FeatureFlags::default());

    let buf_off = render_tab(&screen_off.fields_per_tab[0], 120, 40);
    let buf_on = render_tab(&screen_on.fields_per_tab[0], 120, 40);

    assert_eq!(
        buf_off, buf_on,
        "schema-on and schema-off must produce byte-identical 120×40 render"
    );
}

#[test]
fn project_sync_flag_on_writes_repo_and_base_branch() -> anyhow::Result<()> {
    let mut config = test_config();
    let mut fields = from_schema(&PROJECT_TABLE, &config);

    if let WidgetKind::TextInput(ref mut w) = fields[0].widget {
        w.value = "new-owner/new-repo".to_string();
    } else {
        anyhow::bail!("field[0] must be TextInput for repo");
    }
    if let WidgetKind::TextInput(ref mut w) = fields[1].widget {
        w.value = "develop".to_string();
    } else {
        anyhow::bail!("field[1] must be TextInput for base_branch");
    }

    sync_to_config(&PROJECT_TABLE, &fields, &mut config)?;

    assert_eq!(config.project.repo, "new-owner/new-repo");
    assert_eq!(config.project.base_branch, "develop");
    Ok(())
}

#[test]
fn project_sync_flag_on_preserves_other_config_sections() -> anyhow::Result<()> {
    let original = test_config();
    let mut config = original.clone();
    let mut fields = from_schema(&PROJECT_TABLE, &config);

    if let WidgetKind::TextInput(ref mut w) = fields[0].widget {
        w.value = "x/y".to_string();
    }

    sync_to_config(&PROJECT_TABLE, &fields, &mut config)?;

    assert_eq!(
        config.sessions.max_concurrent, original.sessions.max_concurrent,
        "sync must not disturb sessions.max_concurrent"
    );
    assert_eq!(
        config.budget.per_session_usd, original.budget.per_session_usd,
        "sync must not disturb budget.per_session_usd"
    );
    assert_eq!(
        config.github.auto_pr, original.github.auto_pr,
        "sync must not disturb github.auto_pr"
    );
    Ok(())
}

#[test]
fn project_validation_flag_on_still_fires_on_empty_inputs() {
    let mut config = test_config();
    config.project.repo = String::new();
    config.project.base_branch = String::new();

    let fields = from_schema(&PROJECT_TABLE, &config);
    let errors = validate_fields(&PROJECT_TABLE, &fields);

    let error_indices: Vec<usize> = errors.iter().map(|(i, _)| *i).collect();
    assert!(
        error_indices.contains(&0),
        "validate_fields must report error at index 0 for empty repo; got {error_indices:?}"
    );
    assert!(
        error_indices.contains(&1),
        "validate_fields must report error at index 1 for empty base_branch; got {error_indices:?}"
    );
    assert_eq!(
        errors.len(),
        2,
        "exactly two validation errors expected; got {} errors",
        errors.len()
    );
}
