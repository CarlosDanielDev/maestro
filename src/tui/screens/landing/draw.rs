use super::types::MENU_ITEMS;
use super::{LandingScreen, NetworkMeasureState, NetworkRates, network_rates};
use crate::changelog::{self, ChangeCategory, ChangeItem};
use crate::tui::icons;
use crate::tui::theme::Theme;
use ratatui::{
    Frame,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    symbols::Marker,
    text::{Line, Span},
    widgets::{
        BarChart, Block, Borders, Paragraph, Sparkline,
        canvas::{Canvas, Line as CanvasLine},
    },
};
use unicode_width::{UnicodeWidthChar, UnicodeWidthStr};

const NARROW_WIDTH: u16 = 60;
const MIN_MENU_PADDING: u16 = 2;
const COLUMN_GAP: u16 = 4;
const MENU_KEY_LABEL_GAP: &str = "  ";
const FOOTER_HINT: &str = "<q> quit  <n> notes";
const WIDE_FOOTER_HINT: &str = "<q> quit  <n> notes  <w> net";
const CONSOLE_GAP: u16 = 1;

const ASCII_HEADER: &[&str] = &[
    " __  __    _    _____ ____ _____ ____   ___ ",
    "|  \\/  |  / \\  | ____/ ___|_   _|  _ \\ / _ \\",
    "| |\\/| | / _ \\ |  _| \\___ \\ | | | |_) | | | |",
    "| |  | |/ ___ \\| |___ ___) || | |  _ <| |_| |",
    "|_|  |_/_/   \\_\\_____|____/ |_| |_| \\_\\\\___/",
];

const NERD_HEADER: &[&str] = &[
    "███╗   ███╗ █████╗ ███████╗███████╗████████╗██████╗  ██████╗",
    "████╗ ████║██╔══██╗██╔════╝██╔════╝╚══██╔══╝██╔══██╗██╔═══██╗",
    "██╔████╔██║███████║█████╗  ███████╗   ██║   ██████╔╝██║   ██║",
    "██║╚██╔╝██║██╔══██║██╔══╝  ╚════██║   ██║   ██╔══██╗██║   ██║",
    "██║ ╚═╝ ██║██║  ██║███████╗███████║   ██║   ██║  ██║╚██████╔╝",
    "╚═╝     ╚═╝╚═╝  ╚═╝╚══════╝╚══════╝   ╚═╝   ╚═╝  ╚═╝ ╚═════╝",
];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct WelcomeLayout {
    pub menu_x: u16,
    pub menu_y: u16,
    pub menu_width: u16,
    pub left_padding: u16,
    pub right_padding: u16,
    pub columns: u16,
    pub show_header: bool,
    pub footer_y: u16,
    pub console_tier: Option<ConsoleTier>,
    pub console_x: u16,
    pub console_width: u16,
    pub console_y: u16,
    pub console_height: u16,
}

/// How much of the Release Console fits (#1218). One landing at every size:
/// smaller areas drop panels instead of switching to another screen.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ConsoleTier {
    /// Summary line + highlights.
    Small,
    /// Adds the Mix and Ref Trend charts.
    Medium,
    /// Adds the Internet and Link panels.
    Full,
}

impl ConsoleTier {
    /// Min and max console height, border included.
    pub(crate) fn height_bounds(self) -> (u16, u16) {
        match self {
            Self::Small => (7, 7),
            Self::Medium => (13, 18),
            Self::Full => (16, 30),
        }
    }
}

pub(crate) fn console_tier(area: Rect) -> Option<ConsoleTier> {
    if area.width < 80 || area.height < 24 {
        None
    } else if area.width >= 140 && area.height >= 34 {
        Some(ConsoleTier::Full)
    } else if area.width >= 100 && area.height >= 30 {
        Some(ConsoleTier::Medium)
    } else {
        Some(ConsoleTier::Small)
    }
}

