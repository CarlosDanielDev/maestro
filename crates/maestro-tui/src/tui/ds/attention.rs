//! Needs-you queue (`ds::attention_queue` in `docs/design-system/index.html`,
//! §04 AttentionQueue, §02 Home, §05 "Missing an agent that waits on you").
//!
//! [`rank`] turns plain signals into one ranked list: errors, then agents
//! waiting on you, then ready-to-ship work, then resource notices. Kind comes
//! first; inside a kind the oldest item wins, and items of the same age keep
//! their input order. [`draw`] renders the list as a panel, and
//! [`terminal_title`] gives the `(n) maestro` title.

use chrono::{DateTime, Utc};
use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;
use unicode_width::UnicodeWidthStr;

use crate::session::types::SessionStatus;
use crate::tui::ds::bar_row::key_style;
use crate::tui::ds::gauge::gauge_level;
use crate::tui::ds::money::money;
use crate::tui::ds::panel::{Focus, panel};
use crate::tui::ds::status_chip::status_chip_for_mode;
use crate::tui::icons::{IconId, get_for_mode};
use crate::tui::screens::landing::draw::ellipsize;
use crate::tui::screens::sanitize_for_terminal;
use crate::tui::theme::Theme;
use crate::tui::widgets::focused_selection_style;

/// Rank order: lower sorts first.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum AttentionKind {
    /// Gates failed, a session errored, a doctor check failed, gh auth lost.
    Error,
    /// An agent waits for your review.
    NeedsReview,
    /// Work is done; the PR is ready to open.
    NeedsPr,
    /// Budget or quota notice.
    Resource,
}

/// What the row points at; `⏎` jumps there.
#[derive(Debug, Clone, PartialEq)]
pub enum Subject {
    Session {
        id: uuid::Uuid,
        issue: Option<u64>,
        status: SessionStatus,
    },
    DoctorCheck,
    GhAuth,
    Budget,
    Quota {
        provider: String,
    },
}

#[derive(Debug, Clone, PartialEq)]
pub struct AttentionItem {
    pub kind: AttentionKind,
    pub subject: Subject,
    /// One line, already sanitized for the terminal.
    pub reason: String,
    /// The primary action, always on `⏎`.
    pub action: &'static str,
    /// When the item started to need you. `None` sorts after dated items.
    pub since: Option<DateTime<Utc>>,
}

/// One session as `rank` needs it.
#[derive(Debug, Clone)]
pub struct SessionSignal {
    pub id: uuid::Uuid,
    pub issue: Option<u64>,
    pub title: Option<String>,
    pub status: SessionStatus,
    pub since: Option<DateTime<Utc>>,
}

/// The global budget, only when it alerts (80%) or is spent.
#[derive(Debug, Clone, Copy)]
pub struct BudgetSignal {
    pub spent: f64,
    pub cap: f64,
}

/// A provider whose busiest usage window is high.
#[derive(Debug, Clone)]
pub struct QuotaSignal {
    pub provider: String,
    pub used_pct: f64,
}

/// Plain snapshot of everything `rank` reads. No `App` needed.
#[derive(Debug, Clone, Default)]
pub struct AttentionInput {
    pub sessions: Vec<SessionSignal>,
    /// Failed doctor checks as `name: message`.
    pub doctor_failures: Vec<String>,
    pub gh_auth_lost: bool,
    pub budget: Option<BudgetSignal>,
    pub quota: Vec<QuotaSignal>,
}

/// Rank everything that needs the operator. Kind first; inside a kind the
/// oldest `since` first, undated items after dated ones, ties in input order.
pub fn rank(input: &AttentionInput) -> Vec<AttentionItem> {
    let mut items: Vec<AttentionItem> = input.sessions.iter().filter_map(session_item).collect();
    for failure in &input.doctor_failures {
        // A lost login is its own row; do not show it twice.
        if input.gh_auth_lost && failure.starts_with("gh auth:") {
            continue;
        }
        items.push(item(
            AttentionKind::Error,
            Subject::DoctorCheck,
            failure,
            "details",
        ));
    }
    if input.gh_auth_lost {
        items.push(item(
            AttentionKind::Error,
            Subject::GhAuth,
            "GitHub CLI is logged out · run gh auth login",
            "fix",
        ));
    }
    if let Some(b) = input.budget {
        let reason = format!("{} of {} spent", money(Some(b.spent)), money(Some(b.cap)));
        items.push(item(
            AttentionKind::Resource,
            Subject::Budget,
            &reason,
            "budget",
        ));
    }
    for q in &input.quota {
        let reason = format!("{:.0}% of a usage window used", q.used_pct);
        let subject = Subject::Quota {
            provider: q.provider.clone(),
        };
        items.push(item(AttentionKind::Resource, subject, &reason, "quota"));
    }
    // Stable sort: equal keys keep their input order.
    items.sort_by_key(|i| (i.kind, i.since.is_none(), i.since));
    items
}

