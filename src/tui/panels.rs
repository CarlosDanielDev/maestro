use crate::session::types::Session;
use crate::state::file_claims::FileClaimManager;
use crate::tui::markdown::render_markdown;
use crate::tui::spinner;
use crate::tui::theme::Theme;
use ratatui::{
    Frame,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Gauge, Paragraph, Wrap},
};

/// Minimum dimensions for a single session cell.
const MIN_CELL_WIDTH: u16 = 40;
const MIN_CELL_HEIGHT: u16 = 8;

/// Grid layout calculation result.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct GridLayout {
    pub cols: usize,
    pub rows: usize,
    pub total_pages: usize,
    pub sessions_per_page: usize,
}

impl GridLayout {
    /// Calculate optimal grid layout from session count and available area.
    pub fn calculate(session_count: usize, area_width: u16, area_height: u16) -> Self {
        if session_count == 0 {
            return Self {
                cols: 1,
                rows: 1,
                total_pages: 1,
                sessions_per_page: 0,
            };
        }

        let max_cols = (area_width / MIN_CELL_WIDTH).max(1) as usize;
        let max_rows = (area_height / MIN_CELL_HEIGHT).max(1) as usize;

        let (cols, rows) = match session_count {
            1 => (1, 1),
            2..=3 => (session_count.min(max_cols), 1),
            4 => {
                if max_cols >= 2 && max_rows >= 2 {
                    (2, 2)
                } else {
                    (session_count.min(max_cols), 1)
                }
            }
            5..=6 => {
                if max_cols >= 3 && max_rows >= 2 {
                    (3, 2)
                } else if max_cols >= 2 && max_rows >= 3 {
                    (2, 3)
                } else {
                    (session_count.min(max_cols), 1)
                }
            }
            7..=9 => {
                if max_cols >= 3 && max_rows >= 3 {
                    (3, 3)
                } else if max_cols >= 3 && max_rows >= 2 {
                    (3, 2)
                } else {
                    (session_count.min(max_cols), 1)
                }
            }
            _ => {
                let cols = max_cols.min(3);
                let rows = max_rows.min(3);
                (cols, rows)
            }
        };

        let sessions_per_page = cols * rows;
        let total_pages = if sessions_per_page == 0 {
            1
        } else {
            session_count.div_ceil(sessions_per_page)
        };

        Self {
            cols,
            rows,
            total_pages,
            sessions_per_page,
        }
    }
}

/// Grid navigation state.
#[derive(Debug, Clone)]
pub struct GridState {
    pub current_page: usize,
    pub selected_col: usize,
    pub selected_row: usize,
}

impl GridState {
    pub fn new() -> Self {
        Self {
            current_page: 0,
            selected_col: 0,
            selected_row: 0,
        }
    }

    /// Flat index into the session list.
    pub fn selected_index(&self, layout: &GridLayout) -> usize {
        self.current_page * layout.sessions_per_page
            + self.selected_row * layout.cols
            + self.selected_col
    }

    pub fn move_left(&mut self) {
        self.selected_col = self.selected_col.saturating_sub(1);
    }

    pub fn move_right(&mut self, layout: &GridLayout) {
        if self.selected_col + 1 < layout.cols {
            self.selected_col += 1;
        }
    }

    pub fn move_up(&mut self) {
        self.selected_row = self.selected_row.saturating_sub(1);
    }

    pub fn move_down(&mut self, layout: &GridLayout) {
        if self.selected_row + 1 < layout.rows {
            self.selected_row += 1;
        }
    }

    pub fn next_page(&mut self, layout: &GridLayout) {
        if self.current_page + 1 < layout.total_pages {
            self.current_page += 1;
            self.selected_col = 0;
            self.selected_row = 0;
        }
    }

    pub fn prev_page(&mut self, _layout: &GridLayout) {
        if self.current_page > 0 {
            self.current_page -= 1;
            self.selected_col = 0;
            self.selected_row = 0;
        }
    }

