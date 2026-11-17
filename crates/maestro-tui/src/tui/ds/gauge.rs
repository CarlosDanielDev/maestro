//! `ds::gauge` (#1251): the canonical threshold set for spend and quota
//! gauges — below 60% low, 60–80% medium, 80–100% high, at or over 100% critical
//! (`docs/design-system/index.html` §04 Gauge). Context gauges keep their
//! own 50%/70% thresholds, which match the overflow actions in
//! `context_overflow.rs` (see `docs/design-system/decisions.md`). Older
//! helpers in `theme.rs` (`budget_color`, `gauge_color`) move here in the
//! screen issues.

use ratatui::style::{Modifier, Style};

use crate::tui::icons::{IconId, get_for_mode};
use crate::tui::theme::Theme;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GaugeLevel {
    Low,
    Medium,
    High,
    Critical,
}

/// Level for a used/limit `ratio` (1.0 = the cap). NaN and negatives are low.
pub fn gauge_level(ratio: f64) -> GaugeLevel {
    if ratio >= 1.0 {
        GaugeLevel::Critical
    } else if ratio >= 0.8 {
        GaugeLevel::High
    } else if ratio >= 0.6 {
        GaugeLevel::Medium
    } else {
        GaugeLevel::Low
    }
}

impl GaugeLevel {
    /// Bar and label style: low / medium / high colour; critical is high + BOLD.
    pub fn style(self, theme: &Theme) -> Style {
        match self {
            GaugeLevel::Low => Style::new().fg(theme.gauge_low),
            GaugeLevel::Medium => Style::new().fg(theme.gauge_medium),
            GaugeLevel::High => Style::new().fg(theme.gauge_high),
            GaugeLevel::Critical => Style::new()
                .fg(theme.gauge_high)
                .add_modifier(Modifier::BOLD),
        }
    }
}

/// `(filled, empty)` cells of a `width`-cell bar for a used/limit `ratio`
/// (#1264), from `IconId::GaugeFilled` / `GaugeEmpty` (`▓░`, ASCII `#-`).
/// The ratio is clamped to `0..=1`; NaN is empty.
pub fn bar(ratio: f64, width: usize, nerd_font: bool) -> (String, String) {
    let ratio = if ratio.is_nan() {
        0.0
    } else {
        ratio.clamp(0.0, 1.0)
    };
    let filled = ((ratio * width as f64).round() as usize).min(width);
    (
        get_for_mode(IconId::GaugeFilled, nerd_font).repeat(filled),
        get_for_mode(IconId::GaugeEmpty, nerd_font).repeat(width - filled),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bar_fills_by_ratio_and_clamps() {
        let cases = [
            (0.0, 10, 0),
            (0.5, 10, 5),
            (0.64, 10, 6),
            (1.0, 4, 4),
            (2.5, 4, 4),
            (-1.0, 4, 0),
            (f64::NAN, 4, 0),
        ];
        for (ratio, width, filled) in cases {
            let (f, e) = bar(ratio, width, false);
            assert_eq!(f, "#".repeat(filled), "ratio {ratio}");
            assert_eq!(e, "-".repeat(width - filled), "ratio {ratio}");
        }
        assert_eq!(bar(0.5, 2, true), ("\u{2593}".into(), "\u{2591}".into()));
    }

    #[test]
    fn gauge_thresholds() {
        let cases = [
            (0.0, GaugeLevel::Low),
            (0.59, GaugeLevel::Low),
            (0.6, GaugeLevel::Medium),
            (0.79, GaugeLevel::Medium),
            (0.8, GaugeLevel::High),
            (0.99, GaugeLevel::High),
            (1.0, GaugeLevel::Critical),
            (2.5, GaugeLevel::Critical),
            (f64::INFINITY, GaugeLevel::Critical),
            (-0.1, GaugeLevel::Low),
            (f64::NAN, GaugeLevel::Low),
        ];
        for (ratio, expected) in cases {
            assert_eq!(gauge_level(ratio), expected, "ratio {ratio}");
        }
    }

    #[test]
    fn gauge_level_style_maps_to_gauge_tokens() {
        let t = Theme::dark();
        assert_eq!(GaugeLevel::Low.style(&t), Style::new().fg(t.gauge_low));
        assert_eq!(
            GaugeLevel::Medium.style(&t),
            Style::new().fg(t.gauge_medium)
        );
        assert_eq!(GaugeLevel::High.style(&t), Style::new().fg(t.gauge_high));
        assert_eq!(
            GaugeLevel::Critical.style(&t),
            Style::new().fg(t.gauge_high).add_modifier(Modifier::BOLD)
        );
    }
}