fn item(
    kind: AttentionKind,
    subject: Subject,
    reason: &str,
    action: &'static str,
) -> AttentionItem {
    AttentionItem {
        kind,
        subject,
        reason: sanitize_for_terminal(reason),
        action,
        since: None,
    }
}

fn session_item(s: &SessionSignal) -> Option<AttentionItem> {
    let (kind, why, action) = match s.status {
        SessionStatus::FailedGates => (AttentionKind::Error, "completion gates failed", "open"),
        SessionStatus::Errored => (
            AttentionKind::Error,
            "session stopped with an error",
            "open",
        ),
        SessionStatus::NeedsReview => (
            AttentionKind::NeedsReview,
            "waiting for your review",
            "review",
        ),
        SessionStatus::NeedsPr => (
            AttentionKind::NeedsPr,
            "work done, PR not opened yet",
            "open PR",
        ),
        _ => return None,
    };
    let reason = match &s.title {
        Some(title) => format!("{title} · {why}"),
        None => why.to_string(),
    };
    let subject = Subject::Session {
        id: s.id,
        issue: s.issue,
        status: s.status,
    };
    Some(AttentionItem {
        since: s.since,
        ..item(kind, subject, &reason, action)
    })
}

/// `maestro`, or `(n) maestro` when `n` items need you.
pub fn terminal_title(count: usize) -> String {
    if count == 0 {
        exit_title().to_string()
    } else {
        format!("({count}) maestro")
    }
}

/// The title left behind on quit, so the shell tab keeps no stale count.
pub fn exit_title() -> &'static str {
    "maestro"
}

/// How the panel is drawn. `fleet_busy` picks the empty copy.
#[derive(Debug, Clone, Copy)]
pub struct QueueView {
    pub focused: bool,
    pub selected: usize,
    pub fleet_busy: bool,
    pub now: DateTime<Utc>,
}

pub fn draw(
    f: &mut Frame,
    area: Rect,
    items: &[AttentionItem],
    view: QueueView,
    theme: &Theme,
    nerd_font: bool,
) {
    let title = if items.is_empty() {
        "Needs you".to_string()
    } else {
        format!("Needs you ({})", items.len())
    };
    let focus = if view.focused {
        Focus::Active
    } else {
        Focus::Inactive
    };
    let block = panel(&title, focus, theme);
    let inner = block.inner(area);
    f.render_widget(block, area);

    let lines: Vec<Line> = if items.is_empty() {
        empty_lines(view.fleet_busy, theme, nerd_font)
    } else {
        items
            .iter()
            .enumerate()
            .map(|(i, it)| {
                let selected = view.focused && i == view.selected;
                row(
                    it,
                    selected,
                    inner.width as usize,
                    view.now,
                    theme,
                    nerd_font,
                )
            })
            .collect()
    };
    f.render_widget(Paragraph::new(lines), inner);
}

/// The empty state teaches the next step: start something when idle.
fn empty_lines(fleet_busy: bool, theme: &Theme, nerd_font: bool) -> Vec<Line<'static>> {
    let ok = Style::new()
        .fg(theme.accent_success)
        .add_modifier(Modifier::BOLD);
    let check = Span::styled(
        format!(" {} ", get_for_mode(IconId::ChipCompleted, nerd_font)),
        ok,
    );
    let text = Style::new().fg(theme.text_primary);
    if fleet_busy {
        return vec![Line::from(vec![
            check,
            Span::styled("All clear. Agents are working.", text),
        ])];
    }
    let key = key_style(theme);
    let muted = Style::new().fg(theme.text_muted);
    vec![
        Line::from(vec![
            check,
            Span::styled("Nothing needs you. Start something: i f a", text),
        ]),
        Line::from(vec![
            Span::styled("   i", key),
            Span::styled(" new issue   ", muted),
            Span::styled("f", key),
            Span::styled(" fleet board   ", muted),
            Span::styled("a", key),
            Span::styled(" adopt a Claude Code session", muted),
        ]),
    ]
}

