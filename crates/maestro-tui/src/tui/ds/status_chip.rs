//! `ds::status_chip` (#1251): one chip per `SessionStatus` — glyph, word
//! and colour together, so the status reads without colour (NO_COLOR) and
//! without Nerd Fonts (ASCII icons). Glyphs and words follow the StatusChip
//! table in `docs/design-system/index.html` §04.

use ratatui::style::{Color, Modifier, Style};
use ratatui::text::Span;

use crate::session::types::SessionStatus;
use crate::tui::icons::{self, IconId};
use crate::tui::theme::Theme;

/// The chip for `status` in the current icon mode (`tui.ascii_icons` /
/// `MAESTRO_ASCII_ICONS=1`).
pub fn status_chip(status: SessionStatus, theme: &Theme) -> Span<'static> {
    status_chip_for_mode(status, theme, icons::use_nerd_font())
}

/// Pure variant of [`status_chip`]: pass the icon mode explicitly.
/// Statuses that need a human (`NeedsReview`, `FailedGates`, `Errored`)
/// are bold too.
pub fn status_chip_for_mode(
    status: SessionStatus,
    theme: &Theme,
    nerd_font: bool,
) -> Span<'static> {
    let glyph = icons::get_for_mode(chip_icon(status), nerd_font);
    let mut style = Style::new().fg(chip_color(status, theme));
    if matches!(
        status,
        SessionStatus::NeedsReview | SessionStatus::FailedGates | SessionStatus::Errored
    ) {
        style = style.add_modifier(Modifier::BOLD);
    }
    Span::styled(format!("{glyph} {}", status_word(status)), style)
}

/// The chip colour: the design's v1 tokens for `NeedsPr` / `ConflictFix`
/// (#1353), `theme.status_color` for the rest. `status_color` still maps
/// those two to `status_stalled` / `status_ci_fix` for the older screens.
fn chip_color(status: SessionStatus, theme: &Theme) -> Color {
    match status {
        SessionStatus::NeedsPr => theme.status_needs_pr,
        SessionStatus::ConflictFix => theme.status_conflict,
        _ => theme.status_color(status),
    }
}

/// The chip word for `status`, lower case, as people say it.
pub const fn status_word(status: SessionStatus) -> &'static str {
    match status {
        SessionStatus::Queued => "queued",
        SessionStatus::Spawning => "spawning",
        SessionStatus::Running => "running",
        SessionStatus::GatesRunning => "gates running",
        SessionStatus::NeedsReview => "needs you",
        SessionStatus::NeedsPr => "PR ready",
        SessionStatus::CiFix => "CI fix",
        SessionStatus::ConflictFix => "conflict fix",
        SessionStatus::Retrying => "retrying",
        SessionStatus::Stalled => "stalled",
        SessionStatus::Paused => "paused",
        SessionStatus::Completed => "completed",
        SessionStatus::FailedGates => "gates failed",
        SessionStatus::Errored => "errored",
        SessionStatus::Killed => "killed",
        SessionStatus::Interactive => "interactive",
    }
}

const fn chip_icon(status: SessionStatus) -> IconId {
    match status {
        SessionStatus::Queued => IconId::ChipQueued,
        SessionStatus::Spawning => IconId::ChipSpawning,
        SessionStatus::Running => IconId::ChipRunning,
        SessionStatus::GatesRunning => IconId::ChipGatesRunning,
        SessionStatus::NeedsReview => IconId::ChipNeedsReview,
        SessionStatus::NeedsPr => IconId::ChipNeedsPr,
        SessionStatus::CiFix => IconId::ChipCiFix,
        SessionStatus::ConflictFix => IconId::ChipConflictFix,
        SessionStatus::Retrying => IconId::ChipRetrying,
        SessionStatus::Stalled => IconId::ChipStalled,
        SessionStatus::Paused => IconId::ChipPaused,
        SessionStatus::Completed => IconId::ChipCompleted,
        SessionStatus::FailedGates => IconId::ChipFailedGates,
        SessionStatus::Errored => IconId::ChipErrored,
        SessionStatus::Killed => IconId::ChipKilled,
        SessionStatus::Interactive => IconId::ChipInteractive,
    }
}

