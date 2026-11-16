//! Theme tokens v1 (#1247): four new fields, the `high-contrast` and
//! `no-color` presets, `border_focused` retired, and `NO_COLOR`/`TERM=dumb`
//! detection. Lives next to `theme.rs` via `#[path]` to keep that file small.

use super::*;

const ALL_PRESETS: [ThemePreset; 5] = [
    ThemePreset::Dark,
    ThemePreset::Light,
    ThemePreset::Retro,
    ThemePreset::HighContrast,
    ThemePreset::NoColor,
];

fn preset_theme(preset: ThemePreset) -> Theme {
    Theme::from_config(&ThemeConfig {
        preset,
        overrides: ThemeOverrides::default(),
    })
}

/// Every `field: Value` pair from the derived `Debug` output, so a test over
/// "every colour" also covers fields added later.
fn colour_values(theme: &Theme) -> Vec<String> {
    let dbg = format!("{theme:?}");
    let body = dbg
        .trim_start_matches("Theme {")
        .trim_end_matches('}')
        .trim();
    body.split(", ")
        .map(|pair| pair.split_once(": ").map_or(pair, |(_, v)| v).to_string())
        .collect()
}

#[test]
fn theme_v1_fields_present_in_every_preset() {
    let dark = Theme::dark();
    assert_eq!(dark.surface_bar, Color::Indexed(236));
    assert_eq!(dark.surface_overlay, Color::Indexed(235));
    assert_eq!(dark.status_needs_pr, Color::Blue);
    assert_eq!(dark.status_conflict, Color::LightMagenta);

    for preset in ALL_PRESETS {
        let t = preset_theme(preset);
        let fields = [
            t.surface_bar,
            t.surface_overlay,
            t.status_needs_pr,
            t.status_conflict,
        ];
        if preset == ThemePreset::NoColor {
            assert!(fields.iter().all(|c| *c == Color::Reset), "{preset:?}");
        } else {
            assert!(fields.iter().all(|c| *c != Color::Reset), "{preset:?}");
        }

        let overrides = ThemeOverrides {
            surface_bar: Some(SerializableColor(ThemeColor::Indexed(1))),
            surface_overlay: Some(SerializableColor(ThemeColor::Indexed(2))),
            status_needs_pr: Some(SerializableColor(ThemeColor::Indexed(3))),
            status_conflict: Some(SerializableColor(ThemeColor::Indexed(4))),
            ..Default::default()
        };
        let o = Theme::from_config(&ThemeConfig { preset, overrides });
        assert_eq!(o.surface_bar, Color::Indexed(1), "{preset:?}");
        assert_eq!(o.surface_overlay, Color::Indexed(2), "{preset:?}");
        assert_eq!(o.status_needs_pr, Color::Indexed(3), "{preset:?}");
        assert_eq!(o.status_conflict, Color::Indexed(4), "{preset:?}");
    }
}

#[test]
fn theme_v1_overrides_parse_from_toml() {
    let cfg: ThemeConfig = toml::from_str(
        "[overrides]\nsurface_bar = 236\nsurface_overlay = \"#1a1a1a\"\n\
         status_needs_pr = \"blue\"\nstatus_conflict = \"lightmagenta\"\n",
    )
    .expect("v1 override keys must parse");
    let t = Theme::from_config(&cfg);
    assert_eq!(t.surface_bar, Color::Indexed(236));
    assert_eq!(t.surface_overlay, Color::Rgb(0x1a, 0x1a, 0x1a));
    assert_eq!(t.status_needs_pr, Color::Blue);
    assert_eq!(t.status_conflict, Color::LightMagenta);
}

#[test]
fn border_focused_still_deserializes() {
    let cfg: ThemeConfig =
        toml::from_str("preset = \"dark\"\n[overrides]\nborder_focused = \"white\"\n")
            .expect("a config with border_focused must still load");
    assert!(cfg.overrides.border_focused.is_some());
    // Read and ignored: the theme is the same as without the key.
    assert_eq!(Theme::from_config(&cfg), Theme::dark());
}

