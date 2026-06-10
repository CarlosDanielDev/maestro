//! Read-only in-TUI diff reviewer overlay for the Interaction screen (#918).
//!
//! Review a session's changes GitHub-PR style (diff vs
//! `merge-base(base, HEAD)`) without leaving the live chat. Rendering and
//! the motion set are adapted from gitui's diff component idea (gitui is
//! MIT-licensed, Copyright (c) 2020-2024 gitui authors) — this is a
//! from-scratch ratatui implementation of the same shape: file list pane +
//! unified diff pane + persistent hint bar, vim-like read-only motions.
//!
//! Strictly read-only: no staging, reverting, or editing. The `o` escape
//! hatch opens a shell at the worktree (run `git diff` in your own tools);
//! the overlay never mutates anything.

use ratatui::{
    Frame,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Clear, Paragraph},
};

use crate::tui::theme::Theme;

/// One rendered diff line, classified for styling and hunk navigation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum DiffLineKind {
    /// `diff --git` / `index` / `---` / `+++` headers.
    Header,
    /// `@@ -a,b +c,d @@` hunk markers — the `[`/`]` jump targets.
    Hunk,
    Add,
    Del,
    Context,
}

#[derive(Debug, Clone)]
pub(crate) struct DiffLine {
    pub kind: DiffLineKind,
    pub text: String,
}

#[derive(Debug, Clone)]
pub(crate) struct DiffFile {
    /// New-side path (`b/…` stripped); the file-list label.
    pub path: String,
    pub lines: Vec<DiffLine>,
    pub adds: usize,
    pub dels: usize,
}

/// What a key did to the overlay — the screen maps these to actions.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum DiffReviewOutcome {
    /// Overlay consumed the key.
    Handled,
    /// `Esc`/`q` — close the overlay, back to chat.
    Close,
    /// `o` — open the escape hatch (shell at the worktree).
    OpenShell,
}

/// State of the open reviewer overlay. Pure widget state — building it
/// requires the diff text only, so tests never fork git (the screen gets
/// the text through the `GitOps` seam).
pub(crate) struct DiffReview {
    pub(crate) files: Vec<DiffFile>,
    /// Selected file index (file-list pane + diff pane source).
    pub(crate) selected: usize,
    /// First visible line of the selected file's diff.
    pub(crate) scroll: usize,
    /// Diff-pane height at the last draw (drives paging).
    viewport: usize,
    /// Active search query (`/`), if any.
    query: Option<String>,
    /// In-progress query text while typing after `/`.
    query_input: Option<String>,
}

impl DiffReview {
    pub(crate) fn new(diff_text: &str) -> Self {
        Self {
            files: parse_unified_diff(diff_text),
            selected: 0,
            scroll: 0,
            viewport: 1,
            query: None,
            query_input: None,
        }
    }

    fn current_lines(&self) -> &[DiffLine] {
        self.files
            .get(self.selected)
            .map(|f| f.lines.as_slice())
            .unwrap_or(&[])
    }

    fn max_scroll(&self) -> usize {
        self.current_lines().len().saturating_sub(self.viewport)
    }

    fn clamp(&mut self) {
        self.scroll = self.scroll.min(self.max_scroll());
    }

    /// Handle one key press. Read-only motions only.
    pub(crate) fn handle_key(
        &mut self,
        code: crossterm::event::KeyCode,
        mods: crossterm::event::KeyModifiers,
    ) -> DiffReviewOutcome {
        use crossterm::event::{KeyCode, KeyModifiers};

        // Typing a search query after `/`.
        if let Some(ref mut input) = self.query_input {
            match code {
                KeyCode::Esc => self.query_input = None,
                KeyCode::Enter => {
                    let q = self.query_input.take().unwrap_or_default();
                    if q.is_empty() {
                        self.query = None;
                    } else {
                        self.query = Some(q);
                        self.jump_to_match(true, true);
                    }
                }
                KeyCode::Backspace => {
                    input.pop();
                }
                KeyCode::Char(c) => input.push(c),
                _ => {}
            }
            return DiffReviewOutcome::Handled;
        }

        let ctrl = mods.contains(KeyModifiers::CONTROL);
        match (code, ctrl) {
            (KeyCode::Esc | KeyCode::Char('q'), _) => return DiffReviewOutcome::Close,
            (KeyCode::Char('o'), false) => return DiffReviewOutcome::OpenShell,
            (KeyCode::Char('j') | KeyCode::Down, false) => {
                self.scroll = (self.scroll + 1).min(self.max_scroll());
            }
            (KeyCode::Char('k') | KeyCode::Up, false) => {
                self.scroll = self.scroll.saturating_sub(1);
            }
            (KeyCode::Char('d'), true) => {
                self.scroll = (self.scroll + self.viewport / 2).min(self.max_scroll());
            }
            (KeyCode::Char('u'), true) => {
                self.scroll = self.scroll.saturating_sub(self.viewport / 2);
            }
            (KeyCode::Char('g'), false) => self.scroll = 0,
            (KeyCode::Char('G'), false) => self.scroll = self.max_scroll(),
            (KeyCode::Char(']'), false) => self.jump_hunk(true),
            (KeyCode::Char('['), false) => self.jump_hunk(false),
            (KeyCode::Tab, _) if !self.files.is_empty() => {
                self.selected = (self.selected + 1) % self.files.len();
                self.scroll = 0;
            }
            (KeyCode::BackTab, _) if !self.files.is_empty() => {
                self.selected = (self.selected + self.files.len() - 1) % self.files.len();
                self.scroll = 0;
            }
            (KeyCode::Char('/'), false) => self.query_input = Some(String::new()),
            (KeyCode::Char('n'), false) => self.jump_to_match(true, false),
            (KeyCode::Char('N'), false) => self.jump_to_match(false, false),
            _ => {}
        }
        DiffReviewOutcome::Handled
    }

