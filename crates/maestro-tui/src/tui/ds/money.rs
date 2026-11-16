//! `ds::money` (#1251): the one dollar formatter for the TUI
//! (`docs/design-system/index.html` §02.14). Never prints `$-0.00` or
//! `$0.0000`.

use crate::budget::sanitize::sanitize_cost;

/// `$12.40`, `$0.00`, `<$0.01` for a positive amount under one cent, and
/// `$?` when the cost is unknown (`None`: unpriced model). NaN, infinity,
/// negatives and `-0.0` print as `$0.00`.
pub fn money(usd: Option<f64>) -> String {
    let Some(usd) = usd else {
        return "$?".to_string();
    };
    let usd = sanitize_cost(usd);
    if usd > 0.0 && usd < 0.01 {
        "<$0.01".to_string()
    } else {
        format!("${usd:.2}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn money_formats() {
        let cases = [
            (Some(0.0), "$0.00"),
            (Some(-0.0), "$0.00"),
            (Some(f64::NAN), "$0.00"),
            (Some(f64::INFINITY), "$0.00"),
            (Some(-3.5), "$0.00"),
            (Some(0.004), "<$0.01"),
            (Some(0.01), "$0.01"),
            (Some(12.4), "$12.40"),
            (None, "$?"),
        ];
        for (usd, expected) in cases {
            assert_eq!(money(usd), expected, "{usd:?}");
        }
    }
}
