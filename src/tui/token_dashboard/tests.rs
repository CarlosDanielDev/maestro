use super::*;

#[test]
fn with_thousands_handles_small_numbers() {
    assert_eq!(with_thousands(0), "0");
    assert_eq!(with_thousands(999), "999");
}

#[test]
fn with_thousands_inserts_separator_at_thousands() {
    assert_eq!(with_thousands(1_000), "1,000");
    assert_eq!(with_thousands(12_304), "12,304");
    assert_eq!(with_thousands(200_000), "200,000");
    assert_eq!(with_thousands(204_800), "204,800");
    assert_eq!(with_thousands(1_234_567), "1,234,567");
}

#[test]
fn no_quota_snapshots_returns_none_for_any_provider() {
    let nq = NoQuotaSnapshots;
    assert!(nq.quota_for("minimax").is_none());
    assert!(nq.quota_for("claude").is_none());
    assert!(nq.quota_for("unknown").is_none());
}

#[test]
fn format_context_cell_wide_with_window() {
    assert_eq!(
        format_context_cell(12_304, Some(200_000), false),
        "ctx 12,304/200,000"
    );
}

#[test]
fn format_context_cell_wide_without_window() {
    assert_eq!(format_context_cell(15_000, None, false), "ctx 15,000");
}

#[test]
fn format_quota_cell_wide_with_quota() {
    let row = QuotaRow {
        used: 247,
        limit: 4500,
        window_label: "5h",
        status: QuotaBucket::Warn,
    };
    assert_eq!(format_quota_cell(Some(row), false), "quota 247/4500 (5h)");
}

#[test]
fn format_quota_cell_wide_none_renders_dash() {
    assert_eq!(format_quota_cell(None, false), "-");
}