/// Every status, for tests. Update it when `SessionStatus` grows
/// (`chip_icon` and `status_word` stop compiling first).
#[cfg(test)]
pub(crate) const ALL_STATUSES: [SessionStatus; 16] = [
    SessionStatus::Queued,
    SessionStatus::Spawning,
    SessionStatus::Running,
    SessionStatus::GatesRunning,
    SessionStatus::NeedsReview,
    SessionStatus::NeedsPr,
    SessionStatus::CiFix,
    SessionStatus::ConflictFix,
    SessionStatus::Retrying,
    SessionStatus::Stalled,
    SessionStatus::Paused,
    SessionStatus::Completed,
    SessionStatus::FailedGates,
    SessionStatus::Errored,
    SessionStatus::Killed,
    SessionStatus::Interactive,
];

#[cfg(test)]
mod tests {
    use std::collections::HashSet;

    use super::*;

    /// Splits a chip into `(glyph, word)`. Both may hold spaces (`[ ]`,
    /// `gates running`), so split on the known word, not on a space.
    fn glyph_and_word(status: SessionStatus, nerd_font: bool) -> (String, String) {
        let chip = status_chip_for_mode(status, &Theme::dark(), nerd_font);
        let word = status_word(status);
        let glyph = chip
            .content
            .strip_suffix(&format!(" {word}"))
            .unwrap_or_else(|| panic!("{status:?}: chip must be `glyph {word}`, got {chip:?}"));
        (glyph.to_string(), word.to_string())
    }

    #[test]
    fn status_chip_all_variants_distinct() {
        for nerd_font in [true, false] {
            let mut glyphs = HashSet::new();
            let mut words = HashSet::new();
            for status in ALL_STATUSES {
                let (glyph, word) = glyph_and_word(status, nerd_font);
                assert!(!glyph.is_empty(), "{status:?}: empty glyph");
                assert!(!word.is_empty(), "{status:?}: empty word");
                assert!(
                    glyphs.insert(glyph.clone()),
                    "{status:?}: glyph {glyph:?} reused (nerd_font={nerd_font})"
                );
                assert!(
                    words.insert(word.clone()),
                    "{status:?}: word {word:?} reused"
                );
            }
            assert_eq!(glyphs.len(), ALL_STATUSES.len());
        }
    }

    #[test]
    fn status_chip_ascii_fallback() {
        for status in ALL_STATUSES {
            let chip = status_chip_for_mode(status, &Theme::dark(), false);
            assert!(
                chip.content.is_ascii(),
                "{status:?}: ASCII chip has non-ASCII text {:?}",
                chip.content
            );
            let (glyph, _) = glyph_and_word(status, true);
            assert!(
                !glyph.is_ascii(),
                "{status:?}: Unicode glyph should not be ASCII"
            );
        }
    }

    /// Every preset that has colours (`no_color` maps all to `Reset`).
    fn colour_themes() -> [Theme; 4] {
        [
            Theme::dark(),
            Theme::light(),
            Theme::retro(),
            Theme::high_contrast(),
        ]
    }

    #[test]
    fn status_chip_needs_pr_uses_status_needs_pr_token() {
        for theme in colour_themes() {
            let chip = status_chip_for_mode(SessionStatus::NeedsPr, &theme, true);
            assert_eq!(chip.style.fg, Some(theme.status_needs_pr));
        }
    }

    #[test]
    fn status_chip_conflict_fix_uses_status_conflict_token() {
        for theme in colour_themes() {
            let chip = status_chip_for_mode(SessionStatus::ConflictFix, &theme, true);
            assert_eq!(chip.style.fg, Some(theme.status_conflict));
        }
    }

    #[test]
    fn status_chip_uses_status_color_and_bolds_statuses_that_need_a_human() {
        let theme = Theme::dark();
        for status in ALL_STATUSES {
            let chip = status_chip_for_mode(status, &theme, true);
            if !matches!(status, SessionStatus::NeedsPr | SessionStatus::ConflictFix) {
                assert_eq!(
                    chip.style.fg,
                    Some(theme.status_color(status)),
                    "{status:?}"
                );
            }
            let needs_human = matches!(
                status,
                SessionStatus::NeedsReview | SessionStatus::FailedGates | SessionStatus::Errored
            );
            assert_eq!(
                chip.style.add_modifier.contains(Modifier::BOLD),
                needs_human,
                "{status:?}: BOLD only for statuses that need a human"
            );
        }
    }
}