/// Widest chip word (`✗ gates failed`) plus one space.
const CHIP_W: usize = 15;
const SUBJECT_W: usize = 9;
const AGE_W: usize = 4;
/// Widest action (`open PR`, `details`).
const ACTION_W: usize = 7;

fn row(
    it: &AttentionItem,
    selected: bool,
    width: usize,
    now: DateTime<Utc>,
    theme: &Theme,
    nerd_font: bool,
) -> Line<'static> {
    let chip = chip(it, theme, nerd_font);
    let subject = match &it.subject {
        Subject::Session { issue: Some(n), .. } => format!("#{n}"),
        Subject::Session { id, .. } => id.to_string().chars().take(8).collect(),
        Subject::DoctorCheck => "doctor".into(),
        Subject::GhAuth => "gh".into(),
        Subject::Budget => "$".into(),
        Subject::Quota { provider } => sanitize_for_terminal(provider),
    };
    let age = it.since.map(|t| age(now - t)).unwrap_or_default();
    let key = get_for_mode(IconId::Enter, nerd_font);
    // Marker, chip, subject, then ` age key action` after the reason.
    let fixed = 1 + CHIP_W + SUBJECT_W + 1 + AGE_W + 1 + key.width() + 1 + ACTION_W;
    let reason_w = width.saturating_sub(fixed);
    let reason = ellipsize(&it.reason, reason_w);

    let muted = Style::new().fg(theme.text_muted);
    let mut spans = vec![
        Span::raw(if selected { "▌" } else { " " }),
        pad(chip, CHIP_W),
        Span::styled(
            format!("{:<SUBJECT_W$}", ellipsize(&subject, SUBJECT_W - 1)),
            Style::new()
                .fg(theme.accent_identifier)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(format!("{reason:<reason_w$}"), muted),
        Span::styled(format!(" {age:>AGE_W$} "), muted),
        Span::styled(key.to_string(), key_style(theme)),
        Span::styled(format!(" {:<ACTION_W$}", it.action), muted),
    ];
    if selected {
        let sel = focused_selection_style(theme);
        for span in &mut spans {
            span.style = span.style.patch(sel);
        }
    }
    Line::from(spans)
}

/// `glyph word`: the session chip for sessions; its own word otherwise.
fn chip(it: &AttentionItem, theme: &Theme, nerd_font: bool) -> Span<'static> {
    let (icon, word, style) = match &it.subject {
        Subject::Session { status, .. } => return status_chip_for_mode(*status, theme, nerd_font),
        Subject::DoctorCheck => (IconId::ChipErrored, "doctor", error_style(theme)),
        Subject::GhAuth => (IconId::ChipErrored, "gh auth", error_style(theme)),
        Subject::Budget => (IconId::Warning, "budget", gauge_level(0.8).style(theme)),
        Subject::Quota { .. } => (IconId::Warning, "quota", gauge_level(0.8).style(theme)),
    };
    Span::styled(format!("{} {word}", get_for_mode(icon, nerd_font)), style)
}

fn error_style(theme: &Theme) -> Style {
    Style::new()
        .fg(theme.accent_error)
        .add_modifier(Modifier::BOLD)
}

/// Pad a span to `width` columns (one space at least).
fn pad(span: Span<'static>, width: usize) -> Span<'static> {
    let gap = width.saturating_sub(span.content.width()).max(1);
    Span::styled(format!("{}{}", span.content, " ".repeat(gap)), span.style)
}

/// `45s`, `12m`, `3h`, `2d`.
fn age(d: chrono::Duration) -> String {
    let s = d.num_seconds().max(0);
    match s {
        0..60 => format!("{s}s"),
        60..3_600 => format!("{}m", s / 60),
        3_600..86_400 => format!("{}h", s / 3_600),
        _ => format!("{}d", s / 86_400),
    }
}

#[cfg(test)]
#[path = "attention_tests.rs"]
mod tests;
