//! Dashboard Deps (`g`, §02.20 of `docs/design-system/index.html`, #1292):
//! two lenses, `issues` (the dependency tree of the run) and `files` (the
//! agents and the repo-relative files they edit, `agent_graph`). `g` on
//! another view opens Deps; `g` on Deps switches the lens. The files lens
//! is off with `views.agent_graph_enabled = false`: `g` then stays on
//! issues and the strip says why.
//!
//! The lens takes every key it gets (nothing falls through to the
//! launcher letters). The selection is always drawn: an agent row (shared
//! with the Work list by session id) or a file row.

use crossterm::event::{KeyCode, KeyEvent};
use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::Style;
use ratatui::text::Span;
use uuid::Uuid;

use super::rows::WorkKey;
use super::usage::View;
use super::{Dashboard, DashboardView};
use crate::tui::agent_graph::ladder::{self, Selection};
use crate::tui::agent_graph::model::FilesGraph;
use crate::tui::app::{App, TuiMode};
use crate::tui::ds::breakpoint::Breakpoint;
use crate::tui::navigation::keymap::InlineHint;
use crate::tui::screens::ScreenAction;
use crate::tui::theme::Theme;
use crate::tui::widgets::focused_selection_style;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum Lens {
    #[default]
    Issues,
    Files,
}

/// The focused column of the files lens.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum Column {
    #[default]
    Agents,
    Files,
}

/// Deps state in `Dashboard::deps`.
#[derive(Debug, Clone, Default)]
pub struct Deps {
    pub lens: Lens,
    pub column: Column,
    /// The selected file row (repo-relative path).
    pub file: Option<String>,
}

impl Deps {
    /// Focus `path` in the files lens (the conflict toast's `⏎`).
    pub fn show_file(&mut self, path: &str) {
        self.lens = Lens::Files;
        self.column = Column::Files;
        self.file = Some(path.to_string());
    }
}

/// The lens drawn: files only while the flag is on.
pub fn lens(dash: &Dashboard, files_enabled: bool) -> Lens {
    if files_enabled {
        dash.deps.lens
    } else {
        Lens::Issues
    }
}

/// `g`: open Deps, or switch the lens when Deps is shown.
pub fn press_g(app: &mut App) {
    let enabled = app.is_agent_graph_enabled();
    let dash = &mut app.screen_state.dashboard;
    if dash.view != View::Deps {
        dash.view = View::Deps;
        dash.list.keep_on_session();
        return;
    }
    dash.deps.lens = match dash.deps.lens {
        Lens::Issues if enabled => Lens::Files,
        _ => Lens::Issues,
    };
}

/// Every key on Deps. Always consumed.
pub(crate) fn handle_key(app: &mut App, key: &KeyEvent, graph: &FilesGraph) -> ScreenAction {
    let files = lens(&app.screen_state.dashboard, app.is_agent_graph_enabled()) == Lens::Files;
    let dash = &mut app.screen_state.dashboard;
    match key.code {
        KeyCode::Char('g') => press_g(app),
        KeyCode::Char('o') | KeyCode::Esc => dash.view = View::Ops,
        KeyCode::Char('c') => super::dispatch::show_usage(app, View::Cost),
        KeyCode::Char('t') => super::dispatch::show_usage(app, View::Tokens),
        _ if !files => {}
        KeyCode::Left => dash.deps.column = Column::Agents,
        KeyCode::Right => {
            dash.deps.column = Column::Files;
            if dash.deps.file.is_none() {
                dash.deps.file = graph.files.first().map(|f| f.path.clone());
            }
        }
        KeyCode::Char('j') | KeyCode::Down => step(dash, graph, true),
        KeyCode::Char('k') | KeyCode::Up => step(dash, graph, false),
        KeyCode::Enter => {
            if let Some(id) = enter_target(dash, graph) {
                app.navigate_to(TuiMode::Detail(id));
            }
        }
        KeyCode::Char('X') if dash.deps.column == Column::Agents => {
            let live = dash.list.selected_session().filter(|id| {
                app.pool
                    .get_session(*id)
                    .is_some_and(|s| !s.status.is_terminal())
            });
            if let Some(id) = live {
                app.open_kill_confirm(id);
            }
        }
        _ => {}
    }
    ScreenAction::None
}