/// `[summary, network, charts]` row heights inside the console border; the
/// highlights take what is left.
pub(crate) fn console_row_heights(tier: ConsoleTier, inner_height: u16) -> [u16; 3] {
    match tier {
        ConsoleTier::Full => {
            let network = inner_height.saturating_sub(14).clamp(6, 10);
            let charts = inner_height.saturating_sub(network + 8).clamp(5, 9);
            [2, network, charts]
        }
        ConsoleTier::Medium => [1, 0, inner_height.saturating_sub(7).clamp(5, 9)],
        ConsoleTier::Small => [1, 0, 0],
    }
}

#[derive(Debug, Clone, Copy)]
struct RenderMode {
    use_nerd_font: bool,
    styled: bool,
}

struct MenuItemRender<'a> {
    x: u16,
    y: u16,
    width: u16,
    shortcut: char,
    label: &'a str,
    selected: bool,
}

pub(crate) fn welcome_layout(area: Rect) -> WelcomeLayout {
    let show_header = area.width >= NARROW_WIDTH;
    let columns = if area.width >= 80 { 2 } else { 1 };
    let max_content_width = area.width.saturating_sub(MIN_MENU_PADDING * 2).max(1);
    let base_menu_width = menu_block_width(columns).min(max_content_width).max(1);
    let header_height = if show_header {
        NERD_HEADER.len() as u16 + 2
    } else {
        0
    };
    let menu_height = menu_rows(columns);
    let tier = console_tier(area);
    let console_height = tier.map_or(0, |t| {
        console_height(t, area.height, header_height, menu_height)
    });
    let menu_width = base_menu_width;
    let stage_width = match tier {
        Some(ConsoleTier::Full) => area.width.max(menu_width),
        Some(_) => max_content_width.max(menu_width),
        None => menu_width,
    };
    let console_width = if tier.is_some() { stage_width } else { 0 };
    let console_gap = if tier.is_some() { CONSOLE_GAP } else { 0 };
    let left_padding = area.width.saturating_sub(stage_width) / 2;
    let right_padding = area.width.saturating_sub(stage_width + left_padding);

    let content_height = menu_height + console_gap + console_height;
    let total_height = header_height + content_height + 2;
    let slack = area.height.saturating_sub(total_height);
    let top_padding = if area.height >= 30 {
        slack / 3
    } else {
        slack / 2
    };
    let y_start = area.y + top_padding;
    let menu_y = y_start + header_height;
    let stage_x = area.x + left_padding;
    let menu_x = stage_x + stage_width.saturating_sub(menu_width) / 2;
    let console_y = menu_y + menu_height + console_gap;
    // A blank line separates the footer when it fits. On a short terminal the
    // footer takes that row instead, so the quit and notes hints stay visible.
    let footer_gap = if console_y + console_height + 1 < area.y + area.height {
        1
    } else {
        0
    };
    let footer_y = console_y + console_height + footer_gap;

    WelcomeLayout {
        menu_x,
        menu_y,
        menu_width,
        left_padding,
        right_padding,
        columns,
        show_header,
        footer_y,
        console_tier: tier,
        console_x: stage_x,
        console_width,
        console_y,
        console_height,
    }
}

fn console_height(
    tier: ConsoleTier,
    area_height: u16,
    header_height: u16,
    menu_height: u16,
) -> u16 {
    let (min, max) = tier.height_bounds();
    area_height
        .saturating_sub(header_height)
        .saturating_sub(menu_height)
        .saturating_sub(CONSOLE_GAP)
        .saturating_sub(2)
        .clamp(min, max)
}

impl LandingScreen {
    pub(super) fn draw_impl(&self, f: &mut Frame, area: Rect, theme: &Theme) {
        let mode = RenderMode {
            use_nerd_font: icons::use_nerd_font(),
            styled: should_emit_styles(),
        };
        self.draw_welcome(f, area, theme, mode);
    }

    #[cfg(test)]
    pub(crate) fn draw_impl_for_test(
        &self,
        f: &mut Frame,
        area: Rect,
        theme: &Theme,
        use_nerd_font: bool,
        styled: bool,
    ) {
        self.draw_welcome(
            f,
            area,
            theme,
            RenderMode {
                use_nerd_font,
                styled,
            },
        );
    }