    /// Clamp selection to valid bounds after layout change or session removal.
    #[allow(dead_code)] // Reason: public API for grid state management after session changes
    pub fn clamp(&mut self, layout: &GridLayout, total_sessions: usize) {
        if layout.total_pages > 0 && self.current_page >= layout.total_pages {
            self.current_page = layout.total_pages - 1;
        }
        if self.selected_col >= layout.cols {
            self.selected_col = layout.cols.saturating_sub(1);
        }
        if self.selected_row >= layout.rows {
            self.selected_row = layout.rows.saturating_sub(1);
        }
        let idx = self.selected_index(layout);
        if idx >= total_sessions && total_sessions > 0 {
            let page_start = self.current_page * layout.sessions_per_page;
            let on_page = total_sessions.saturating_sub(page_start);
            if on_page > 0 {
                let last = on_page - 1;
                self.selected_row = last / layout.cols;
                self.selected_col = last % layout.cols;
            }
        }
    }
}

pub struct PanelView {
    pub selected: Option<usize>,
    /// Scroll offset for the message area in agent panels.
    pub scroll_offset: u16,
    pub grid_state: GridState,
}

impl PanelView {
    pub fn new() -> Self {
        Self {
            selected: None,
            scroll_offset: 0,
            grid_state: GridState::new(),
        }
    }

    pub fn scroll_up(&mut self) {
        self.scroll_offset = self.scroll_offset.saturating_sub(1);
    }

    pub fn scroll_down(&mut self) {
        self.scroll_offset = self.scroll_offset.saturating_add(1);
    }

    pub fn selected_index(&self) -> usize {
        self.selected.unwrap_or(0)
    }

    pub fn draw(
        &self,
        f: &mut Frame,
        sessions: &[&Session],
        area: Rect,
        theme: &Theme,
        spinner_tick: usize,
    ) {
        self.draw_with_claims(f, sessions, None, area, theme, spinner_tick);
    }

    pub fn draw_with_claims(
        &self,
        f: &mut Frame,
        sessions: &[&Session],
        file_claims: Option<&FileClaimManager>,
        area: Rect,
        theme: &Theme,
        spinner_tick: usize,
    ) {
        if sessions.is_empty() {
            let block = Block::default()
                .borders(Borders::ALL)
                .border_style(Style::default().fg(theme.border_inactive))
                .title(" No sessions ");
            let msg = Paragraph::new("Waiting for sessions to start…")
                .style(Style::default().fg(theme.text_secondary))
                .block(block)
                .wrap(Wrap { trim: true });
            f.render_widget(msg, area);
            return;
        }

        let layout = GridLayout::calculate(sessions.len(), area.width, area.height);

        let page_start = self.grid_state.current_page * layout.sessions_per_page;
        let page_end = sessions.len().min(page_start + layout.sessions_per_page);
        let page_sessions = &sessions[page_start..page_end];

        // Reserve space for page indicator if paginated
        let (grid_area, indicator_area) = if layout.total_pages > 1 {
            let chunks = Layout::default()
                .direction(Direction::Vertical)
                .constraints([Constraint::Min(1), Constraint::Length(1)])
                .split(area);
            (chunks[0], Some(chunks[1]))
        } else {
            (area, None)
        };

        // Split into rows
        let row_constraints: Vec<Constraint> = (0..layout.rows)
            .map(|_| Constraint::Ratio(1, layout.rows as u32))
            .collect();
        let rows = Layout::default()
            .direction(Direction::Vertical)
            .constraints(row_constraints)
            .split(grid_area);

        for row_idx in 0..layout.rows {
            let col_constraints: Vec<Constraint> = (0..layout.cols)
                .map(|_| Constraint::Ratio(1, layout.cols as u32))
                .collect();
            let cols = Layout::default()
                .direction(Direction::Horizontal)
                .constraints(col_constraints)
                .split(rows[row_idx]);

            for col_idx in 0..layout.cols {
                let session_idx = row_idx * layout.cols + col_idx;
                if session_idx < page_sessions.len() {
                    let is_selected = self.grid_state.selected_row == row_idx
                        && self.grid_state.selected_col == col_idx;
                    let has_conflict = file_claims
                        .map(|fc| fc.has_active_conflict(page_sessions[session_idx].id))
                        .unwrap_or(false);
                    draw_single_panel(
                        f,
                        page_sessions[session_idx],
                        cols[col_idx],
                        is_selected,
                        has_conflict,
                        if is_selected { self.scroll_offset } else { 0 },
                        theme,
                        spinner_tick,
                    );
                }
            }
        }

        // Page indicator
        if let Some(ind_area) = indicator_area {
            let text = format!(
                " Page {}/{} — [/] prev/next ",
                self.grid_state.current_page + 1,
                layout.total_pages
            );
            f.render_widget(
                Paragraph::new(text).style(Style::default().fg(theme.text_muted)),
                ind_area,
            );
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn draw_single_panel(
    f: &mut Frame,
    session: &Session,
    area: Rect,
    is_selected: bool,
    has_conflict: bool,
    scroll: u16,
    theme: &Theme,
    spinner_tick: usize,
) {
    let status_color = theme.status_color(session.status);

    let fork_indicator = if session.parent_session_id.is_some() {
        format!(" [fork:{}]", session.fork_depth)
    } else {
        String::new()
    };

    let conflict_indicator = if has_conflict { " CONFLICT" } else { "" };

    let title = match (session.issue_number, &session.issue_title) {
        (Some(n), Some(t)) => {
            let max_title_len = 30;
            let short_title: String = if t.chars().count() > max_title_len {
                let truncated: String = t.chars().take(max_title_len - 1).collect();
                format!("{}…", truncated)
            } else {
                t.clone()
            };
            format!(
                " #{} — {}{}{} ",
                n, short_title, fork_indicator, conflict_indicator
            )
        }
        (Some(n), None) => format!(" #{}{}{} ", n, fork_indicator, conflict_indicator),
        _ => format!(
            " {}{}{} ",
            &session.id.to_string()[..8],
            fork_indicator,
            conflict_indicator
        ),
    };

    let border_style = if has_conflict {
        Style::default()
            .fg(theme.accent_error)
            .add_modifier(Modifier::BOLD | Modifier::SLOW_BLINK)
    } else if is_selected {
        Style::default()
            .fg(theme.border_focused)
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(status_color)
    };

    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(border_style)
        .title(title);

    let inner = block.inner(area);
    f.render_widget(block, area);

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1), // status + elapsed
            Constraint::Length(1), // cost + files
            Constraint::Length(2), // context gauge
            Constraint::Length(1), // current activity
            Constraint::Min(1),    // last message (scrollable)
        ])
        .split(inner);

