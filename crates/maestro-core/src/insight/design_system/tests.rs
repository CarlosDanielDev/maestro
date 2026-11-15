use super::*;

// --- palette_from_source ----------------------------------------------

#[test]
fn palette_named_color_produces_color_name() {
    let src = r#"
impl Theme {
    pub fn dark() -> Self {
        Self {
            branding_bg: Color::Green,
        }
    }
}
"#;
    let palette = palette_from_source(src);
    assert!(
        palette.contains(&NamedValue {
            name: "branding_bg".into(),
            value: "Green".into(),
        }),
        "expected branding_bg=Green, got {palette:?}"
    );
}

#[test]
fn palette_rgb_color_produces_rgb_string() {
    let src = r#"
impl Theme {
    pub fn dark() -> Self {
        Self {
            accent_foo: Color::Rgb(0, 255, 65),
        }
    }
}
"#;
    let palette = palette_from_source(src);
    assert!(
        palette.contains(&NamedValue {
            name: "accent_foo".into(),
            value: "rgb(0,255,65)".into(),
        }),
        "expected accent_foo=rgb(0,255,65), got {palette:?}"
    );
}

#[test]
fn palette_only_reads_dark_constructor() {
    let src = r#"
impl Theme {
    pub fn dark() -> Self {
        Self { bg: Color::Green }
    }
    pub fn light() -> Self {
        Self { bg: Color::Blue }
    }
}
"#;
    let palette = palette_from_source(src);
    assert_eq!(
        palette.len(),
        1,
        "only dark() fields expected, got {palette:?}"
    );
    assert_eq!(palette[0].value, "Green");
}

#[test]
fn palette_empty_source_returns_empty() {
    assert!(
        palette_from_source("").is_empty(),
        "empty source must return empty Vec"
    );
}

#[test]
fn palette_unrecognized_color_expr_does_not_panic() {
    // A call expression that is not Color::Rgb — must not panic, and the
    // recognizable field must still be extracted (totality + partial
    // extraction).
    let src = r#"
impl Theme {
    pub fn dark() -> Self {
        Self {
            bg: Color::from_u32(0),
            known: Color::Red,
        }
    }
}
"#;
    let palette = palette_from_source(src);
    assert!(
        palette
            .iter()
            .any(|nv| nv.name == "known" && nv.value == "Red"),
        "known field must still be present, got {palette:?}"
    );
}

// --- icons_from_source -------------------------------------------------

#[test]
fn icons_single_arm_extracts_nerd_glyph() {
    let src = r#"
const fn icon_pair(id: IconId) -> IconPair {
    match id {
        IconId::ChevronRight => IconPair::new("\u{f054}", ">"),
    }
}
"#;
    let icons = icons_from_source(src);
    assert_eq!(icons.len(), 1);
    assert_eq!(icons[0].name, "ChevronRight");
    assert_eq!(icons[0].value, "\u{f054}");
}

#[test]
fn icons_multiple_arms_each_produce_named_value() {
    let src = r#"
const fn icon_pair(id: IconId) -> IconPair {
    match id {
        IconId::ChevronRight => IconPair::new("\u{f054}", ">"),
        IconId::Play => IconPair::new("\u{f40a}", "[>]"),
    }
}
"#;
    let icons = icons_from_source(src);
    assert_eq!(icons.len(), 2, "expected 2 icons, got {icons:?}");
    let play = icons
        .iter()
        .find(|nv| nv.name == "Play")
        .expect("Play icon");
    assert_eq!(play.value, "\u{f40a}");
}

#[test]
fn icons_empty_source_returns_empty() {
    assert!(
        icons_from_source("").is_empty(),
        "empty source must return empty Vec"
    );
}

#[test]
fn icons_non_icon_pair_call_ignored() {
    // An arm whose body is not an IconPair::new(..) call must be skipped
    // without panic; the valid arm is still extracted.
    let src = r#"
const fn icon_pair(id: IconId) -> IconPair {
    match id {
        IconId::ChevronRight => IconPair::new("\u{f054}", ">"),
        IconId::Foo => todo!(),
    }
}
"#;
    let icons = icons_from_source(src);
    assert!(
        icons.iter().any(|nv| nv.name == "ChevronRight"),
        "valid arm must be extracted, got {icons:?}"
    );
    assert!(
        !icons.iter().any(|nv| nv.name == "Foo"),
        "Foo arm with no IconPair::new must be skipped"
    );
}

// --- mascot_summary ----------------------------------------------------

#[test]
fn mascot_summary_counts_static_frames_tables() {
    let src = r#"
static IDLE_FRAMES: [[&str; 2]; 6] = [];
static CONDUCTING_FRAMES: [[&str; 2]; 6] = [];
"#;
    let mascot = mascot_summary(src);
    assert!(
        mascot.is_some(),
        "expected Some(Mascot) when frames tables exist"
    );
    assert_eq!(mascot.unwrap().frame_count, 2);
}

#[test]
fn mascot_summary_name_and_description_are_fixed() {
    let src = "static IDLE_FRAMES: [[&str; 2]; 6] = [];";
    let mascot = mascot_summary(src).expect("expected Some(Mascot)");
    assert_eq!(mascot.name, "Clawd");
    assert!(
        !mascot.description.is_empty(),
        "description must be non-empty"
    );
}

#[test]
fn mascot_summary_empty_source_returns_none() {
    assert!(
        mascot_summary("").is_none(),
        "empty source must return None"
    );
}

#[test]
fn mascot_summary_no_frames_tables_returns_none() {
    let src = r#"
static FOO: u32 = 0;
pub fn hello() {}
"#;
    assert!(
        mascot_summary(src).is_none(),
        "no *_FRAMES statics must return None"
    );
}

// --- collect -----------------------------------------------------------

#[test]
fn collect_wires_palette_icons_and_mascot() {
    let theme_src = r#"
impl Theme {
    pub fn dark() -> Self {
        Self { bg: Color::Green }
    }
}
"#;
    let icons_src = r#"
const fn icon_pair(id: IconId) -> IconPair {
    match id {
        IconId::ChevronRight => IconPair::new("\u{f054}", ">"),
    }
}
"#;
    let frames_src = "static IDLE_FRAMES: [[&str; 2]; 6] = [];";

    let ds = collect(theme_src, icons_src, frames_src);

    assert!(!ds.palette.is_empty(), "palette must be populated");
    assert!(!ds.icons.is_empty(), "icons must be populated");
    assert!(ds.mascot.is_some(), "mascot must be Some");
    assert!(ds.styles.is_empty(), "styles must stay empty");
    assert!(
        ds.layout_conventions.is_empty(),
        "layout_conventions must stay empty"
    );
}

#[test]
fn collect_all_empty_inputs_produces_empty_design_system() {
    let ds = collect("", "", "");
    assert!(ds.palette.is_empty());
    assert!(ds.icons.is_empty());
    assert!(ds.mascot.is_none());
    assert!(ds.styles.is_empty());
    assert!(ds.layout_conventions.is_empty());
}
