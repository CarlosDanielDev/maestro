//! Help rows built from [`super::registry`] (#1398), so Help names every
//! bound key and cannot drift from dispatch. One row per (scope, action,
//! label); its aliases share the row (`? / F1`, `⌃Q / ⌃X / ⌃C / q`).
//! Hidden bindings are listed too: Help shows every key, the footer does not.

use std::sync::LazyLock;

use super::{Action, Chord, Scope, registry};
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
        Action::Quit => Some("asks first while work is live".into()),
        // #1265: where the retired Shift `Q` went, and where TurboQuant is
        // switched (TURBOQUANT_TOGGLE). One row at 80 columns.
        Action::Palette => Some("TurboQuant; on/off in Settings".into()),
        // #1269: `R` only plans; the Run queue's launch dialog spends.
        Action::RunAll => Some("opens the Run queue; launches nothing".into()),
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

fn group(title: &'static str, keep: impl Fn(&Row) -> bool) -> KeyBindingGroup {
    let bindings = rows()
        .iter()
        .filter(|r| keep(r))
        .map(|r| KeyBinding {
            key: &r.key,
            description: &r.description,
        })
        .collect();
    KeyBindingGroup { title, bindings }
}

fn is_tab(r: &Row) -> bool {
    matches!(r.action, Action::SwitchTab(_))
}

/// The Global scope as one Help group, without the digit tab rows: the
/// digits switch tabs only on the seven tab screens ([`tab_help_group`]).
pub fn global_help_group() -> KeyBindingGroup {
    group("Global keys", |r| r.scope == Scope::Global && !is_tab(r))
}

/// The digit rows (`1  Home` … `7  PRD`), for a tab screen only.
pub fn tab_help_group() -> KeyBindingGroup {
    group("Tabs", |r| r.scope == Scope::Global && is_tab(r))
}

/// The registry bindings of `mode`'s screen (and its panels), if any.
pub fn screen_help_group(mode: TuiMode) -> Option<KeyBindingGroup> {
    let g = group("This screen", |r| {
        r.scope != Scope::Global && Scope::Screen(mode).encloses(r.scope)
    });
    (!g.bindings.is_empty()).then_some(g)
}

/// True when `chord` is a Global binding, so a screen's own key list can
/// leave it to the Global group (`u` on Dashboard).
pub fn is_global(chord: Chord) -> bool {
    registry()
        .iter()
        .any(|b| b.scope == Scope::Global && b.chord == chord)
}
