//! The session screen, `TuiMode::Session { id, view }` (#1288, §02.13
//! "Status: session views" of `docs/design-system/index.html`): the
//! Dashboard detail pane's header, the `d f l v e` view strip, and the view
//! body. Every view reads the session from `id`, never a list position.

mod header;
pub mod output;

pub use header::header;

use ratatui::Frame;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::Style;
use ratatui::text::{Line, Span};
use ratatui::widgets::{Paragraph, Wrap};

use crate::tui::app::{App, SessionView, TuiMode};
use crate::tui::ds::sub_tabs::{SubTab, sub_tabs};
use crate::tui::theme::Theme;

/// Switch to `view` of session `id` in place: the strip is tabs, so the nav
/// stack keeps the screen that opened the session.
pub fn open_view(app: &mut App, id: uuid::Uuid, view: SessionView) {
    match view {
        SessionView::Output => app.screen_state.output_scroll = output::OutputScroll::default(),
        SessionView::Log | SessionView::Gates => app.log_viewer_scroll = 0,
        SessionView::Events => {
            app.call_log_state = crate::tui::call_log::state::CallLogState::default();
        }
        SessionView::Detail => {}
    }
    app.tui_mode = TuiMode::Session { id, view };
}

/// The `d f l v e` strip through `ds::sub_tabs`. Gates carries `✗` when a
/// gate failed; Events its call count.
pub fn strip(
    view: SessionView,
    session: Option<&crate::session::types::Session>,
    theme: &Theme,
) -> Line<'static> {
    let failed = session.is_some_and(|s| s.gate_results.iter().any(|g| !g.passed));
    let count = session.map_or(0, |s| s.call_log.len());
    let events = count.to_string();
    let keys = SessionView::ALL.map(|(_, k, _)| k.to_string());
    let tabs: Vec<SubTab> = SessionView::ALL
        .iter()
        .zip(&keys)
        .map(|((v, _, label), key)| SubTab {
            key,
            label,
            badge: match v {
                SessionView::Gates if failed => Some("✗"),
                SessionView::Events if count > 0 => Some(events.as_str()),
                _ => None,
            },
        })
        .collect();
    let active = SessionView::ALL.iter().position(|(v, _, _)| *v == view);
    sub_tabs(&tabs, active.unwrap_or(usize::MAX), theme)
}

pub fn draw(
    f: &mut Frame,
    app: &mut App,
    id: uuid::Uuid,
    view: SessionView,
    area: Rect,
    theme: &Theme,
    spinner_tick: usize,
) {
    let nerd_font = crate::icon_mode::use_nerd_font();
    let session = app.pool.get_session(id);
    let header = session
        .map(|s| {
            header(
                s,
                chrono::Utc::now(),
                f.area().width,
                area.width,
                theme,
                nerd_font,
            )
        })
        .unwrap_or_default();
    let [head, tabs, body] = Layout::vertical([
        Constraint::Length(header.len() as u16),
        Constraint::Length(1),
        Constraint::Min(1),
    ])
    .areas(area);
    f.render_widget(Paragraph::new(header), head);
    let strip = strip(view, session, theme);
    f.render_widget(Paragraph::new(strip), tabs);

    if view == SessionView::Log {
        crate::tui::log_viewer::draw_log_viewer(
            f,
            &mut app.log_viewer_cache,
            &app.session_logger,
            id,
            app.log_viewer_scroll,
            body,
            theme,
        );
        return;
    }
    let Some(session) = session else {
        let text = " This session is no longer in the pool · Esc back";
        f.render_widget(
            Paragraph::new(Span::styled(text, Style::new().fg(theme.text_muted)))
                .wrap(Wrap { trim: false }),
            body,
        );
        return;
    };
    match view {
        SessionView::Detail => crate::tui::detail::draw_detail_with_claims(
            f,
            session,
            &app.progress_tracker,
            Some(&app.pool.file_claims),
            body,
            theme,
        ),
        SessionView::Output => crate::tui::fullscreen::draw_fullscreen(
            f,
            session,
            &app.progress_tracker,
            body,
            theme,
            spinner_tick,
            &mut app.screen_state.output_scroll,
        ),
        SessionView::Gates => crate::tui::screens::gate_output_viewer::draw_gate_output_viewer(
            f,
            session,
            app.log_viewer_scroll,
            app.completion_summary.is_some(),
            body,
            theme,
        ),
        SessionView::Events => {
            crate::tui::call_log::draw::draw_call_log(f, session, &app.call_log_state, body, theme)
        }
        SessionView::Log => {}
    }
}

#[cfg(test)]
#[path = "tests.rs"]
mod tests;