    fn draw_welcome(&self, f: &mut Frame, area: Rect, theme: &Theme, mode: RenderMode) {
        let layout = welcome_layout(area);
        if layout.show_header {
            self.draw_header(f, area, &layout, theme, mode);
        }
        self.draw_menu(f, area, &layout, theme, mode);
        self.draw_release_console(f, area, &layout, theme, mode);
        self.draw_footer(f, area, &layout, theme, mode);
    }

    fn draw_header(
        &self,
        f: &mut Frame,
        area: Rect,
        layout: &WelcomeLayout,
        theme: &Theme,
        mode: RenderMode,
    ) {
        let header = header_for_width(mode.use_nerd_font, area.width);
        let header_y = layout
            .menu_y
            .saturating_sub(header.len() as u16)
            .saturating_sub(2);
        let style = if mode.styled {
            Style::default()
                .fg(theme.accent_success)
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default()
        };

        for (idx, text) in header.iter().enumerate() {
            render_centered_text(f, area, header_y + idx as u16, text, style);
        }

        let version = format!("v{}", env!("CARGO_PKG_VERSION"));
        let version_style = if mode.styled {
            Style::default().fg(theme.text_secondary)
        } else {
            Style::default()
        };
        render_centered_text(
            f,
            area,
            header_y + header.len() as u16,
            &version,
            version_style,
        );
    }

    fn draw_menu(
        &self,
        f: &mut Frame,
        area: Rect,
        layout: &WelcomeLayout,
        theme: &Theme,
        mode: RenderMode,
    ) {
        let rows = menu_rows(layout.columns);
        let column_width = column_width_for_layout(layout);

        for row in 0..rows {
            let y = layout.menu_y + row;
            if y >= area.y + area.height {
                break;
            }

            for col in 0..layout.columns {
                let idx = row as usize + (col as usize * rows as usize);
                let Some(item) = MENU_ITEMS.get(idx) else {
                    continue;
                };

                let x = layout.menu_x + col * (column_width + COLUMN_GAP);
                let selected = idx == self.selected;
                let item_width = if col + 1 == layout.columns {
                    layout
                        .menu_width
                        .saturating_sub(col * (column_width + COLUMN_GAP))
                } else {
                    column_width
                };
                render_menu_item(
                    f,
                    MenuItemRender {
                        x,
                        y,
                        width: item_width,
                        shortcut: item.shortcut,
                        label: item.label,
                        selected,
                    },
                    theme,
                    mode,
                );
            }
        }
    }

    fn draw_footer(
        &self,
        f: &mut Frame,
        area: Rect,
        layout: &WelcomeLayout,
        theme: &Theme,
        mode: RenderMode,
    ) {
        if layout.footer_y >= area.y + area.height {
            return;
        }

        let style = if mode.styled {
            Style::default().fg(theme.text_secondary)
        } else {
            Style::default()
        };
        let x = area.x + layout.left_padding;
        let width = area
            .width
            .saturating_sub(layout.left_padding)
            .saturating_sub(layout.right_padding);
        let hint = if layout.console_tier == Some(ConsoleTier::Full) {
            WIDE_FOOTER_HINT
        } else {
            FOOTER_HINT
        };
        render_text(f, x, layout.footer_y, width, hint, style);
    }

