//! Help rows built from [`super::registry`] (#1398), so Help names every
//! bound key and cannot drift from dispatch. One row per (scope, action,
//! label); its aliases share the row (`? / F1`, `⌃Q / ⌃X / ⌃C / q`).
//! Hidden bindings are listed too: Help shows every key, the footer does not.

use std::sync::LazyLock;

use super::{Action, Chord, Scope, TURBOQUANT_TOGGLE, registry};
use crate::tui::app::TuiMode;
use crate::tui::navigation::keymap::{KeyBinding, KeyBindingGroup};

struct Row {
    scope: Scope,
    action: Action,
    label: &'static str,
    key: String,
    description: String,
}

/// Help text the registry label does not hold. Kept short and true.
fn detail(action: Action) -> Option<String> {
    match action {
        Action::Quit => Some("asks before it stops live agents or a team run".into()),
        Action::TurboquantDashboard => Some(format!("dashboard; on/off in {TURBOQUANT_TOGGLE}")),
        _ => None,
    }
}

fn rows() -> &'static [Row] {
    static ROWS: LazyLock<Vec<Row>> = LazyLock::new(|| {
        let mut rows: Vec<Row> = Vec::new();
        for b in registry() {
            let same =
                |r: &&mut Row| r.scope == b.scope && r.action == b.action && r.label == b.label;
            if let Some(row) = rows.iter_mut().find(same) {
                row.key.push_str(" / ");
                row.key.push_str(b.chord.label());
                continue;
            }
            rows.push(Row {
                scope: b.scope,
                action: b.action,
                label: b.label,
                key: b.chord.label().to_string(),
                description: match detail(b.action) {
                    Some(d) => format!("{} — {d}", b.label),
                    None => b.label.to_string(),
                },
            });
        }
        rows
    });
    &ROWS
}

fn group(title: &'static str, keep: impl Fn(Scope) -> bool) -> KeyBindingGroup {
    let bindings = rows()
        .iter()
        .filter(|r| keep(r.scope))
        .map(|r| KeyBinding {
            key: &r.key,
            description: &r.description,
        })
        .collect();
    KeyBindingGroup { title, bindings }
}

/// The Global scope as one Help group.
pub fn global_help_group() -> KeyBindingGroup {
    group("Global keys", |s| s == Scope::Global)
}

/// The registry bindings of `mode`'s screen (and its panels), if any.
pub fn screen_help_group(mode: TuiMode) -> Option<KeyBindingGroup> {
    let g = group("This screen", |s| {
        s != Scope::Global && Scope::Screen(mode).encloses(s)
    });
    (!g.bindings.is_empty()).then_some(g)
}

/// True when `chord` is a Global binding, so a screen's own key list can
/// leave it to the Global group (`Q`, `M`, `u` on Dashboard).
pub fn is_global(chord: Chord) -> bool {
    registry()
        .iter()
        .any(|b| b.scope == Scope::Global && b.chord == chord)
}
