//! `ds::status_bar` (#1257): the one state-only row at the top of every
//! screen (`docs/design-system/index.html` §04 StatusBar, Shell rules).
//! No key hints and no actions here: brand, fleet counts, budget, quota and
//! TurboQuant on the left; the `▲ BYPASS` and `UNTRUSTED` safety chips and
//! the update chip pinned right. Drawn on `surface_bar`.
//!
//! When the row is too narrow, whole segments drop (a value is never cut):
//! the update chip first, then TurboQuant, quota, the continuous-run chip,
//! budget and fleet counts.
//! The brand and the two safety chips never drop.

use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;

use crate::session::types::SessionStatus;
use crate::tui::ds::gauge::gauge_level;
use crate::tui::ds::money::money;
use crate::tui::icons::{self, IconId};
use crate::tui::theme::Theme;
use crate::tui::widgets::bypass_indicator::{self, BypassIndicatorState};

/// Sessions grouped the way the bar counts them.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct FleetCounts {
    /// An agent is working: spawning, running, gates, CI or conflict fix, retry.
    pub running: usize,
    /// Waiting on you: review, PR ready, or an interactive session's turn.
    pub waiting: usize,
    /// Gates failed or errored.
    pub failed: usize,
    /// Every session in the pool, finished ones included.
    pub total: usize,
}

impl FleetCounts {
    pub fn from_statuses<'a>(statuses: impl IntoIterator<Item = &'a SessionStatus>) -> Self {
        use SessionStatus::*;
        let mut counts = Self::default();
        for status in statuses {
            counts.total += 1;
            match status {
                Spawning | Running | GatesRunning | CiFix | ConflictFix | Retrying => {
                    counts.running += 1
                }
                NeedsReview | NeedsPr | Interactive => counts.waiting += 1,
                FailedGates | Errored => counts.failed += 1,
                Queued | Completed | Paused | Killed | Stalled => {}
            }
        }
        counts
    }
}

/// Everything the bar shows, as plain data. `ui::draw` builds it from the
/// App; tests build it by hand.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct StatusModel {
    pub version: String,
    pub fleet: FleetCounts,
    pub spent: f64,
    /// Global budget cap; `None` shows `$x spent`.
    pub cap: Option<f64>,
    /// Quota badges (`claude 42% ↻2h30m →`); empty when quota is off.
    pub quota: Vec<String>,
    pub turboquant: bool,
    pub untrusted: bool,
    pub bypass: bool,
    /// The update chip, when a newer version exists.
    pub update: Option<UpdateChip>,
    /// `(done, attempted)` while a continuous run is active.
    pub continuous: Option<(usize, usize)>,
}

/// What the update chip says (#1267).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum UpdateChip {
    /// `↑ <version> u`; the version is already sanitized.
    Available(String),
    /// `↑ queued · after <n> agents`: `u` was pressed while agents ran.
    Queued { after: usize },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Segment {
    Fleet,
    Budget,
    Continuous,
    Quota,
    TurboQuant,
}

/// Left segments in the order they drop, after the update chip.
const DROP_ORDER: [Segment; 5] = [
    Segment::TurboQuant,
    Segment::Quota,
    Segment::Continuous,
    Segment::Budget,
    Segment::Fleet,
];

type Spans = Vec<Span<'static>>;

