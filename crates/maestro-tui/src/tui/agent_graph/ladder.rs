//! The ladder of the Files lens (§02.20 "Ladder", #1292): agents left,
//! files right, Braille S-curve edges between, one detail panel for the
//! selection. One row per item, so labels never collide at any count.
//! Every colour is a `Theme` token: status from `theme.status_color`
//! (via `ds::status_chip`), the selection's edges `border_active`, edges
//! into a contested file `accent_warning`, the rest `text_muted`. Read
//! edges are not drawn; the file column counts read-only files.
//!
//! Wide puts the detail panel right of the ladder, Regular in a strip
//! under it. Compact (< 100 cols) has no canvas: the agents, then the
//! files with their editors (`#407 #409`) on each row.

use ratatui::Frame;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::symbols::Marker;
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;
use ratatui::widgets::canvas::{Canvas, Line as Edge};
use unicode_width::UnicodeWidthStr;
use uuid::Uuid;

use super::model::{FileRow, FilesGraph, agent_label};
use crate::session::types::Session;
use crate::tui::ds::breakpoint::Breakpoint;
use crate::tui::ds::fit::ellipsize_line;
use crate::tui::ds::panel::{Focus, panel};
use crate::tui::ds::status_chip::status_chip_for_mode;
use crate::tui::screens::sanitize_for_terminal;
use crate::tui::theme::Theme;
use crate::tui::widgets::focused_selection_style;

/// What is selected: an agent (shared with the Work list) or a file.
pub(crate) struct Selection<'a> {
    pub(crate) agent: Option<Uuid>,
    pub(crate) file: Option<&'a str>,
    pub(crate) files_focused: bool,
}

impl Selection<'_> {
    pub(super) fn is_file(&self, row: &FileRow) -> bool {
        self.files_focused && self.file == Some(row.path.as_str())
    }

    fn is_agent(&self, id: Uuid) -> bool {
        !self.files_focused && self.agent == Some(id)
    }
}

/// Everything one draw reads.
pub(super) struct Ctx<'a> {
    pub(super) graph: &'a FilesGraph,
    sessions: &'a [&'a Session],
    pub(super) sel: &'a Selection<'a>,
    pub(super) theme: &'a Theme,
    pub(super) nerd_font: bool,
}

impl Ctx<'_> {
    pub(super) fn session(&self, id: Uuid) -> Option<&Session> {
        self.sessions.iter().copied().find(|s| s.id == id)
    }

    pub(super) fn name(&self, id: Uuid) -> String {
        self.session(id).map_or_else(|| "?".into(), agent_label)
    }

    pub(super) fn muted(&self) -> Style {
        Style::new().fg(self.theme.text_muted)
    }
}