#[test]
fn border_focused_is_read_only_by_the_theme_module() {
    fn walk(dir: &std::path::Path, skip: &[std::path::PathBuf], hits: &mut Vec<String>) {
        for entry in std::fs::read_dir(dir).expect("read tui dir") {
            let path = entry.expect("dir entry").path();
            if path.is_dir() {
                walk(&path, skip, hits);
            } else if path.extension().is_some_and(|e| e == "rs") && !skip.contains(&path) {
                let text = std::fs::read_to_string(&path).expect("read source");
                if text.contains("border_focused") {
                    hits.push(path.display().to_string());
                }
            }
        }
    }
    let tui = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src/tui");
    // The theme module itself: `theme.rs` and this sibling test file.
    let skip = [tui.join("theme.rs"), tui.join("theme_v1_tests.rs")];
    let mut hits = Vec::new();
    walk(&tui, &skip, &mut hits);
    assert!(
        hits.is_empty(),
        "border_focused is retired; still read in {hits:?}"
    );
}

#[test]
fn focused_block_uses_border_active() {
    for theme in [
        Theme::dark(),
        Theme::light(),
        Theme::retro(),
        Theme::high_contrast(),
    ] {
        let block = theme.styled_block_plain(true);
        let mut buf = ratatui::buffer::Buffer::empty(ratatui::layout::Rect::new(0, 0, 3, 3));
        ratatui::widgets::Widget::render(block, buf.area, &mut buf);
        assert_eq!(buf[(0, 0)].fg, theme.border_active);
    }
}

#[test]
fn no_color_preset_is_all_reset() {
    let t = Theme::no_color();
    let values = colour_values(&t);
    assert!(values.len() > 40, "Debug parse found only {values:?}");
    for v in &values {
        assert_eq!(v, "Reset", "NoColor must carry no colour: {values:?}");
    }
    assert_eq!(preset_theme(ThemePreset::NoColor), t);
}

#[test]
fn high_contrast_preset_has_no_reset_and_light_muted_text() {
    let t = Theme::high_contrast();
    assert!(colour_values(&t).iter().all(|v| v != "Reset"));
    assert_eq!(t.text_primary, Color::Rgb(255, 255, 255));
    assert_eq!(t.text_muted, Color::Rgb(0xc4, 0xc4, 0xc4));
    assert_eq!(preset_theme(ThemePreset::HighContrast), t);
}

#[test]
fn new_presets_parse_with_kebab_case_names() {
    let hc: ThemeConfig = toml::from_str("preset = \"high-contrast\"").expect("high-contrast");
    assert_eq!(hc.preset, ThemePreset::HighContrast);
    let nc: ThemeConfig = toml::from_str("preset = \"no-color\"").expect("no-color");
    assert_eq!(nc.preset, ThemePreset::NoColor);
}

#[test]
fn no_color_env_selects_no_color_preset() {
    let cfg = ThemeConfig::default();
    let env = |pairs: &'static [(&'static str, &'static str)]| {
        move |k: &str| {
            pairs
                .iter()
                .find(|(key, _)| *key == k)
                .map(|(_, v)| v.to_string())
        }
    };

    assert_eq!(
        Theme::detect_from_env(&cfg, env(&[("NO_COLOR", "1")])),
        Theme::no_color()
    );
    assert_eq!(
        Theme::detect_from_env(&cfg, env(&[("TERM", "dumb")])),
        Theme::no_color()
    );
    // NO_COLOR wins over a configured preset and its overrides.
    let light = ThemeConfig {
        preset: ThemePreset::Light,
        overrides: ThemeOverrides {
            text_primary: Some(SerializableColor(ThemeColor::Red)),
            ..Default::default()
        },
    };
    assert_eq!(
        Theme::detect_from_env(&light, env(&[("NO_COLOR", "yes")])),
        Theme::no_color()
    );
    // An empty NO_COLOR does not count (no-color.org), and a normal TERM keeps the preset.
    let mut expected = Theme::dark();
    expected.apply_capability(ColorCapability::Extended);
    assert_eq!(
        Theme::detect_from_env(&cfg, env(&[("NO_COLOR", ""), ("TERM", "xterm-256color")])),
        expected
    );
}