fn width(spans: &[Span<'_>]) -> usize {
    spans.iter().map(Span::width).sum()
}

/// Width of the chips that never drop (`▲ BYPASS`, `UNTRUSTED`).
pub fn pinned_width(model: &StatusModel, nerd_font: bool) -> u16 {
    let theme = Theme::no_color();
    u16::try_from(width(&pinned(model, &theme, nerd_font))).unwrap_or(u16::MAX)
}

fn pinned(model: &StatusModel, theme: &Theme, nerd_font: bool) -> Spans {
    let chip = Style::default()
        .fg(theme.accent_warning)
        .add_modifier(Modifier::REVERSED | Modifier::BOLD);
    let mut spans = Spans::new();
    if model.bypass {
        spans.extend(bypass_indicator::render_line(BypassIndicatorState::Active, theme).spans);
    }
    if model.untrusted {
        if !spans.is_empty() {
            spans.push(Span::raw(" "));
        }
        let glyph = icons::get_for_mode(IconId::ChipErrored, nerd_font);
        spans.push(Span::styled(format!(" {glyph} UNTRUSTED "), chip));
    }
    spans
}

fn update_chip(chip: &UpdateChip, theme: &Theme, nerd_font: bool) -> Spans {
    let warn = Style::default().fg(theme.accent_warning);
    let arrow = icons::get_for_mode(IconId::ArrowUp, nerd_font);
    let version = match chip {
        UpdateChip::Available(version) => version,
        UpdateChip::Queued { after } => {
            let wait = match after {
                0 => "until idle".to_string(),
                1 => "after 1 agent".to_string(),
                n => format!("after {n} agents"),
            };
            return vec![
                Span::raw(" "),
                Span::styled(format!("{arrow} queued · {wait}"), warn),
            ];
        }
    };
    vec![
        Span::raw(" "),
        Span::styled(format!("{arrow} {version} "), warn),
        Span::styled(
            "u",
            warn.add_modifier(Modifier::BOLD | Modifier::UNDERLINED),
        ),
    ]
}

fn brand(model: &StatusModel, theme: &Theme) -> Spans {
    vec![
        Span::styled(
            " MAESTRO ",
            Style::default()
                .fg(theme.branding_fg)
                .bg(theme.branding_bg)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(
            format!(" {}", model.version),
            Style::default().fg(theme.text_muted),
        ),
    ]
}

fn fleet(counts: FleetCounts, theme: &Theme, nerd_font: bool) -> Spans {
    let muted = Style::default().fg(theme.text_muted);
    if counts.total == 0 {
        let glyph = icons::get_for_mode(IconId::ChipQueued, nerd_font);
        return vec![Span::styled(format!("{glyph} no agents"), muted)];
    }
    let part = |icon, n: usize, status, label: &'static str| {
        [
            Span::styled(
                format!("{} {n}", icons::get_for_mode(icon, nerd_font)),
                Style::default().fg(theme.status_color(status)),
            ),
            Span::styled(label, muted),
        ]
    };
    let mut spans = Spans::new();
    spans.extend(part(
        IconId::ChipRunning,
        counts.running,
        SessionStatus::Running,
        " run ",
    ));
    spans.extend(part(
        IconId::ChipNeedsReview,
        counts.waiting,
        SessionStatus::NeedsReview,
        " wait ",
    ));
    spans.extend(part(
        IconId::ChipErrored,
        counts.failed,
        SessionStatus::Errored,
        " fail",
    ));
    spans
}

fn cap_text(cap: f64) -> String {
    if cap.is_finite() && cap.fract() == 0.0 {
        format!("{cap:.0}")
    } else {
        format!("{cap:.2}")
    }
}

fn budget(model: &StatusModel, theme: &Theme) -> Spans {
    let muted = Style::default().fg(theme.text_muted);
    match model.cap {
        Some(cap) if cap > 0.0 => vec![
            Span::styled(
                money(Some(model.spent)),
                gauge_level(model.spent / cap).style(theme),
            ),
            Span::styled(format!("/{}", cap_text(cap)), muted),
        ],
        _ => vec![
            Span::styled(
                money(Some(model.spent)),
                Style::default().fg(theme.text_primary),
            ),
            Span::styled(" spent", muted),
        ],
    }
}

fn turboquant(on: bool, theme: &Theme) -> Spans {
    vec![if on {
        Span::styled(
            "TQ:ON",
            Style::default()
                .fg(theme.branding_fg)
                .bg(theme.accent_success)
                .add_modifier(Modifier::BOLD),
        )
    } else {
        Span::styled("TQ:OFF", Style::default().fg(theme.text_secondary))
    }]
}

fn left_segments(model: &StatusModel, theme: &Theme, nerd_font: bool) -> Vec<(Segment, Spans)> {
    let mut out = vec![
        (Segment::Fleet, fleet(model.fleet, theme, nerd_font)),
        (Segment::Budget, budget(model, theme)),
    ];
    if let Some((done, attempted)) = model.continuous {
        out.push((
            Segment::Continuous,
            vec![Span::styled(
                format!(" CONTINUOUS: {done}/{attempted} done "),
                Style::default()
                    .fg(theme.accent_info)
                    .add_modifier(Modifier::REVERSED | Modifier::BOLD),
            )],
        ));
    }
    if !model.quota.is_empty() {
        out.push((
            Segment::Quota,
            vec![Span::styled(
                model.quota.join("  "),
                Style::default().fg(theme.text_secondary),
            )],
        ));
    }
    out.push((Segment::TurboQuant, turboquant(model.turboquant, theme)));
    out
}

/// The bar's `(left, right)` spans for `width` columns, after dropping
/// whatever does not fit.
pub fn segments(
    model: &StatusModel,
    columns: u16,
    theme: &Theme,
    nerd_font: bool,
) -> (Spans, Spans) {
    let columns = usize::from(columns);
    let sep = Span::styled(
        format!(" {} ", icons::get_for_mode(IconId::SeparatorV, nerd_font)),
        Style::default().fg(theme.text_muted),
    );
    let mut left = left_segments(model, theme, nerd_font);
    let mut update = model
        .update
        .as_ref()
        .map(|chip| update_chip(chip, theme, nerd_font));
    let pinned = pinned(model, theme, nerd_font);
    let brand = brand(model, theme);

    let fits = |left: &[(Segment, Spans)], update: &Option<Spans>| {
        let left_w = width(&brand)
            + left
                .iter()
                .map(|(_, s)| width(s) + sep.width())
                .sum::<usize>();
        let right_w = width(&pinned) + update.as_deref().map_or(0, width);
        let gap = usize::from(right_w > 0);
        left_w + gap + right_w <= columns
    };

    if !fits(&left, &update) {
        update = None;
    }
    for kind in DROP_ORDER {
        if fits(&left, &update) {
            break;
        }
        left.retain(|(k, _)| *k != kind);
    }

    let mut left_spans = brand;
    for (_, spans) in left {
        left_spans.push(sep.clone());
        left_spans.extend(spans);
    }
    let mut right_spans = pinned;
    right_spans.extend(update.unwrap_or_default());
    (left_spans, right_spans)
}

/// Draws the bar on one row of `area` in the current icon mode.
pub fn draw(f: &mut Frame, area: Rect, model: &StatusModel, theme: &Theme) {
    draw_for_mode(f, area, model, theme, icons::use_nerd_font());
}

/// [`draw`] with the icon mode passed in (use it in tests).
pub fn draw_for_mode(
    f: &mut Frame,
    area: Rect,
    model: &StatusModel,
    theme: &Theme,
    nerd_font: bool,
) {
    let row = Rect::new(area.x, area.y, area.width, area.height.min(1));
    let bar = Style::default().bg(theme.surface_bar);
    f.render_widget(Paragraph::new("").style(bar), row);

    let (left, right) = segments(model, row.width, theme, nerd_font);
    // The right group is drawn last over its own columns, so the safety
    // chips stay whole even when the left group has to be clipped.
    let right_w = u16::try_from(width(&right))
        .unwrap_or(u16::MAX)
        .min(row.width);
    let left_area = Rect::new(row.x, row.y, row.width - right_w, row.height);
    let right_area = Rect::new(row.x + row.width - right_w, row.y, right_w, row.height);
    f.render_widget(Paragraph::new(Line::from(left)).style(bar), left_area);
    f.render_widget(Paragraph::new(Line::from(right)).style(bar), right_area);
}

#[cfg(test)]
#[path = "status_bar_tests.rs"]
mod tests;