    // Status line
    let mut status_spans = vec![Span::styled(
        format!("{} {} ", session.status.symbol(), session.status.label()),
        Style::default()
            .fg(status_color)
            .add_modifier(Modifier::BOLD),
    )];
    if session.is_hollow_completion {
        status_spans.push(Span::styled(
            "HOLLOW ",
            Style::default()
                .fg(theme.accent_warning)
                .add_modifier(Modifier::BOLD),
        ));
    }
    status_spans.push(Span::styled(
        session.elapsed_display(),
        Style::default().fg(theme.text_primary),
    ));
    let status_line = Line::from(status_spans);
    f.render_widget(Paragraph::new(status_line), chunks[0]);

    // Cost + file count
    let cost_line = Line::from(vec![
        Span::styled(
            format!("${:.2}", session.cost_usd),
            Style::default().fg(theme.accent_warning),
        ),
        Span::raw("  "),
        Span::styled(
            format!("{} files", session.files_touched.len()),
            Style::default().fg(theme.text_secondary),
        ),
    ]);
    f.render_widget(Paragraph::new(cost_line), chunks[1]);

    // Context gauge
    let ctx_pct = (session.context_pct * 100.0).min(100.0);
    let gauge_color = theme.gauge_color(ctx_pct);
    let gauge_label = if ctx_pct > 70.0 {
        format!("ctx: {:.0}% OVERFLOW", ctx_pct)
    } else {
        format!("ctx: {:.0}%", ctx_pct)
    };
    let gauge = Gauge::default()
        .gauge_style(Style::default().fg(gauge_color))
        .label(gauge_label)
        .percent(ctx_pct as u16);
    f.render_widget(gauge, chunks[2]);

