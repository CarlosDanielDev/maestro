//! #1289: no Usage view prints a dollar amount outside `ds::money`, also
//! for NaN, negative and negative-zero costs.

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::{Terminal, backend::TestBackend};

use super::usage::{empty_app, session, tokens};
use crate::session::types::SessionStatus;
use crate::tui::input_handler::handle_key;

fn text(t: &Terminal<TestBackend>) -> String {
    let buf = t.backend().buffer();
    (0..buf.area.height)
        .map(|y| {
            (0..buf.area.width)
                .map(|x| buf[(x, y)].symbol())
                .collect::<String>()
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// Every `$` in `out` is `ds::money` output: `$12.40`, `<$0.01`, `$?`.
fn assert_money_only(out: &str) {
    let chars: Vec<char> = out.chars().collect();
    for (i, _) in chars.iter().enumerate().filter(|(_, c)| **c == '$') {
        if chars.get(i + 1) == Some(&'/') {
            continue; // a unit (`$/MTok`), not an amount
        }
        let rest: String = chars[i + 1..]
            .iter()
            .take_while(|c| !c.is_whitespace())
            .collect();
        let rest = rest.trim_end_matches(|c: char| !c.is_ascii_digit() && c != '?');
        let ok = rest == "?" || {
            let (int, dec) = rest.split_once('.').unwrap_or(("", ""));
            !int.is_empty()
                && int.chars().all(|c| c.is_ascii_digit())
                && dec.len() == 2
                && dec.chars().all(|c| c.is_ascii_digit())
        };
        assert!(ok, "not money(): ${rest:?} in\n{out}");
    }
    for bad in ["NaN", "inf", "$-", "-$", "-0."] {
        assert!(!out.contains(bad), "{bad} in\n{out}");
    }
}

#[tokio::test]
async fn usage_money_formatting() {
    let mut app = empty_app();
    let k = tokens(10_000, 5_000, 1_000, 0);
    let mut nan = session(
        1,
        SessionStatus::Running,
        "claude-opus-4-5",
        f64::NAN,
        k.clone(),
    );
    nan.tq_handoff_original_tokens = Some(9_000);
    nan.tq_handoff_compressed_tokens = Some(3_000);
    let neg = session(
        2,
        SessionStatus::Running,
        "claude-opus-4-5",
        -3.5,
        k.clone(),
    );
    let zero = session(
        3,
        SessionStatus::Completed,
        "claude-opus-4-5",
        -0.0,
        k.clone(),
    );
    let cent = session(4, SessionStatus::Completed, "claude-opus-4-5", 0.004, k);
    for s in [nan, neg, zero, cent] {
        app.pool.enqueue(s);
    }
    for total in [f64::NAN, -0.0, -12.0] {
        app.total_cost = total;
        for (key, w, h) in [('c', 160, 40), ('t', 160, 40), ('c', 90, 30), ('t', 90, 30)] {
            let mut t = Terminal::new(TestBackend::new(w, h)).unwrap();
            handle_key(
                &mut app,
                KeyEvent::new(KeyCode::Char(key), KeyModifiers::NONE),
            )
            .await;
            t.draw(|f| {
                let bp = crate::tui::ds::breakpoint::Breakpoint::from_area(f.area());
                crate::tui::screens::dashboard::dispatch::sync(&mut app);
                let (view, dash) = crate::tui::screens::dashboard::dispatch::split(&mut app);
                let theme = crate::tui::theme::Theme::dark();
                crate::tui::screens::dashboard::draw::draw(
                    f,
                    f.area(),
                    &view,
                    dash,
                    bp,
                    &theme,
                    true,
                );
            })
            .unwrap();
            let out = text(&t);
            let panel = if key == 'c' {
                "Spend by session"
            } else {
                "Tokens by session"
            };
            assert!(out.contains(panel), "{key} draws a Usage view: {out}");
            assert_money_only(&out);
        }
    }
}
