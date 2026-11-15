//! Snapshot the schema-driven settings renderer at 80×24 with a synthetic
//! TableSchema containing every `FieldKind`. Locks the visual contract for
//! all six leaf kinds and the recursive `NestedTable` flattening.

use insta::assert_snapshot;
use ratatui::layout::Rect;
use ratatui::{Terminal, backend::TestBackend};

use crate::tui::screens::settings::schema_tab::build::from_schema;
use crate::tui::screens::settings::schema_tab::test_fixture::{SYNTH_TABLE, default_config};
use crate::tui::theme::Theme;

#[test]
fn schema_tab_synth_table_renders_80x24() {
    let fields = from_schema(&SYNTH_TABLE, &default_config());
    let mut terminal = Terminal::new(TestBackend::new(80, 24)).unwrap();
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
                let focused = i == 0;
                field.widget.draw(f, row, &theme, focused, None);
            }
        })
        .unwrap();
    assert_snapshot!(terminal.backend());
}
