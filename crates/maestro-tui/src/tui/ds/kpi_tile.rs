//! `ds::kpi_tile` (#1256): one KPI in a small panel (`kpiTile` in
//! `docs/design-system/index.html`, §04 Kit v2 and the Stats KpiTile caption).
//! Row 1: value and delta with a direction glyph. Row 2: sparkline and one
//! context line. A missing value draws its reason, never a fake `0`.

use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Paragraph, Sparkline};

use crate::tui::icons::{self, IconId};
use crate::tui::theme::Theme;

/// Total height of a tile: two content rows inside a bordered panel.
pub const KPI_TILE_HEIGHT: u16 = 4;

/// Direction of a change.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Trend {
    Up,
    Down,
    Flat,
}

/// A change against the previous window, e.g. `$1.10` going up.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct KpiDelta<'a> {
    pub text: &'a str,
    pub trend: Trend,
}

/// What the tile shows.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KpiState<'a> {
    Ready {
        value: &'a str,
        delta: Option<KpiDelta<'a>>,
        spark: &'a [u64],
        context: &'a str,
    },
    /// No value yet; `reason` says why (`no runs yet`).
    Empty { reason: &'a str },
}

/// One tile: a title on the border and its state.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct KpiTile<'a> {
    pub title: &'a str,
    pub state: KpiState<'a>,
}

/// Draw `tile` in `area` in the current icon mode.
pub fn kpi_tile(f: &mut Frame, area: Rect, tile: &KpiTile, theme: &Theme) {
    kpi_tile_for_mode(f, area, tile, theme, icons::use_nerd_font());
}

/// Pure variant of [`kpi_tile`]: pass the icon mode explicitly.
pub fn kpi_tile_for_mode(
    f: &mut Frame,
    area: Rect,
    tile: &KpiTile,
    theme: &Theme,
    nerd_font: bool,
) {
    let block = theme.styled_block(tile.title, false);
    let inner = block.inner(area);
    f.render_widget(block, area);
    if inner.width == 0 || inner.height == 0 {
        return;
    }
    let top = Rect { height: 1, ..inner };
    let muted = Style::new().fg(theme.text_muted);
    let (value, delta, spark, context) = match tile.state {
        KpiState::Empty { reason } => {
            f.render_widget(
                Paragraph::new(Span::styled(format!(" {reason}"), muted)),
                top,
            );
            return;
        }
        KpiState::Ready {
            value,
            delta,
            spark,
            context,
        } => (value, delta, spark, context),
    };

    let mut first = vec![Span::styled(
        format!(" {value}"),
        Style::new()
            .fg(theme.text_primary)
            .add_modifier(Modifier::BOLD),
    )];
    if let Some(d) = delta {
        first.push(Span::styled(
            format!("  {} {}", trend_glyph(d.trend, nerd_font), d.text),
            Style::new().fg(theme.text_primary),
        ));
    }
    f.render_widget(Paragraph::new(Line::from(first)), top);

    if inner.height < 2 {
        return;
    }
    let second = Rect {
        y: inner.y + 1,
        height: 1,
        ..inner
    };
    // Same caps as the design: 6 bars on a narrow tile, else 12.
    let cap = if inner.width < 26 { 6 } else { 12 };
    let bars = spark
        .len()
        .min(cap)
        .min(usize::from(inner.width.saturating_sub(1)));
    let spark_area = Rect {
        x: second.x + 1,
        width: bars as u16,
        ..second
    };
    f.render_widget(
        Sparkline::default()
            .data(&spark[spark.len() - bars..])
            .style(Style::new().fg(theme.accent_info)),
        spark_area,
    );
    let gap = if bars > 0 { 2 } else { 1 };
    let text_area = Rect {
        x: second.x + bars as u16 + gap,
        width: second.width.saturating_sub(bars as u16 + gap),
        ..second
    };
    f.render_widget(Paragraph::new(Span::styled(context, muted)), text_area);
}