    fn draw_release_console(
        &self,
        f: &mut Frame,
        area: Rect,
        layout: &WelcomeLayout,
        theme: &Theme,
        mode: RenderMode,
    ) {
        let Some(tier) = layout.console_tier else {
            return;
        };
        if layout.console_height == 0 || layout.console_y >= area.y + area.height {
            return;
        }
        let items = whats_new_highlights();
        if items.is_empty() {
            return;
        }

        let version = env!("CARGO_PKG_VERSION");
        let rect = Rect::new(
            layout.console_x,
            layout.console_y,
            layout.console_width,
            layout.console_height,
        );
        let title = format!("Release Console v{version}");
        let block = if mode.styled {
            theme
                .styled_block(&title, false)
                .title_bottom(Line::from(" Press [n] for full release notes ").centered())
        } else {
            Block::default()
                .borders(Borders::ALL)
                .title(title)
                .title_bottom(Line::from(" Press [n] for full release notes ").centered())
        };
        let inner = block.inner(rect);
        f.render_widget(block, rect);

        if inner.height < 4 || inner.width < 32 {
            return;
        }

        let [summary_height, network_height, charts_height] =
            console_row_heights(tier, inner.height);
        let rows = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(summary_height),
                Constraint::Length(network_height),
                Constraint::Length(charts_height),
                Constraint::Min(3),
            ])
            .split(inner);

        let summary = release_summary_line(theme, mode);
        f.render_widget(Paragraph::new(summary), rows[0]);

        if tier == ConsoleTier::Full {
            draw_internet_monitor(
                f,
                rows[1],
                theme,
                mode,
                self.network_measure_state(),
                self.network_peak_rates(),
            );
        }

        if tier != ConsoleTier::Small {
            let charts = Layout::default()
                .direction(Direction::Horizontal)
                .constraints([Constraint::Percentage(45), Constraint::Percentage(55)])
                .split(rows[2]);
            draw_release_mix(f, charts[0], theme, mode);
            draw_pr_trace(f, charts[1], theme, mode);
        }

        // Small has no room for a boxed panel: unboxed, all highlights fit.
        let lines = whats_new_lines(&items, rows[3].width as usize, theme, mode);
        let highlights = Paragraph::new(lines);
        let highlights = if tier == ConsoleTier::Small {
            highlights
        } else {
            highlights.block(chart_block("Highlights", theme, mode))
        };
        f.render_widget(highlights, rows[3]);
    }
}

fn draw_internet_monitor(
    f: &mut Frame,
    area: Rect,
    theme: &Theme,
    mode: RenderMode,
    state: NetworkMeasureState,
    peak_rates: Option<NetworkRates>,
) {
    if area.width < 40 || area.height < 4 {
        return;
    }

    let chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(68), Constraint::Percentage(32)])
        .split(area);

    draw_network_canvas(f, chunks[0], theme, mode, state);
    draw_network_stats(f, chunks[1], theme, mode, state, peak_rates);
}

fn draw_network_canvas(
    f: &mut Frame,
    area: Rect,
    theme: &Theme,
    mode: RenderMode,
    state: NetworkMeasureState,
) {
    const UPLOAD: [f64; 36] = [
        5.5, 6.8, 5.9, 4.4, 3.2, 4.9, 6.1, 5.0, 3.8, 2.4, 1.8, 3.5, 5.4, 7.0, 6.2, 4.7, 3.1, 2.0,
        3.4, 5.8, 7.8, 8.6, 6.5, 4.2, 3.0, 4.8, 6.9, 8.2, 7.1, 5.5, 3.6, 2.6, 4.0, 5.2, 6.3, 4.9,
    ];
    const DOWNLOAD: [f64; 36] = [
        2.0, 2.6, 3.4, 5.8, 8.2, 6.9, 4.2, 2.2, 1.8, 2.6, 3.0, 4.5, 5.2, 3.8, 2.4, 2.0, 3.6, 6.8,
        9.1, 7.0, 4.6, 2.7, 2.1, 3.5, 5.9, 8.8, 9.8, 7.4, 5.1, 2.5, 1.9, 2.8, 4.1, 6.0, 7.2, 4.4,
    ];

    let up_color = if mode.styled {
        theme.accent_info
    } else {
        Color::Reset
    };
    let down_color = if mode.styled {
        theme.accent_warning
    } else {
        Color::Reset
    };
    let mid_color = if mode.styled {
        theme.border_inactive
    } else {
        Color::Reset
    };

    let canvas = Canvas::default()
        .block(chart_block("Internet", theme, mode))
        .marker(Marker::Braille)
        .x_bounds([0.0, (UPLOAD.len() - 1) as f64])
        .y_bounds([-10.0, 10.0])
        .paint(move |ctx| {
            ctx.draw(&CanvasLine {
                x1: 0.0,
                y1: 0.0,
                x2: (UPLOAD.len() - 1) as f64,
                y2: 0.0,
                color: mid_color,
            });
            if let Some(tick) = network_sample_tick(state) {
                for idx in 0..UPLOAD.len() {
                    let value = shifted_sample(&UPLOAD, idx, tick, 0.45);
                    let x = idx as f64;
                    ctx.draw(&CanvasLine {
                        x1: x,
                        y1: 0.0,
                        x2: x,
                        y2: value,
                        color: up_color,
                    });
                }
                for idx in 0..DOWNLOAD.len() {
                    let value = shifted_sample(&DOWNLOAD, idx, tick + 9, 0.35);
                    let x = idx as f64;
                    ctx.draw(&CanvasLine {
                        x1: x,
                        y1: 0.0,
                        x2: x,
                        y2: -value,
                        color: down_color,
                    });
                }
            }
        });
    f.render_widget(canvas, area);
}