    /// Jump to the next/previous hunk marker relative to the current scroll.
    fn jump_hunk(&mut self, forward: bool) {
        let lines = self.current_lines();
        let hunks: Vec<usize> = lines
            .iter()
            .enumerate()
            .filter(|(_, l)| l.kind == DiffLineKind::Hunk)
            .map(|(i, _)| i)
            .collect();
        let target = if forward {
            hunks.into_iter().find(|&i| i > self.scroll)
        } else {
            hunks.into_iter().rev().find(|&i| i < self.scroll)
        };
        if let Some(i) = target {
            self.scroll = i;
            self.clamp();
        }
    }

    /// Jump to the next/previous line matching the active query. `from_here`
    /// includes the current line (used right after the query is entered).
    fn jump_to_match(&mut self, forward: bool, from_here: bool) {
        let Some(query) = self.query.clone() else {
            return;
        };
        let lines = self.current_lines();
        let matches: Vec<usize> = lines
            .iter()
            .enumerate()
            .filter(|(_, l)| l.text.contains(&query))
            .map(|(i, _)| i)
            .collect();
        if matches.is_empty() {
            return;
        }
        let target = if forward {
            let floor = if from_here {
                self.scroll
            } else {
                self.scroll + 1
            };
            matches
                .iter()
                .copied()
                .find(|&i| i >= floor)
                .or_else(|| matches.first().copied())
        } else {
            matches
                .iter()
                .copied()
                .rev()
                .find(|&i| i < self.scroll)
                .or_else(|| matches.last().copied())
        };
        if let Some(i) = target {
            self.scroll = i;
            self.clamp();
        }
    }

