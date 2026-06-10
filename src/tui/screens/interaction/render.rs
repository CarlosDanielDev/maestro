//! Frame rendering for the Interaction screen — split from `mod.rs` to keep
//! it under the 400-line guardrail. Pure draw code; state lives in `mod.rs`.

use ratatui::{
    Frame,
    layout::{Constraint, Layout, Rect},
};

use super::{HEADER_HEIGHT, INPUT_HEIGHT, InteractionScreen, effective_offset, inset_x};
use crate::session::interaction::InteractionState;
use crate::tui::screens::interaction::{history, input};
use crate::tui::theme::Theme;

impl InteractionScreen {
    pub(super) fn draw_impl(&mut self, f: &mut Frame, area: Rect, theme: &Theme) {
        let chunks = Layout::vertical([
            Constraint::Length(HEADER_HEIGHT),
            Constraint::Min(0),
            Constraint::Length(1),
            Constraint::Length(INPUT_HEIGHT),
        ])
        .split(area);
        let header_area = chunks[0];
        // Inset the transcript by one column each side so the rounded card
        // borders get a gutter and the right border never clips against the
        // terminal edge (#987 QA).
        let history_area = inset_x(chunks[1], 1);
        let keybar_area = chunks[2];
        let input_area = chunks[3];

        input::draw_header(
            f,
            header_area,
            theme,
            &self.agent_label,
            &self.model,
            self.issue_number,
            &self.issue_title,
        );

        let total = history::visual_total(&self.history, theme, history_area.width);
        let viewport = history_area.height as usize;
        self.last_max_offset = total.saturating_sub(viewport);
        self.last_viewport = viewport;
        let offset = effective_offset(self.auto_scroll, self.scroll_offset, total, viewport);
        if self.auto_scroll {
            self.scroll_offset = offset;
        }

        history::draw_history(
            f,
            history_area,
            theme,
            &self.history,
            offset,
            self.issue_number,
            &self.issue_title,
        );
        input::draw_keybar(f, keybar_area, theme, self.pushup_enabled());
        if self.state == InteractionState::Terminated {
            input::draw_terminated_banner(f, input_area, theme, self.close_reason.as_ref());
        } else if let Some(pr_number) = self.teardown_pr_in_flight {
            // Async teardown in flight (#941): the UI stays responsive while
            // git runs off-thread — show what's happening instead of an
            // editable input pane.
            input::draw_teardown_banner(f, input_area, theme, pr_number, self.spinner_tick);
        } else {
            let nerd = crate::icon_mode::use_nerd_font();
            // Slow the throbber to ~7fps (the loop redraws at ~20fps): a calmer
            // rotation reads as a clear spinner instead of a vibrating blob.
            let calm = self.spinner_tick / 3;
            let spinner = crate::tui::spinner::graph_node_frame(calm, nerd);
            let wave = crate::tui::spinner::responding_wave(calm, nerd);
            input::draw_input(
                f,
                input_area,
                theme,
                &self.editor,
                self.is_streaming(),
                spinner,
                &wave,
            );
        }

        if self.quit_modal_open {
            input::draw_quit_modal(f, area, theme, &self.worktree_path);
        }

        if let Some(review) = self.diff_review.as_mut() {
            review.draw(f, area, theme);
        }
    }
}