fn shifted_sample(samples: &[f64], idx: usize, tick: usize, pulse: f64) -> f64 {
    let source = samples[(idx + tick / 2) % samples.len()];
    let phase = ((idx + tick) % 6) as f64;
    (source + phase * pulse).min(10.0)
}

fn draw_network_stats(
    f: &mut Frame,
    area: Rect,
    theme: &Theme,
    mode: RenderMode,
    state: NetworkMeasureState,
    peak_rates: Option<NetworkRates>,
) {
    let Some(tick) = network_sample_tick(state) else {
        let lines = if mode.styled {
            vec![
                Line::from(vec![
                    Span::styled("state ", Style::default().fg(theme.text_secondary)),
                    Span::styled(
                        "standby",
                        Style::default()
                            .fg(theme.text_primary)
                            .add_modifier(Modifier::BOLD),
                    ),
                ]),
                Line::from(vec![
                    Span::styled("press ", Style::default().fg(theme.text_secondary)),
                    Span::styled("[w]", Style::default().fg(theme.accent_warning)),
                    Span::styled(" measure", Style::default().fg(theme.text_primary)),
                ]),
                Line::from(vec![
                    Span::styled("mode  ", Style::default().fg(theme.text_secondary)),
                    Span::styled("manual", Style::default().fg(theme.text_primary)),
                ]),
                Line::from(vec![
                    Span::styled("sample ", Style::default().fg(theme.text_secondary)),
                    Span::styled("paused", Style::default().fg(theme.text_primary)),
                ]),
            ]
        } else {
            vec![
                Line::from("state standby"),
                Line::from("press [w] measure"),
                Line::from("mode  manual"),
                Line::from("sample paused"),
            ]
        };
        f.render_widget(
            Paragraph::new(lines).block(chart_block("Link", theme, mode)),
            area,
        );
        return;
    };

    let current_rates = network_rates(tick);
    let peak_rates = peak_rates.unwrap_or(current_rates);
    let down_label = format!("{:.2} KiB/s", current_rates.down_kib_s);
    let up_label = format!("{} Byte/s", current_rates.up_bytes_s);
    let top_down_label = format!("{:.2} KiB/s", peak_rates.down_kib_s);
    let top_up_label = format!("{} Byte/s", peak_rates.up_bytes_s);
    let state_label = match state {
        NetworkMeasureState::Measuring { .. } => "measuring",
        NetworkMeasureState::Last { .. } => "last sample",
        NetworkMeasureState::Standby => "standby",
    };
    let sample_label = match state {
        NetworkMeasureState::Measuring { .. } => "live",
        NetworkMeasureState::Last { .. } => "held",
        NetworkMeasureState::Standby => "paused",
    };
    let lines = if mode.styled {
        vec![
            Line::from(vec![
                Span::styled("state ", Style::default().fg(theme.text_secondary)),
                Span::styled(
                    state_label,
                    Style::default()
                        .fg(theme.text_primary)
                        .add_modifier(Modifier::BOLD),
                ),
            ]),
            Line::from(vec![
                Span::styled("▼ down ", Style::default().fg(theme.accent_info)),
                Span::styled(
                    down_label,
                    Style::default()
                        .fg(theme.text_primary)
                        .add_modifier(Modifier::BOLD),
                ),
            ]),
            Line::from(vec![
                Span::styled("  top  ", Style::default().fg(theme.text_secondary)),
                Span::styled(top_down_label, Style::default().fg(theme.text_primary)),
            ]),
            Line::from(vec![
                Span::styled("▲ up   ", Style::default().fg(theme.accent_warning)),
                Span::styled(
                    up_label,
                    Style::default()
                        .fg(theme.text_primary)
                        .add_modifier(Modifier::BOLD),
                ),
            ]),
            Line::from(vec![
                Span::styled("  top  ", Style::default().fg(theme.text_secondary)),
                Span::styled(top_up_label, Style::default().fg(theme.text_primary)),
            ]),
            Line::from(vec![
                Span::styled("sample ", Style::default().fg(theme.text_secondary)),
                Span::styled(sample_label, Style::default().fg(theme.text_primary)),
            ]),
        ]
    } else {
        vec![
            Line::from(format!("state {state_label}")),
            Line::from(format!("v down {down_label}")),
            Line::from(format!("  top  {top_down_label}")),
            Line::from(format!("^ up   {up_label}")),
            Line::from(format!("  top  {top_up_label}")),
            Line::from(format!("sample {sample_label}")),
        ]
    };

    f.render_widget(
        Paragraph::new(lines).block(chart_block("Link", theme, mode)),
        area,
    );
}

