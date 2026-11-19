//! `ds::toast` (#1254): the one toast. A rounded overlay at the bottom
//! right on `surface_overlay`, with a severity glyph + word so it reads
//! under NO_COLOR, and an optional `^Z undo` countdown. Follows `toast`
//! in `docs/design-system/index.html` §04 (Components, Kit v2).
//!
//! Lifetimes: success / info 3 s, warn 6 s, error sticky until `Esc`, an
//! undo toast lives for its [`UNDO_WINDOW`]. One toast is visible at a
//! time: a newer success / info / warn replaces the older one, errors
//! queue and show one by one once no other toast is live. At most
//! `MAX_QUEUED_ERRORS` errors wait; a newer one drops the oldest.

use std::collections::VecDeque;
use std::time::{Duration, Instant};

use ratatui::{
    Frame,
    layout::Rect,
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Clear, Paragraph},
};

use crate::tui::ds::panel::{Focus, panel_plain};
use crate::tui::icons::{self, IconId};
use crate::tui::screens::sanitize_for_terminal;
use crate::tui::theme::Theme;

/// How long `Ctrl+Z` can undo after an undo toast appears.
pub const UNDO_WINDOW: Duration = Duration::from_secs(5);
const SHORT_TTL: Duration = Duration::from_secs(3);
const WARN_TTL: Duration = Duration::from_secs(6);
/// Widest toast, borders included (design `w: 72`).
const MAX_WIDTH: u16 = 72;
const HEIGHT: u16 = 3;
/// Cells in the countdown bar: one per second of [`UNDO_WINDOW`].
const BAR_CELLS: u64 = 5;
/// Most error toasts kept waiting; past it the oldest is dropped, so a
/// burst of errors cannot grow the queue without limit (#1382).
const MAX_QUEUED_ERRORS: usize = 5;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Severity {
    Success,
    Info,
    Warn,
    Error,
}

impl Severity {
    /// The word drawn after the glyph.
    pub const fn word(self) -> &'static str {
        match self {
            Self::Success => "ok",
            Self::Info => "info",
            Self::Warn => "warn",
            Self::Error => "error",
        }
    }

    const fn icon(self) -> IconId {
        match self {
            Self::Success => IconId::CheckCircle,
            Self::Info => IconId::DotFill,
            Self::Warn => IconId::Warning,
            Self::Error => IconId::XCircle,
        }
    }

    fn colour(self, theme: &Theme) -> Color {
        match self {
            Self::Success => theme.accent_success,
            Self::Info => theme.accent_info,
            Self::Warn => theme.accent_warning,
            Self::Error => theme.accent_error,
        }
    }

    /// `None` = sticky until dismissed.
    const fn ttl(self) -> Option<Duration> {
        match self {
            Self::Success | Self::Info => Some(SHORT_TTL),
            Self::Warn => Some(WARN_TTL),
            Self::Error => None,
        }
    }
}

/// What a toast draws at one instant.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ToastView<'a> {
    pub severity: Severity,
    pub message: &'a str,
    /// Time left to undo; `Some` only on a live undo toast.
    pub undo_left: Option<Duration>,
}

/// A toast before it is shown. `U` is what `Ctrl+Z` hands back.
pub struct Toast<U> {
    severity: Severity,
    message: String,
    undo: Option<U>,
}

impl<U> Toast<U> {
    pub fn new(severity: Severity, message: impl Into<String>) -> Self {
        Self {
            severity,
            message: message.into(),
            undo: None,
        }
    }

    /// A success toast that `Ctrl+Z` can undo within [`UNDO_WINDOW`].
    pub fn undoable(message: impl Into<String>, undo: U) -> Self {
        Self {
            undo: Some(undo),
            ..Self::new(Severity::Success, message)
        }
    }

    fn ttl(&self) -> Option<Duration> {
        if self.undo.is_some() {
            Some(UNDO_WINDOW)
        } else {
            self.severity.ttl()
        }
    }
}

struct Shown<U> {
    toast: Toast<U>,
    at: Instant,
}

impl<U> Shown<U> {
    fn left(&self, now: Instant) -> Option<Duration> {
        let ttl = self.toast.ttl()?;
        Some(ttl.saturating_sub(now.saturating_duration_since(self.at)))
    }

    fn live(&self, now: Instant) -> bool {
        self.left(now).is_none_or(|left| !left.is_zero())
    }

    fn view(&self, now: Instant) -> ToastView<'_> {
        ToastView {
            severity: self.toast.severity,
            message: &self.toast.message,
            undo_left: self.toast.undo.as_ref().and(self.left(now)),
        }
    }
}

/// The toast queue `App` owns. Every method takes `now`, so tests drive
/// the clock.
pub struct ToastQueue<U> {
    transient: Option<Shown<U>>,
    errors: VecDeque<Shown<U>>,
}

impl<U> Default for ToastQueue<U> {
    fn default() -> Self {
        Self {
            transient: None,
            errors: VecDeque::new(),
        }
    }
}