/// Move the selection in the focused column.
fn step(dash: &mut Dashboard, graph: &FilesGraph, down: bool) {
    fn next<T: PartialEq + Clone>(items: &[T], at: Option<&T>, down: bool) -> Option<T> {
        let i = match at.and_then(|a| items.iter().position(|x| x == a)) {
            Some(i) if down => (i + 1).min(items.len().saturating_sub(1)),
            Some(i) => i.saturating_sub(1),
            None => 0,
        };
        items.get(i).cloned()
    }
    match dash.deps.column {
        Column::Agents => {
            let at = dash.list.selected_session();
            if let Some(id) = next(&graph.agents, at.as_ref(), down) {
                dash.list.select(WorkKey::Session(id));
            }
        }
        Column::Files => {
            let paths: Vec<String> = graph.files.iter().map(|f| f.path.clone()).collect();
            dash.deps.file = next(&paths, dash.deps.file.as_ref(), down);
        }
    }
}

/// `⏎`: the selected agent's session, or the session holding the
/// selected file (its first editor when nobody holds the claim).
fn enter_target(dash: &Dashboard, graph: &FilesGraph) -> Option<Uuid> {
    match dash.deps.column {
        Column::Agents => dash.list.selected_session(),
        Column::Files => {
            let path = dash.deps.file.as_deref()?;
            let row = graph.files.iter().find(|f| f.path == path)?;
            row.holder.or_else(|| row.editors.first().copied())
        }
    }
}

/// The body of Deps under the view strip.
pub fn draw(
    f: &mut Frame,
    area: Rect,
    view: &DashboardView<'_>,
    dash: &Dashboard,
    bp: Breakpoint,
    theme: &Theme,
    nerd_font: bool,
) {
    if lens(dash, view.files_enabled) == Lens::Issues {
        return crate::tui::dep_graph::draw_dep_graph(f, view.assigner, area, theme);
    }
    let sel = Selection {
        agent: dash.list.selected_session(),
        file: dash.deps.file.as_deref(),
        files_focused: dash.deps.column == Column::Files,
    };
    ladder::draw(
        f,
        area,
        &view.files,
        &view.sessions,
        &sel,
        bp,
        theme,
        nerd_font,
    );
}

/// `lens  issues · files  g switch` after the view strip, the drawn lens
/// reversed; the files chip reads `⊘ off in [views]` when the flag is off.
pub fn lens_spans(dash: &Dashboard, files_enabled: bool, theme: &Theme) -> Vec<Span<'static>> {
    let muted = Style::new().fg(theme.text_muted);
    let sel = focused_selection_style(theme);
    let active = lens(dash, files_enabled);
    let chip =
        |label: &str, on: bool| Span::styled(format!(" {label} "), if on { sel } else { muted });
    let mut spans = vec![
        Span::styled("   lens ", muted),
        chip("issues", active == Lens::Issues),
        Span::styled("·", muted),
    ];
    if files_enabled {
        spans.push(chip("files", active == Lens::Files));
        spans.push(Span::styled(
            " g",
            crate::tui::ds::bar_row::key_style(theme),
        ));
        spans.push(Span::styled(" switch", muted));
    } else {
        spans.push(Span::styled(" files ⊘ off in [views]", muted));
    }
    spans
}

/// `6 live · 15 files edited · 1 contested`, the right side of the strip.
pub fn summary(view: &DashboardView<'_>) -> String {
    let live = view
        .sessions
        .iter()
        .filter(|s| !s.status.is_terminal())
        .count();
    if !view.files_enabled {
        return format!("{live} live ");
    }
    format!(
        "{live} live · {} files edited · {} contested ",
        view.files.edited(),
        view.files.contested()
    )
}

/// The footer on Deps: only the keys the drawn lens and column take.
pub fn footer(app: &App, hints: &mut Vec<InlineHint>) {
    let dash = &app.screen_state.dashboard;
    let enabled = app.is_agent_graph_enabled();
    let pairs: &[(&'static str, &'static str)] = match (lens(dash, enabled), dash.deps.column) {
        (Lens::Issues, _) if enabled => &[("g", "files lens"), ("Esc", "Ops")],
        (Lens::Issues, _) => &[("Esc", "Ops")],
        (Lens::Files, Column::Agents) => &[
            ("j/k", "move"),
            ("→", "files"),
            ("Enter", "open session"),
            ("X", "kill"),
            ("g", "issues lens"),
            ("Esc", "Ops"),
        ],
        (Lens::Files, Column::Files) => &[
            ("j/k", "move"),
            ("←", "agents"),
            ("Enter", "open owner"),
            ("g", "issues lens"),
            ("Esc", "Ops"),
        ],
    };
    *hints = crate::tui::ds::key_hints::from_pairs(pairs);
}

#[cfg(test)]
#[path = "deps_tests.rs"]
mod tests;
