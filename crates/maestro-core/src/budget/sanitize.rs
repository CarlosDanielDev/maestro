//! Value-clamp primitives shared between the budget projector and the TUI
//! provider rollup view-model (#776/#848/#854).
//!
//! Pure functions — no I/O, no allocation. The render layer never receives
//! NaN / Infinity / negative cost or out-of-range percent values because the
//! view-model assembly layer (`build_provider_rows`) and the projection
//! layer (`DefaultBudgetProjector::projected_turn_cost`) both sanitize at
//! their boundaries via these helpers.

/// Clamp cost values to a sane range: NaN / Infinity / negative → 0.0.
pub(crate) fn sanitize_cost(cost: f64) -> f64 {
    if !cost.is_finite() || cost.is_sign_negative() {
        0.0
    } else {
        cost
    }
}

/// Clamp percent values to `[0.0, 1.0]`. NaN / negative → 0.0; >1.0 → 1.0.
pub(crate) fn sanitize_pct(pct: f64) -> f64 {
    if pct.is_nan() || pct < 0.0 {
        0.0
    } else if pct > 1.0 {
        1.0
    } else {
        pct
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sanitize_cost_nan_returns_zero() {
        assert_eq!(sanitize_cost(f64::NAN), 0.0);
    }

    #[test]
    fn sanitize_cost_negative_returns_zero() {
        assert_eq!(sanitize_cost(-1.5), 0.0);
    }

    #[test]
    fn sanitize_cost_normal_passes_through() {
        assert_eq!(sanitize_cost(0.42), 0.42);
    }

    #[test]
    fn sanitize_cost_zero_passes_through() {
        assert_eq!(sanitize_cost(0.0), 0.0);
    }

    #[test]
    fn sanitize_cost_infinity_returns_zero() {
        assert_eq!(sanitize_cost(f64::INFINITY), 0.0);
    }

    #[test]
    fn sanitize_cost_negative_zero_returns_positive_zero() {
        // -0.0 is sign-negative in IEEE 754; sanitize_cost must return +0.0
        // so cost rendering never shows `$-0.00`.
        let result = sanitize_cost(-0.0_f64);
        assert_eq!(result, 0.0);
        assert!(!result.is_sign_negative(), "expected +0.0, got -0.0");
    }

    #[test]
    fn sanitize_pct_nan_returns_zero() {
        assert_eq!(sanitize_pct(f64::NAN), 0.0);
    }

    #[test]
    fn sanitize_pct_negative_returns_zero() {
        assert_eq!(sanitize_pct(-0.5), 0.0);
    }

    #[test]
    fn sanitize_pct_above_one_clamps_to_one() {
        assert_eq!(sanitize_pct(1.5), 1.0);
    }

    #[test]
    fn sanitize_pct_normal_passes_through() {
        assert_eq!(sanitize_pct(0.75), 0.75);
    }

    #[test]
    fn sanitize_pct_exactly_one_passes_through() {
        assert_eq!(sanitize_pct(1.0), 1.0);
    }
}