fn network_sample_tick(state: NetworkMeasureState) -> Option<usize> {
    match state {
        NetworkMeasureState::Standby => None,
        NetworkMeasureState::Measuring { tick } | NetworkMeasureState::Last { tick } => Some(tick),
    }
}

fn draw_release_mix(f: &mut Frame, area: Rect, theme: &Theme, mode: RenderMode) {
    let data = release_category_counts();
    let block = chart_block("Mix", theme, mode);
    let style = if mode.styled {
        Style::default()
            .fg(theme.accent_success)
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default()
    };
    let value_style = if mode.styled {
        Style::default()
            .fg(theme.branding_fg)
            .bg(theme.accent_success)
    } else {
        Style::default()
    };
    let label_style = if mode.styled {
        Style::default().fg(theme.text_secondary)
    } else {
        Style::default()
    };

    let chart = BarChart::default()
        .block(block)
        .data(&data)
        .bar_width(3)
        .bar_gap(1)
        .bar_style(style)
        .value_style(value_style)
        .label_style(label_style);
    f.render_widget(chart, area);
}

fn draw_pr_trace(f: &mut Frame, area: Rect, theme: &Theme, mode: RenderMode) {
    let data = release_ref_trend();
    let max = data.iter().copied().max().unwrap_or(1).max(1);
    let block = chart_block("Ref Trend", theme, mode);
    let style = if mode.styled {
        Style::default()
            .fg(theme.accent_warning)
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default()
    };

    let sparkline = Sparkline::default()
        .block(block)
        .data(&data)
        .max(max)
        .style(style);
    f.render_widget(sparkline, area);
}

fn chart_block<'a>(title: &'a str, theme: &Theme, mode: RenderMode) -> Block<'a> {
    let block = Block::default().borders(Borders::ALL).title(title);
    if mode.styled {
        block
            .border_style(Style::default().fg(theme.border_inactive))
            .title_style(Style::default().fg(theme.title_accent))
    } else {
        block
    }
}