impl<U> ToastQueue<U> {
    pub fn push(&mut self, toast: Toast<U>, now: Instant) {
        let shown = Shown { toast, at: now };
        if shown.toast.severity == Severity::Error {
            if self.errors.len() >= MAX_QUEUED_ERRORS {
                self.errors.pop_front();
            }
            self.errors.push_back(shown);
        } else {
            self.transient = Some(shown);
        }
    }

    fn live_transient(&self, now: Instant) -> Option<&Shown<U>> {
        self.transient.as_ref().filter(|s| s.live(now))
    }

    pub fn visible(&self, now: Instant) -> Option<ToastView<'_>> {
        self.live_transient(now)
            .or_else(|| self.errors.front())
            .map(|s| s.view(now))
    }

    /// `Ctrl+Z`: the visible toast's undo, once, inside its window.
    pub fn take_undo(&mut self, now: Instant) -> Option<U> {
        self.live_transient(now)?.toast.undo.as_ref()?;
        self.transient.take().and_then(|s| s.toast.undo)
    }

    /// `Esc`: drop the visible error. `false` when no error is visible.
    pub fn dismiss_error(&mut self, now: Instant) -> bool {
        if self.live_transient(now).is_some() {
            return false;
        }
        self.errors.pop_front().is_some()
    }
}

/// Draw `view` at the bottom right of `area`, above its last row (the
/// footer). Reads the global icon mode.
pub fn draw(f: &mut Frame, area: Rect, view: &ToastView<'_>, theme: &Theme) {
    draw_for_mode(f, area, view, theme, icons::use_nerd_font());
}

/// Pure variant of [`draw`]: pass the icon mode explicitly.
pub fn draw_for_mode(
    f: &mut Frame,
    area: Rect,
    view: &ToastView<'_>,
    theme: &Theme,
    nerd_font: bool,
) {
    let colour = view.severity.colour(theme);
    let bg = Style::new().bg(theme.surface_overlay);
    let glyph = icons::get_for_mode(view.severity.icon(), nerd_font);
    let left = Line::from(vec![
        Span::styled(
            format!("{glyph} {}", view.severity.word()),
            bg.fg(colour).add_modifier(Modifier::BOLD),
        ),
        Span::styled("  ", bg),
        Span::styled(
            sanitize_for_terminal(view.message),
            bg.fg(theme.text_primary),
        ),
    ]);
    let right = view
        .undo_left
        .map(|left| countdown(left, colour, bg, theme, nerd_font));
    let right_w = right.as_ref().map_or(0, |r| r.width() as u16 + 2);

    let content_w = u16::try_from(left.width())
        .unwrap_or(u16::MAX)
        .saturating_add(right_w);
    let w = content_w
        .saturating_add(4)
        .min(MAX_WIDTH)
        .min(area.width.saturating_sub(2));
    if w < 12 || area.height < HEIGHT + 1 {
        return;
    }
    let rect = Rect::new(area.right() - w - 1, area.bottom() - HEIGHT - 1, w, HEIGHT);
    let block = panel_plain(Focus::Overlay, theme)
        .border_style(Style::new().fg(colour))
        .style(bg);
    let inner = block.inner(rect);
    f.render_widget(Clear, rect);
    f.render_widget(block, rect);

    let row = Rect::new(inner.x + 1, inner.y, inner.width.saturating_sub(2), 1);
    let right_w = right_w.min(row.width);
    let left_w = row.width - right_w;
    f.render_widget(Paragraph::new(left), Rect::new(row.x, row.y, left_w, 1));
    if let Some(right) = right {
        f.render_widget(
            Paragraph::new(right).right_aligned(),
            Rect::new(row.x + left_w, row.y, right_w, 1),
        );
    }
}

/// `^Z undo ▓▓▓░░ 3s`: one bar cell per second left, rounded up.
fn countdown(
    left: Duration,
    colour: Color,
    bg: Style,
    theme: &Theme,
    nerd_font: bool,
) -> Line<'static> {
    let secs = left.as_millis().div_ceil(1_000).min(u128::from(BAR_CELLS)) as usize;
    let filled = icons::get_for_mode(IconId::GaugeFilled, nerd_font).repeat(secs);
    let empty =
        icons::get_for_mode(IconId::GaugeEmpty, nerd_font).repeat(BAR_CELLS as usize - secs);
    Line::from(vec![
        Span::styled(
            "^Z",
            bg.fg(theme.accent_warning).add_modifier(Modifier::BOLD),
        ),
        Span::styled(" undo ", bg.fg(theme.text_muted)),
        Span::styled(filled, bg.fg(colour)),
        Span::styled(empty, bg.fg(theme.text_muted)),
        Span::styled(format!(" {secs}s"), bg.fg(theme.text_muted)),
    ])
}

#[cfg(test)]
#[path = "toast_tests.rs"]
mod tests;