/// The delta glyph: `↑ ↓ =`, or `^ v =` in ASCII mode.
fn trend_glyph(trend: Trend, nerd_font: bool) -> &'static str {
    match trend {
        Trend::Up => icons::get_for_mode(IconId::ArrowUp, nerd_font),
        Trend::Down => icons::get_for_mode(IconId::ArrowDown, nerd_font),
        Trend::Flat => "=",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use insta::assert_snapshot;
    use ratatui::Terminal;
    use ratatui::backend::TestBackend;
    use ratatui::layout::{Constraint, Layout};

    const SPARK: [u64; 10] = [2, 3, 2, 5, 1, 4, 6, 5, 7, 6];

    fn ready() -> KpiTile<'static> {
        KpiTile {
            title: "Spend · 30d",
            state: KpiState::Ready {
                value: "$38.42",
                delta: Some(KpiDelta {
                    text: "$1.10",
                    trend: Trend::Up,
                }),
                spark: &SPARK,
                context: "of $50 cap",
            },
        }
    }

    fn empty() -> KpiTile<'static> {
        KpiTile {
            title: "Runs / week",
            state: KpiState::Empty {
                reason: "no runs yet",
            },
        }
    }

    fn draw(tile: &KpiTile, theme: &Theme) -> Terminal<TestBackend> {
        let mut terminal = Terminal::new(TestBackend::new(30, KPI_TILE_HEIGHT)).unwrap();
        terminal
            .draw(|f| kpi_tile_for_mode(f, f.area(), tile, theme, true))
            .unwrap();
        terminal
    }

    fn row(terminal: &Terminal<TestBackend>, y: u16) -> String {
        let buf = terminal.backend().buffer();
        (0..buf.area.width)
            .map(|x| buf[(x, y)].symbol().to_string())
            .collect()
    }

    #[test]
    fn kpi_tile_ready_shows_value_delta_spark_and_context() {
        let terminal = draw(&ready(), &Theme::dark());
        let value_row = row(&terminal, 1);
        assert!(value_row.contains("$38.42  ↑ $1.10"), "{value_row:?}");
        let spark_row = row(&terminal, 2);
        assert!(spark_row.contains("of $50 cap"), "{spark_row:?}");
        assert!(
            spark_row.chars().any(|c| ('▁'..='█').contains(&c)),
            "sparkline drawn: {spark_row:?}"
        );
    }

    #[test]
    fn kpi_tile_delta_glyphs_have_ascii_fallback() {
        assert_eq!(trend_glyph(Trend::Up, true), "↑");
        assert_eq!(trend_glyph(Trend::Down, true), "↓");
        assert_eq!(trend_glyph(Trend::Flat, true), "=");
        assert_eq!(trend_glyph(Trend::Up, false), "^");
        assert_eq!(trend_glyph(Trend::Down, false), "v");
        assert_eq!(trend_glyph(Trend::Flat, false), "=");
    }

    #[test]
    fn kpi_tile_empty_shows_reason_not_zero() {
        let terminal = draw(&empty(), &Theme::dark());
        let all = format!("{}{}", row(&terminal, 1), row(&terminal, 2));
        assert!(all.contains("no runs yet"), "{all:?}");
        assert!(!all.contains('0'), "no fake zero: {all:?}");
    }

    #[test]
    fn kpi_tile_ready() {
        assert_snapshot!(draw(&ready(), &Theme::dark()).backend());
    }

    #[test]
    fn kpi_tile_empty() {
        assert_snapshot!(draw(&empty(), &Theme::dark()).backend());
    }

    #[test]
    fn kpi_tile_no_color() {
        let theme = Theme::no_color();
        let mut terminal = Terminal::new(TestBackend::new(60, KPI_TILE_HEIGHT)).unwrap();
        terminal
            .draw(|f| {
                let [a, b] = Layout::horizontal([Constraint::Fill(1); 2]).areas(f.area());
                kpi_tile_for_mode(f, a, &ready(), &theme, true);
                kpi_tile_for_mode(f, b, &empty(), &theme, true);
            })
            .unwrap();
        let out = format!("{:?}", terminal.backend().buffer());
        for colour in ["Green", "Yellow", "Red", "Cyan", "Rgb", "Indexed"] {
            assert!(!out.contains(colour), "NoColor drew {colour}: {out}");
        }
        assert_snapshot!(out);
    }
}