fn release_summary_line(theme: &Theme, mode: RenderMode) -> Line<'static> {
    let Some(entry) = changelog::current_version() else {
        return Line::from(" release data unavailable ");
    };

    let changes = entry
        .sections
        .iter()
        .map(|section| section.items.len())
        .sum::<usize>();
    let refs = entry
        .sections
        .iter()
        .flat_map(|section| section.items.iter())
        .flat_map(|item| item.issue_numbers.iter())
        .count();
    let categories = entry
        .sections
        .iter()
        .filter(|section| !section.items.is_empty())
        .count();

    let date = entry.date.as_deref().unwrap_or("unreleased");
    if !mode.styled {
        return Line::from(format!(
            " changes: {changes}   refs: {refs}   categories: {categories}   date: {date}"
        ));
    }

    Line::from(vec![
        Span::styled(" changes ", Style::default().fg(theme.text_secondary)),
        Span::styled(
            changes.to_string(),
            Style::default()
                .fg(theme.accent_success)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled("   refs ", Style::default().fg(theme.text_secondary)),
        Span::styled(
            refs.to_string(),
            Style::default()
                .fg(theme.accent_warning)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled("   categories ", Style::default().fg(theme.text_secondary)),
        Span::styled(
            categories.to_string(),
            Style::default()
                .fg(theme.accent_info)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled("   date ", Style::default().fg(theme.text_secondary)),
        Span::styled(date.to_string(), Style::default().fg(theme.text_primary)),
    ])
}

fn release_category_counts() -> [(&'static str, u64); 5] {
    let mut counts = [0_u64; 5];
    if let Some(entry) = changelog::current_version() {
        for section in &entry.sections {
            let idx = match section.category {
                ChangeCategory::Added => Some(0),
                ChangeCategory::Fixed => Some(1),
                ChangeCategory::Changed => Some(2),
                ChangeCategory::Performance => Some(3),
                ChangeCategory::Documentation | ChangeCategory::Testing => Some(4),
                _ => None,
            };
            if let Some(idx) = idx {
                counts[idx] += section.items.len() as u64;
            }
        }
    }

    [
        ("Add", counts[0]),
        ("Fix", counts[1]),
        ("Chg", counts[2]),
        ("Perf", counts[3]),
        ("Docs", counts[4]),
    ]
}

fn release_ref_trend() -> Vec<u64> {
    let mut points: Vec<u64> = changelog::changelog()
        .entries
        .iter()
        .take(24)
        .map(|entry| {
            entry
                .sections
                .iter()
                .flat_map(|section| section.items.iter())
                .map(|item| item.issue_numbers.len() as u64)
                .sum::<u64>()
        })
        .collect();

    points.reverse();
    if points.iter().any(|count| *count > 0) {
        return points;
    }

    let mut item_counts: Vec<u64> = changelog::changelog()
        .entries
        .iter()
        .take(24)
        .map(|entry| {
            entry
                .sections
                .iter()
                .map(|section| section.items.len() as u64)
                .sum::<u64>()
        })
        .collect();
    item_counts.reverse();
    if item_counts.is_empty() {
        item_counts.push(1);
    }
    item_counts
}

fn whats_new_highlights() -> Vec<(ChangeCategory, &'static ChangeItem)> {
    let version = env!("CARGO_PKG_VERSION");
    changelog::changelog().highlights_with_category(version, 4)
}

fn whats_new_lines<'a>(
    items: &[(ChangeCategory, &'static ChangeItem)],
    width: usize,
    theme: &Theme,
    mode: RenderMode,
) -> Vec<Line<'a>> {
    items
        .iter()
        .map(|(cat, item)| {
            let prefix = format!("  [{}] ", cat.label());
            let max_text = width.saturating_sub(prefix.width());
            let text = ellipsize(&item.text, max_text);
            if mode.styled {
                Line::from(vec![
                    Span::styled(
                        prefix,
                        Style::default()
                            .fg(theme.accent_success)
                            .add_modifier(Modifier::BOLD),
                    ),
                    Span::styled(text, Style::default().fg(theme.text_primary)),
                ])
            } else {
                Line::from(format!("{prefix}{text}"))
            }
        })
        .collect()
}

fn menu_rows(columns: u16) -> u16 {
    let columns = columns.max(1) as usize;
    MENU_ITEMS.len().div_ceil(columns) as u16
}

fn menu_block_width(columns: u16) -> u16 {
    let item_width = MENU_ITEMS
        .iter()
        .map(|item| menu_item_width(item.shortcut, item.label))
        .max()
        .unwrap_or(1) as u16;
    if columns > 1 {
        item_width * columns + COLUMN_GAP
    } else {
        item_width
    }
}

fn menu_item_width(shortcut: char, label: &str) -> usize {
    format!("[{}]{}{}", shortcut, MENU_KEY_LABEL_GAP, label).width()
}

fn column_width_for_layout(layout: &WelcomeLayout) -> u16 {
    if layout.columns > 1 {
        layout.menu_width.saturating_sub(COLUMN_GAP) / 2
    } else {
        layout.menu_width
    }
}

fn header_for_width(use_nerd_font: bool, width: u16) -> &'static [&'static str] {
    let nerd_header_width = NERD_HEADER
        .iter()
        .map(|line| line.width())
        .max()
        .unwrap_or(0) as u16;
    if use_nerd_font && nerd_header_width <= width {
        NERD_HEADER
    } else {
        ASCII_HEADER
    }
}

fn should_emit_styles() -> bool {
    should_emit_styles_from_env(
        |key| std::env::var(key).ok(),
        std::env::var_os("NO_COLOR").is_some(),
    )
}

fn should_emit_styles_from_env(
    get_env: impl Fn(&str) -> Option<String>,
    no_color_present: bool,
) -> bool {
    if no_color_present {
        return false;
    }
    get_env("TERM").is_none_or(|term| term != "dumb")
}

fn render_menu_item(f: &mut Frame, item: MenuItemRender<'_>, theme: &Theme, mode: RenderMode) {
    let key = format!("[{}]", item.shortcut);
    let label_width = item
        .width
        .saturating_sub(key.width() as u16)
        .saturating_sub(MENU_KEY_LABEL_GAP.width() as u16) as usize;
    let label = ellipsize(item.label, label_width);
    let selected_style = if mode.styled {
        Style::default()
            .fg(theme.selection_fg)
            .bg(theme.selection_bg)
            .add_modifier(Modifier::BOLD | Modifier::REVERSED)
    } else {
        Style::default()
    };
    let key_style = if !mode.styled {
        Style::default()
    } else if item.selected {
        selected_style
    } else {
        Style::default().fg(theme.text_secondary)
    };
    let label_style = if !mode.styled {
        Style::default()
    } else if item.selected {
        selected_style
    } else {
        Style::default().fg(theme.text_primary)
    };

    let line = Line::from(vec![
        Span::styled(key, key_style),
        Span::styled(MENU_KEY_LABEL_GAP, label_style),
        Span::styled(label, label_style),
    ]);
    let rect = Rect::new(item.x, item.y, item.width, 1);
    f.render_widget(Paragraph::new(line), rect);
}

fn render_centered_text(f: &mut Frame, area: Rect, y: u16, text: &str, style: Style) {
    if y >= area.y + area.height {
        return;
    }
    let text_width = text.width() as u16;
    let x = area.x + area.width.saturating_sub(text_width) / 2;
    let width = text_width.min(area.width);
    render_text(f, x, y, width, text, style);
}

fn render_text(f: &mut Frame, x: u16, y: u16, width: u16, text: &str, style: Style) {
    if width == 0 {
        return;
    }
    let text = ellipsize(text, width as usize);
    let rect = Rect::new(x, y, width, 1);
    f.render_widget(Paragraph::new(Line::from(Span::styled(text, style))), rect);
}

// RED stub (#1224): replaced in GREEN.
#[allow(dead_code)]
fn release_mix_text(_width: usize) -> String {
    String::new()
}

pub(crate) fn ellipsize(text: &str, max_width: usize) -> String {
    if text.width() <= max_width {
        return text.to_string();
    }
    if max_width == 0 {
        return String::new();
    }
    if max_width == 1 {
        return "…".to_string();
    }

    let mut out = String::new();
    let mut width = 0;
    for ch in text.chars() {
        let ch_width = ch.width().unwrap_or(0);
        if width + ch_width > max_width - 1 {
            break;
        }
        out.push(ch);
        width += ch_width;
    }
    out.push('…');
    out
}

#[cfg(test)]
#[path = "draw_tests.rs"]
mod tests;