    // Current activity (phase-aware animation)
    let activity_text = if session.is_hollow_completion {
        "> \u{26A0} Session completed without performing any work".to_string()
    } else {
        let phase = spinner::animation_phase(
            session.status,
            session.is_thinking,
            &session.current_activity,
        );
        let thinking_elapsed = session.thinking_started_at.map(|t| t.elapsed());
        format!(
            "> {}",
            spinner::animated_activity(
                phase,
                spinner_tick,
                &session.current_activity,
                thinking_elapsed
            )
        )
    };
    let activity = Line::from(Span::styled(
        activity_text,
        Style::default().fg(theme.accent_info),
    ));
    f.render_widget(Paragraph::new(activity), chunks[3]);

    // Last message (scrollable, rendered as markdown)
    let md_text = if session.last_message.is_empty() {
        ratatui::text::Text::raw("Waiting for output...")
    } else {
        render_markdown(
            &session.last_message,
            theme,
            chunks[4].width.saturating_sub(2),
        )
    };
    let msg = Paragraph::new(md_text)
        .wrap(Wrap { trim: true })
        .scroll((scroll, 0));
    f.render_widget(msg, chunks[4]);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn grid_layout_zero_sessions() {
        let l = GridLayout::calculate(0, 120, 40);
        assert_eq!(l.cols, 1);
        assert_eq!(l.rows, 1);
        assert_eq!(l.total_pages, 1);
        assert_eq!(l.sessions_per_page, 0);
    }

    #[test]
    fn grid_layout_two_sessions() {
        let l = GridLayout::calculate(2, 120, 40);
        assert_eq!(l.cols, 2);
        assert_eq!(l.rows, 1);
    }

    #[test]
    fn grid_layout_four_sessions_wide_terminal() {
        let l = GridLayout::calculate(4, 120, 40);
        assert_eq!(l.cols, 2);
        assert_eq!(l.rows, 2);
    }

    #[test]
    fn grid_layout_four_sessions_narrow_terminal() {
        // 60px wide / 40 min = 1 col max
        let l = GridLayout::calculate(4, 60, 40);
        assert_eq!(l.cols, 1);
    }

    #[test]
    fn grid_layout_ten_sessions_paginated() {
        let l = GridLayout::calculate(10, 120, 40);
        assert_eq!(l.cols, 3);
        assert_eq!(l.rows, 3);
        assert_eq!(l.sessions_per_page, 9);
        assert_eq!(l.total_pages, 2);
    }

    #[test]
    fn grid_state_move_left_at_zero_stays() {
        let mut s = GridState::new();
        s.move_left();
        assert_eq!(s.selected_col, 0);
    }

    #[test]
    fn grid_state_move_right_at_boundary_stays() {
        let layout = GridLayout {
            cols: 3,
            rows: 2,
            total_pages: 1,
            sessions_per_page: 6,
        };
        let mut s = GridState::new();
        s.selected_col = 2;
        s.move_right(&layout);
        assert_eq!(s.selected_col, 2);
    }

    #[test]
    fn grid_state_selected_index_calculates_correctly() {
        let layout = GridLayout {
            cols: 3,
            rows: 3,
            total_pages: 2,
            sessions_per_page: 9,
        };
        let mut s = GridState::new();
        s.selected_row = 1;
        s.selected_col = 2;
        assert_eq!(s.selected_index(&layout), 5); // row 1 * 3 cols + col 2

        s.current_page = 1;
        s.selected_row = 0;
        s.selected_col = 0;
        assert_eq!(s.selected_index(&layout), 9); // page 1 * 9 + 0
    }

    #[test]
    fn grid_state_next_page_prev_page() {
        let layout = GridLayout {
            cols: 3,
            rows: 3,
            total_pages: 3,
            sessions_per_page: 9,
        };
        let mut s = GridState::new();
        s.next_page(&layout);
        assert_eq!(s.current_page, 1);
        s.next_page(&layout);
        assert_eq!(s.current_page, 2);
        s.next_page(&layout); // at last page, stays
        assert_eq!(s.current_page, 2);
        s.prev_page(&layout);
        assert_eq!(s.current_page, 1);
    }

    #[test]
    fn grid_state_clamp_after_session_removal() {
        let layout = GridLayout {
            cols: 2,
            rows: 2,
            total_pages: 1,
            sessions_per_page: 4,
        };
        let mut s = GridState::new();
        s.selected_row = 1;
        s.selected_col = 1; // index 3
        s.clamp(&layout, 2); // only 2 sessions
        assert!(s.selected_index(&layout) < 2);
    }
}