    /// Render the overlay over `area` (the full Interaction screen).
    pub(crate) fn draw(&mut self, f: &mut Frame, area: Rect, theme: &Theme) {
        let overlay = centered(area, 96, 92);
        f.render_widget(Clear, overlay);
        let block = theme
            .styled_block(" Diff review (read-only) ", true)
            .border_style(Style::default().fg(theme.accent_info));
        let inner = block.inner(overlay);
        f.render_widget(block, overlay);

        let rows = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Min(3), Constraint::Length(1)])
            .split(inner);
        let panes = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Percentage(28), Constraint::Percentage(72)])
            .split(rows[0]);

        self.draw_file_list(f, panes[0], theme);
        self.draw_diff_pane(f, panes[1], theme);
        self.draw_hint_bar(f, rows[1], theme);
    }

    fn draw_file_list(&self, f: &mut Frame, area: Rect, theme: &Theme) {
        let block = theme.styled_block_plain(false);
        let inner = block.inner(area);
        f.render_widget(block, area);

        if self.files.is_empty() {
            f.render_widget(
                Paragraph::new(Line::from(Span::styled(
                    " no changes vs base",
                    Style::default().fg(theme.text_secondary),
                ))),
                inner,
            );
            return;
        }

        // Keep the selected file visible in tall lists.
        let height = inner.height as usize;
        let first = self
            .selected
            .saturating_sub(height.saturating_sub(1) / 2)
            .min(self.files.len().saturating_sub(height));
        let lines: Vec<Line> = self
            .files
            .iter()
            .enumerate()
            .skip(first)
            .take(height)
            .map(|(i, file)| {
                let marker = if i == self.selected { ">" } else { " " };
                let style = if i == self.selected {
                    Style::default()
                        .fg(theme.accent_info)
                        .add_modifier(Modifier::BOLD)
                } else {
                    Style::default().fg(theme.text_primary)
                };
                Line::from(vec![
                    Span::styled(format!("{marker} {}", file.path), style),
                    Span::styled(
                        format!(" +{}", file.adds),
                        Style::default().fg(theme.accent_success),
                    ),
                    Span::styled(
                        format!(" -{}", file.dels),
                        Style::default().fg(theme.accent_error),
                    ),
                ])
            })
            .collect();
        f.render_widget(Paragraph::new(lines), inner);
    }

    fn draw_diff_pane(&mut self, f: &mut Frame, area: Rect, theme: &Theme) {
        let block = theme.styled_block_plain(false);
        let inner = block.inner(area);
        f.render_widget(block, area);
        self.viewport = inner.height as usize;
        self.clamp();

        let lines: Vec<Line> = self
            .current_lines()
            .iter()
            .skip(self.scroll)
            .take(inner.height as usize)
            .map(|l| {
                let style = match l.kind {
                    DiffLineKind::Add => Style::default().fg(theme.accent_success),
                    DiffLineKind::Del => Style::default().fg(theme.accent_error),
                    DiffLineKind::Hunk => Style::default()
                        .fg(theme.accent_info)
                        .add_modifier(Modifier::BOLD),
                    DiffLineKind::Header => Style::default()
                        .fg(theme.text_secondary)
                        .add_modifier(Modifier::BOLD),
                    DiffLineKind::Context => Style::default().fg(theme.text_primary),
                };
                Line::from(Span::styled(
                    crate::tui::screens::sanitize_for_terminal(&l.text),
                    style,
                ))
            })
            .collect();
        f.render_widget(Paragraph::new(lines), inner);
    }

    fn draw_hint_bar(&self, f: &mut Frame, area: Rect, theme: &Theme) {
        let text = match &self.query_input {
            Some(input) => format!(" /{input}_   (Enter search · Esc cancel)"),
            None => {
                " j/k scroll  Ctrl+d/u page  ]/[ hunk  g/G top/bot  Tab file  / search  n/N next  o shell  q close"
                    .to_string()
            }
        };
        f.render_widget(
            Paragraph::new(Line::from(Span::styled(
                text,
                Style::default().fg(theme.text_secondary),
            ))),
            area,
        );
    }
}

fn centered(area: Rect, pct_w: u16, pct_h: u16) -> Rect {
    let w = area.width * pct_w / 100;
    let h = area.height * pct_h / 100;
    Rect {
        x: area.x + (area.width.saturating_sub(w)) / 2,
        y: area.y + (area.height.saturating_sub(h)) / 2,
        width: w,
        height: h,
    }
}

/// Parse `git diff` unified output into per-file line lists. Tolerant: any
/// unrecognized line inside a file renders as context; bytes before the
/// first `diff --git` are ignored.
pub(crate) fn parse_unified_diff(text: &str) -> Vec<DiffFile> {
    let mut files: Vec<DiffFile> = Vec::new();
    for raw in text.lines() {
        if let Some(rest) = raw.strip_prefix("diff --git ") {
            // `a/old b/new` → label with the new-side path.
            let path = rest
                .split_whitespace()
                .last()
                .map(|p| p.strip_prefix("b/").unwrap_or(p))
                .unwrap_or(rest)
                .to_string();
            files.push(DiffFile {
                path,
                lines: vec![DiffLine {
                    kind: DiffLineKind::Header,
                    text: raw.to_string(),
                }],
                adds: 0,
                dels: 0,
            });
            continue;
        }
        let Some(file) = files.last_mut() else {
            continue;
        };
        let kind = if raw.starts_with("@@") {
            DiffLineKind::Hunk
        } else if raw.starts_with("+++") || raw.starts_with("---") || raw.starts_with("index ") {
            DiffLineKind::Header
        } else if raw.starts_with('+') {
            file.adds += 1;
            DiffLineKind::Add
        } else if raw.starts_with('-') {
            file.dels += 1;
            DiffLineKind::Del
        } else {
            DiffLineKind::Context
        };
        file.lines.push(DiffLine {
            kind,
            text: raw.to_string(),
        });
    }
    files
}

#[cfg(test)]
mod tests {
    use super::*;
    use crossterm::event::{KeyCode, KeyModifiers};