/// A line of the file column: a section header, a file, or the read-only
/// count.
enum FileLine<'a> {
    Header(&'static str, usize),
    File(&'a FileRow),
    ReadOnly(usize),
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn draw(
    f: &mut Frame,
    area: Rect,
    graph: &FilesGraph,
    sessions: &[&Session],
    sel: &Selection<'_>,
    bp: Breakpoint,
    theme: &Theme,
    nerd_font: bool,
) {
    let cx = Ctx {
        graph,
        sessions,
        sel,
        theme,
        nerd_font,
    };
    let (ladder, detail) = match bp {
        Breakpoint::Wide => {
            let [l, d] =
                Layout::horizontal([Constraint::Min(0), Constraint::Length(46)]).areas(area);
            (l, d)
        }
        _ => {
            let [l, d] = Layout::vertical([Constraint::Min(0), Constraint::Length(6)]).areas(area);
            (l, d)
        }
    };
    let block = panel("Files · worktrees merged", Focus::Active, theme);
    let inner = block.inner(ladder);
    f.render_widget(block, ladder);
    if graph.agents.is_empty() {
        let text = " No agents yet · this lens shows which agents edit the same files";
        f.render_widget(Paragraph::new(Span::styled(text, cx.muted())), inner);
    } else if bp == Breakpoint::Compact {
        draw_compact(f, inner, &cx);
    } else {
        draw_columns(f, inner, &cx, bp);
    }
    super::file_card::draw(f, detail, &cx, bp == Breakpoint::Wide);
}

fn file_lines(graph: &FilesGraph) -> Vec<FileLine<'_>> {
    let mut out = Vec::new();
    for (label, contested) in [("CONTESTED", true), ("EDITED", false)] {
        let rows: Vec<&FileRow> = (graph.files.iter())
            .filter(|r| !r.editors.is_empty() && r.contested() == contested)
            .collect();
        if label == "CONTESTED" && rows.is_empty() {
            continue;
        }
        out.push(FileLine::Header(label, rows.len()));
        out.extend(rows.into_iter().map(FileLine::File));
    }
    let reads = graph.files.iter().filter(|r| r.editors.is_empty()).count();
    if reads > 0 {
        out.push(FileLine::ReadOnly(reads));
    }
    out
}

fn draw_columns(f: &mut Frame, area: Rect, cx: &Ctx<'_>, bp: Breakpoint) {
    let (aw, bw) = if bp == Breakpoint::Wide {
        (40, 14)
    } else {
        (36, 10)
    };
    let [agents, band, files] = Layout::horizontal([
        Constraint::Length(aw),
        Constraint::Length(bw),
        Constraint::Min(0),
    ])
    .areas(area);
    let flines = file_lines(cx.graph);
    let n = cx.graph.agents.len();
    // Agents spread over the height of the file column, under a header.
    let total = flines.len().max(n + 1).max(1);
    let gap = ((total - 1) / n).max(1);
    let agent_y: Vec<usize> = (0..n).map(|i| 1 + i * gap).collect();
    let file_y = |path: &str| {
        flines
            .iter()
            .position(|l| matches!(l, FileLine::File(r) if r.path == path))
    };
    let height = area.height as usize;
    let focus = if cx.sel.files_focused {
        cx.sel.file.and_then(file_y)
    } else {
        (cx.sel.agent)
            .and_then(|id| cx.graph.agents.iter().position(|a| *a == id))
            .map(|i| agent_y[i])
    };
    let offset = focus.map_or(0, |y| (y + 1).saturating_sub(height));

    let mut left = vec![Line::default(); total];
    left[0] = header("AGENTS", n, cx.theme);
    for (i, id) in cx.graph.agents.iter().enumerate() {
        left[agent_y[i]] = agent_line(*id, agents.width as usize, cx);
    }
    let right: Vec<Line> = flines
        .iter()
        .map(|l| file_line(l, files.width as usize, cx))
        .collect();
    let window = |lines: Vec<Line<'static>>| -> Vec<Line<'static>> {
        lines.into_iter().skip(offset).take(height).collect()
    };
    f.render_widget(Paragraph::new(window(left)), agents);
    f.render_widget(Paragraph::new(window(right)), files);
    draw_edges(f, band, cx, &agent_y, &flines, offset);
}

/// Braille S-curves, flat at both ends so they meet the rows.
fn draw_edges(
    f: &mut Frame,
    band: Rect,
    cx: &Ctx<'_>,
    agent_y: &[usize],
    flines: &[FileLine<'_>],
    offset: usize,
) {
    let (w, h) = (f64::from(band.width), f64::from(band.height));
    let y_of = |line: usize| h - (line as f64 - offset as f64) - 0.5;
    // Muted first, the selection last, so it draws on top.
    let mut edges: Vec<(u8, f64, f64, Color)> = Vec::new();
    for (line, fl) in flines.iter().enumerate() {
        let FileLine::File(row) = fl else { continue };
        for editor in &row.editors {
            let Some(i) = cx.graph.agents.iter().position(|a| a == editor) else {
                continue;
            };
            let (rank, color) = if cx.sel.is_file(row) || cx.sel.is_agent(*editor) {
                (2, cx.theme.border_active)
            } else if row.contested() {
                (1, cx.theme.accent_warning)
            } else {
                (0, cx.theme.text_muted)
            };
            edges.push((rank, y_of(agent_y[i]), y_of(line), color));
        }
    }
    edges.sort_by_key(|e| e.0);
    let canvas = Canvas::default()
        .marker(Marker::Braille)
        .x_bounds([0.0, w])
        .y_bounds([0.0, h])
        .paint(move |ctx| {
            const STEPS: usize = 12;
            for &(_, y1, y2, color) in &edges {
                let at = |k: usize| {
                    let t = k as f64 / STEPS as f64;
                    let s = t * t * (3.0 - 2.0 * t);
                    (w * t, y1 + (y2 - y1) * s)
                };
                for k in 0..STEPS {
                    let ((x1, ya), (x2, yb)) = (at(k), at(k + 1));
                    ctx.draw(&Edge::new(x1, ya, x2, yb, color));
                }
            }
        });
    f.render_widget(canvas, band);
}

fn header(label: &str, n: usize, theme: &Theme) -> Line<'static> {
    let bold = Style::new().add_modifier(Modifier::BOLD);
    Line::from(vec![
        Span::styled(format!(" {label}"), bold.fg(theme.text_muted)),
        Span::styled(format!("  {n}"), bold.fg(theme.text_primary)),
    ])
}

/// `▌` and the focused selection style on the selected row.
fn mark(spans: Vec<Span<'static>>, selected: bool, width: usize, theme: &Theme) -> Line<'static> {
    let mut all = vec![Span::raw(if selected { "▌" } else { " " })];
    all.extend(spans);
    let line = ellipsize_line(Line::from(all), width);
    if selected {
        line.style(focused_selection_style(theme))
    } else {
        line
    }
}

/// `⠋ running  #407 auth-refactor  ✎4`, the Work list's chip and identity.
fn agent_line(id: Uuid, width: usize, cx: &Ctx<'_>) -> Line<'static> {
    let Some(s) = cx.session(id) else {
        return Line::default();
    };
    let chip = status_chip_for_mode(s.status, cx.theme, cx.nerd_font);
    let edits = format!(" ✎{}", cx.graph.edits_of(id));
    let name = agent_label(s);
    let title = sanitize_for_terminal(s.issue_title.as_deref().unwrap_or(&s.prompt));
    let room = width
        .saturating_sub(1 + chip.content.width() + 1 + name.width() + 1 + edits.width())
        .max(1);
    let title = crate::tui::screens::landing::draw::ellipsize(&title, room);
    let spans = vec![
        Span::styled(format!("{} ", chip.content), chip.style),
        Span::styled(
            format!("{name} "),
            Style::new()
                .fg(cx.theme.accent_identifier)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(
            format!("{title:<room$}"),
            Style::new().fg(cx.theme.text_primary),
        ),
        Span::styled(edits, cx.muted()),
    ];
    mark(spans, cx.sel.is_agent(id), width, cx.theme)
}

fn file_line(line: &FileLine<'_>, width: usize, cx: &Ctx<'_>) -> Line<'static> {
    let row = match line {
        FileLine::Header(label, n) => return header(label, *n, cx.theme),
        FileLine::ReadOnly(n) => {
            return Line::from(Span::styled(format!("   + {n} read only"), cx.muted()));
        }
        FileLine::File(row) => row,
    };
    let mut tail = format!(" {}✎", row.editors.len());
    if row.conflict {
        tail.push_str(" ✗ conflict");
    }
    let path_w = width.saturating_sub(2 + tail.width()).max(1);
    let path_style = if row.contested() {
        Style::new()
            .fg(cx.theme.accent_warning)
            .add_modifier(Modifier::BOLD)
    } else {
        Style::new().fg(cx.theme.text_primary)
    };
    let tail_style = if row.conflict {
        Style::new().fg(cx.theme.status_conflict)
    } else {
        cx.muted()
    };
    let spans = vec![
        Span::styled(format!("{:<path_w$}", mid(&row.path, path_w)), path_style),
        Span::styled(tail, tail_style),
    ];
    mark(spans, cx.sel.is_file(row), width, cx.theme)
}

/// Cut the middle of a path so its file name survives: `src/…/ui.rs`.
pub(super) fn mid(path: &str, width: usize) -> String {
    let path = sanitize_for_terminal(path);
    if path.width() <= width {
        return path;
    }
    let name = path.rsplit('/').next().unwrap_or(&path);
    if name.width() + 1 >= width {
        return crate::tui::screens::landing::draw::ellipsize(name, width);
    }
    let head: String = path.chars().take(width - name.width() - 2).collect();
    format!("{head}…/{name}")
}

/// Compact: the agents, then the files with their editors on each row.
fn draw_compact(f: &mut Frame, area: Rect, cx: &Ctx<'_>) {
    let width = area.width as usize;
    let mut lines = vec![header("AGENTS", cx.graph.agents.len(), cx.theme)];
    lines.extend(cx.graph.agents.iter().map(|id| agent_line(*id, width, cx)));
    let mut focus = (cx.sel.agent)
        .filter(|_| !cx.sel.files_focused)
        .and_then(|id| cx.graph.agents.iter().position(|a| *a == id))
        .map(|i| i + 1);
    for fl in file_lines(cx.graph) {
        let line = match fl {
            FileLine::File(row) => {
                if cx.sel.is_file(row) {
                    focus = Some(lines.len());
                }
                let editors: Vec<String> = row.editors.iter().map(|e| cx.name(*e)).collect();
                let mut l = file_line(&fl, width.saturating_sub(editors.join(" ").width() + 1), cx);
                l.spans
                    .push(Span::styled(format!(" {}", editors.join(" ")), cx.muted()));
                l
            }
            _ => file_line(&fl, width, cx),
        };
        lines.push(line);
    }
    let height = area.height as usize;
    let offset = focus.map_or(0, |y| (y + 1).saturating_sub(height));
    let shown: Vec<Line> = lines.into_iter().skip(offset).take(height).collect();
    f.render_widget(Paragraph::new(shown), area);
}