    const SAMPLE: &str = "\
diff --git a/src/a.rs b/src/a.rs
index 111..222 100644
--- a/src/a.rs
+++ b/src/a.rs
@@ -1,3 +1,4 @@
 fn main() {
+    println!(\"hi\");
 }
@@ -10,2 +11,1 @@
-    old();
 fn tail() {}
diff --git a/src/b.rs b/src/b.rs
index 333..444 100644
--- a/src/b.rs
+++ b/src/b.rs
@@ -1,1 +1,1 @@
-fn b_old() {}
+fn b_new() {}
";

    #[test]
    fn parse_splits_files_and_counts_lines() {
        let files = parse_unified_diff(SAMPLE);
        assert_eq!(files.len(), 2);
        assert_eq!(files[0].path, "src/a.rs");
        assert_eq!(files[0].adds, 1);
        assert_eq!(files[0].dels, 1);
        assert_eq!(files[1].path, "src/b.rs");
        assert_eq!(files[1].adds, 1);
        assert_eq!(files[1].dels, 1);
    }

    #[test]
    fn parse_empty_diff_yields_no_files() {
        assert!(parse_unified_diff("").is_empty());
    }

    #[test]
    fn hunk_jump_moves_between_markers() {
        let mut review = DiffReview::new(SAMPLE);
        review.viewport = 5;
        assert_eq!(review.scroll, 0);
        review.handle_key(KeyCode::Char(']'), KeyModifiers::NONE);
        let first_hunk = review.scroll;
        assert!(
            review.current_lines()[first_hunk].kind == DiffLineKind::Hunk,
            "lands on a hunk marker"
        );
        review.handle_key(KeyCode::Char(']'), KeyModifiers::NONE);
        assert!(review.scroll > first_hunk, "advances to the second hunk");
        review.handle_key(KeyCode::Char('['), KeyModifiers::NONE);
        assert_eq!(review.scroll, first_hunk, "jumps back");
    }

    #[test]
    fn tab_cycles_files_and_resets_scroll() {
        let mut review = DiffReview::new(SAMPLE);
        review.scroll = 3;
        review.handle_key(KeyCode::Tab, KeyModifiers::NONE);
        assert_eq!(review.selected, 1);
        assert_eq!(review.scroll, 0);
        review.handle_key(KeyCode::Tab, KeyModifiers::NONE);
        assert_eq!(review.selected, 0, "wraps");
        review.handle_key(KeyCode::BackTab, KeyModifiers::NONE);
        assert_eq!(review.selected, 1, "BackTab cycles back");
    }

    #[test]
    fn search_jumps_to_match_and_n_advances() {
        let mut review = DiffReview::new(SAMPLE);
        review.viewport = 3;
        review.handle_key(KeyCode::Char('/'), KeyModifiers::NONE);
        for c in "fn".chars() {
            review.handle_key(KeyCode::Char(c), KeyModifiers::NONE);
        }
        review.handle_key(KeyCode::Enter, KeyModifiers::NONE);
        let first = review.scroll;
        assert!(review.current_lines()[first].text.contains("fn"));
        review.handle_key(KeyCode::Char('n'), KeyModifiers::NONE);
        assert!(review.scroll > first, "n advances to the next match");
        review.handle_key(KeyCode::Char('N'), KeyModifiers::NONE);
        assert_eq!(review.scroll, first, "N goes back");
    }

    #[test]
    fn close_and_shell_outcomes() {
        let mut review = DiffReview::new(SAMPLE);
        assert_eq!(
            review.handle_key(KeyCode::Char('q'), KeyModifiers::NONE),
            DiffReviewOutcome::Close
        );
        assert_eq!(
            review.handle_key(KeyCode::Esc, KeyModifiers::NONE),
            DiffReviewOutcome::Close
        );
        assert_eq!(
            review.handle_key(KeyCode::Char('o'), KeyModifiers::NONE),
            DiffReviewOutcome::OpenShell
        );
    }

    #[test]
    fn motions_are_read_only_surface() {
        // No key mutates the parsed diff — the only state that moves is
        // scroll/selection/search.
        let mut review = DiffReview::new(SAMPLE);
        let before: Vec<usize> = review.files.iter().map(|f| f.lines.len()).collect();
        for code in [
            KeyCode::Char('j'),
            KeyCode::Char('k'),
            KeyCode::Char('g'),
            KeyCode::Char('G'),
            KeyCode::Char(']'),
            KeyCode::Char('['),
            KeyCode::Tab,
        ] {
            review.handle_key(code, KeyModifiers::NONE);
        }
        let after: Vec<usize> = review.files.iter().map(|f| f.lines.len()).collect();
        assert_eq!(before, after);
    }
}
