//! TurboQuant adapter for session context compression.
//!
//! Bridges the raw quantization pipeline with the session layer by converting
//! text chunks into pseudo-embeddings, compressing them, and tracking metrics.

use crate::util::truncate_at_char_boundary;

/// Honest projection of theoretical TurboQuant savings for a session, derived
/// from its actual `TokenUsage` rather than from running TQ on the raw prompt.
///
/// All fields are derived; the struct is plain data. Constructed via
/// `TurboQuantAdapter::project_savings`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SavingsProjection {
    pub input_tokens: u64,
    pub projected_compressed: u64,
    pub projected_saved_tokens: u64,
    pub projected_saved_usd: f64,
    pub compression_ratio: f64,
    pub rate_per_token_usd: f64,
}

/// Discriminator for per-session savings: real data from fork-handoff
/// compression (`Actual`) vs. theoretical projection from token counts
/// (`Projection`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SavingsKind {
    Projection,
    Actual,
}

impl SavingsKind {
    pub const fn label(self) -> &'static str {
        match self {
            Self::Actual => "ACTUAL",
            Self::Projection => "proj.",
        }
    }
}

/// Per-session rollup shown on the TurboQuant dashboard. Either derived
/// from a real handoff compression or projected from `TokenUsage`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SessionSavings {
    pub kind: SavingsKind,
    pub saved_tokens: u64,
    pub saved_usd: f64,
    pub ratio: f64,
}

/// Metrics emitted after a compression operation.
#[derive(Debug, Clone)]
pub struct CompressionMetrics {
    /// Original token count (estimated from character count).
    pub original_tokens: u64,
    /// Compressed token count (estimated).
    pub compressed_tokens: u64,
    /// Compression ratio (original / compressed).
    pub compression_ratio: f64,
}

impl CompressionMetrics {
    /// Format as a human-readable activity log entry.
    pub fn log_entry(&self) -> String {
        let orig = crate::util::formatting::format_tokens(self.original_tokens);
        let comp = crate::util::formatting::format_tokens(self.compressed_tokens);
        format!(
            "[TurboQuant] Compressed context: {} → {} tokens ({:.1}x)",
            orig, comp, self.compression_ratio
        )
    }
}

/// Rank text segments by semantic similarity to a query.
///
/// Text-preserving: callers keep the original strings and use the returned
/// indices to select which segments survive. Keeps output human-readable text
/// rather than opaque quantized vectors, so callers can inject the result
/// into downstream LLM prompts.
pub trait TextRanker: Send + Sync {
    /// Returns `(index, score)` pairs sorted by descending score.
    /// When disabled or inputs empty, returns indices in original order with
    /// score 0.0 (for disabled case) or an empty vec (for empty input).
    fn rank_segments(&self, segments: &[&str], query: &str) -> Vec<(usize, f32)>;

    /// Indices of segments retained after removing near-duplicates above
    /// `threshold` (cosine similarity, 0.0-1.0). First occurrence wins.
    /// When disabled, returns all indices in order.
    fn dedup_by_similarity(&self, segments: &[&str], threshold: f32) -> Vec<usize>;

    /// Is the ranker currently enabled?
    fn is_ranker_enabled(&self) -> bool;
}

/// TurboQuant adapter that compresses session context using vector quantization.
pub struct TurboQuantAdapter {
    /// Bit width for quantization.
    bit_width: u8,
    /// Whether the adapter is enabled.
    enabled: bool,
}

impl TurboQuantAdapter {
    pub fn new(bit_width: u8) -> Self {
        Self {
            bit_width,
            enabled: true,
        }
    }

    /// Enable or disable the adapter. Test-only seam for exercising the
    /// disabled path through `compact_session_history`, `compress_handoff`,
    /// and `compact_system_prompt`; production flips `config.turboquant.enabled`
    /// at the config layer instead.
    #[cfg(test)]
    pub fn set_enabled(&mut self, enabled: bool) {
        self.enabled = enabled;
    }

    /// Whether the adapter is currently enabled.
    pub fn is_active(&self) -> bool {
        self.enabled
    }

    /// Average-pool all chunk vectors for a text into a single normalized vector.
    /// Used by `TextRanker::rank_segments` and `dedup_by_similarity`.
    fn pool_to_single_vec(&self, text: &str) -> Vec<f32> {
        let chunks = self.text_to_vectors(text);
        if chunks.is_empty() {
            return Vec::new();
        }
        let dim = chunks[0].len();
        let mut sum = vec![0.0f32; dim];
        for v in &chunks {
            for (s, x) in sum.iter_mut().zip(v) {
                *s += *x;
            }
        }
        let n = chunks.len() as f32;
        sum.iter_mut().for_each(|x| *x /= n);
        let norm: f32 = sum.iter().map(|x| x * x).sum::<f32>().sqrt();
        if norm > 0.0 {
            sum.iter_mut().for_each(|x| *x /= norm);
        }
        sum
    }

    /// Convert text to f32 vectors using simple byte-level embedding.
    /// Each chunk of characters is mapped to a fixed-dimension vector.
    fn text_to_vectors(&self, text: &str) -> Vec<Vec<f32>> {
        const CHUNK_SIZE: usize = 256;
        const DIM: usize = 64;

        text.as_bytes()
            .chunks(CHUNK_SIZE)
            .map(|chunk| {
                let mut vec = vec![0.0f32; DIM];
                for (i, &byte) in chunk.iter().enumerate() {
                    vec[i % DIM] += (byte as f32 - 128.0) / 128.0;
                }
                // Normalize
                let norm: f32 = vec.iter().map(|x| x * x).sum::<f32>().sqrt();
                if norm > 0.0 {
                    vec.iter_mut().for_each(|x| *x /= norm);
                }
                vec
            })
            .collect()
    }

    /// Estimate token count from character count (rough: ~4 chars per token).
    pub(crate) fn estimate_tokens(text: &str) -> u64 {
        (text.len() as u64).div_ceil(4)
    }
}

impl TextRanker for TurboQuantAdapter {
    fn rank_segments(&self, segments: &[&str], query: &str) -> Vec<(usize, f32)> {
        if segments.is_empty() {
            return Vec::new();
        }
        if !self.enabled {
            return segments.iter().enumerate().map(|(i, _)| (i, 0.0)).collect();
        }
        let query_vec = self.pool_to_single_vec(query);
        if query_vec.is_empty() {
            return segments.iter().enumerate().map(|(i, _)| (i, 0.0)).collect();
        }
        let mut scored: Vec<(usize, f32)> = segments
            .iter()
            .enumerate()
            .map(|(i, s)| {
                let v = self.pool_to_single_vec(s);
                let score = if v.is_empty() {
                    0.0
                } else {
                    cosine(&query_vec, &v)
                };
                (i, score)
            })
            .collect();
        scored.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
        scored
    }

    fn dedup_by_similarity(&self, segments: &[&str], threshold: f32) -> Vec<usize> {
        if segments.is_empty() {
            return Vec::new();
        }
        if !self.enabled {
            return (0..segments.len()).collect();
        }
        let vecs: Vec<Vec<f32>> = segments
            .iter()
            .map(|s| self.pool_to_single_vec(s))
            .collect();
        let mut keep: Vec<usize> = Vec::new();
        for (i, v) in vecs.iter().enumerate() {
            if v.is_empty() {
                keep.push(i);
                continue;
            }
            let is_dup = keep.iter().any(|&j| {
                if vecs[j].is_empty() {
                    return false;
                }
                cosine(&vecs[j], v) >= threshold
            });
            if !is_dup {
                keep.push(i);
            }
        }
        keep
    }

    fn is_ranker_enabled(&self) -> bool {
        self.enabled
    }
}

fn cosine(a: &[f32], b: &[f32]) -> f32 {
    // Both inputs are already unit-normalized by pool_to_single_vec.
    a.iter().zip(b).map(|(x, y)| x * y).sum()
}

/// Output of fork-handoff compression.
#[derive(Debug, Clone)]
#[allow(dead_code)] // Reason: metrics surface exposed to the TUI activity log and tests
pub struct CompressedHandoff {
    pub text: String,
    pub metrics: CompressionMetrics,
    pub segments_total: usize,
    pub segments_selected: usize,
    pub truncated: bool,
}

impl TurboQuantAdapter {
    /// Compress a parent-session handoff by ranking segments against the
    /// child's task prompt. Segments are split on blank lines (`\n\n`).
    ///
    /// When the adapter is disabled, returns the raw `history` unchanged
    /// with `compression_ratio = 1.0`.
    pub fn compress_handoff(
        &self,
        history: &str,
        task_prompt: &str,
        token_budget: usize,
    ) -> CompressedHandoff {
        if history.is_empty() {
            return CompressedHandoff {
                text: String::new(),
                metrics: CompressionMetrics {
                    original_tokens: 0,
                    compressed_tokens: 0,
                    compression_ratio: 1.0,
                },
                segments_total: 0,
                segments_selected: 0,
                truncated: false,
            };
        }

        let original_tokens = Self::estimate_tokens(history);
        let segments: Vec<&str> = split_handoff_segments(history);
        let segments_total = segments.len();

        if !self.enabled {
            return CompressedHandoff {
                text: history.to_string(),
                metrics: CompressionMetrics {
                    original_tokens,
                    compressed_tokens: original_tokens,
                    compression_ratio: 1.0,
                },
                segments_total,
                segments_selected: segments_total,
                truncated: false,
            };
        }

        if token_budget == 0 {
            return CompressedHandoff {
                text: String::new(),
                metrics: CompressionMetrics {
                    original_tokens,
                    compressed_tokens: 0,
                    compression_ratio: if original_tokens == 0 {
                        1.0
                    } else {
                        original_tokens as f64
                    },
                },
                segments_total,
                segments_selected: 0,
                truncated: false,
            };
        }

        let ranked = self.rank_segments(&segments, task_prompt);
        let tb = crate::turboquant::budget::TokenBudget::new(token_budget as u64);
        let sel = tb.select(&ranked, |i| Self::estimate_tokens(segments[i]));
        let mut kept = sel.indices.clone();
        kept.sort_unstable();
        let text: String = kept
            .iter()
            .map(|&i| segments[i])
            .collect::<Vec<_>>()
            .join("\n\n");

        let compressed_tokens = Self::estimate_tokens(&text);
        let compression_ratio = if compressed_tokens == 0 {
            original_tokens as f64
        } else {
            original_tokens as f64 / compressed_tokens as f64
        };

        CompressedHandoff {
            text,
            metrics: CompressionMetrics {
                original_tokens,
                compressed_tokens,
                compression_ratio,
            },
            segments_total,
            segments_selected: kept.len(),
            truncated: sel.truncated_first,
        }
    }

    /// Deduplicate near-identical prompt components and emit a compacted appendix.
    ///
    /// Uses cosine similarity threshold 0.92. Components are kept in original
    /// order (first occurrence wins). Returns the joined string (components
    /// separated by `\n\n`).
    ///
    /// When the adapter is disabled, returns `components.join("\n\n")`
    /// verbatim (no-op).
    ///
    /// `token_budget`: when > 0, components are dropped from the tail until
    /// the joined string fits. A single component larger than the budget is
    /// truncated to fit (not dropped), so the output never silently drops
    /// all content.
    pub fn compact_system_prompt(&self, components: &[&str], token_budget: usize) -> String {
        if components.is_empty() {
            return String::new();
        }
        if !self.enabled {
            return enforce_budget(&components.join("\n\n"), token_budget);
        }

        let kept_idx = self.dedup_by_similarity(components, 0.92);
        let kept: Vec<&str> = kept_idx.iter().map(|&i| components[i]).collect();
        let joined = kept.join("\n\n");
        enforce_budget(&joined, token_budget)
    }
}

/// Maximum number of segments a single handoff is allowed to produce.
/// Above this, extra segments are dropped to bound ranking cost at O(n·dim).
const MAX_HANDOFF_SEGMENTS: usize = 2_000;

/// Split a handoff blob into segments at paragraph boundaries (blank lines)
/// so tool calls and assistant messages aren't cut mid-word.
/// Segment count is capped to prevent quadratic-time DoS on adversarial input.
fn split_handoff_segments(history: &str) -> Vec<&str> {
    history
        .split("\n\n")
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .take(MAX_HANDOFF_SEGMENTS)
        .collect()
}

fn enforce_budget(text: &str, token_budget: usize) -> String {
    if token_budget == 0 {
        return text.to_string();
    }
    let end = truncate_at_char_boundary(text, token_budget.saturating_mul(4));
    text[..end].to_string()
}

/// Report returned by `compact_session_history` describing what was changed.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct StateCompactionReport {
    pub activity_before: usize,
    pub activity_after: usize,
    pub dedup_collapsed: usize,
    pub trimmed_non_key: usize,
}

impl TurboQuantAdapter {
    /// Compact a session's activity log in place.
    ///
    /// Pass 1: collapse consecutive identical messages (same `message` field)
    /// into a single entry annotated `"<msg> (xN)"`. The first timestamp of
    /// the run is preserved.
    ///
    /// Pass 2: when the session is terminal AND the adapter is enabled,
    /// retain only "key events" (errors, completions, file touches, status
    /// transitions, turboquant metrics).
    ///
    /// Returns a report describing the before/after counts. No-op (returns a
    /// zero-delta report) when the adapter is disabled.
    pub fn compact_session_history(
        &self,
        session: &mut crate::session::types::Session,
    ) -> StateCompactionReport {
        let before = session.activity_log.len();
        if !self.enabled || before == 0 {
            return StateCompactionReport {
                activity_before: before,
                activity_after: before,
                ..Default::default()
            };
        }

        let dedup_collapsed = collapse_consecutive(&mut session.activity_log);

        let trimmed_non_key = if session.status.is_terminal() {
            trim_to_key_events(&mut session.activity_log)
        } else {
            0
        };

        StateCompactionReport {
            activity_before: before,
            activity_after: session.activity_log.len(),
            dedup_collapsed,
            trimmed_non_key,
        }
    }
}

fn collapse_consecutive(log: &mut Vec<crate::session::types::ActivityEntry>) -> usize {
    if log.len() < 2 {
        return 0;
    }
    let original = log.len();
    let mut out: Vec<crate::session::types::ActivityEntry> = Vec::with_capacity(original);
    let mut run_count: usize = 0;
    let mut run_base: Option<String> = None;

    for entry in log.drain(..) {
        let base_message = base_message_of(&entry.message);
        match &run_base {
            Some(rb) if *rb == base_message => {
                run_count += 1;
                if let Some(last) = out.last_mut() {
                    last.message = format!("{} (x{})", rb, run_count);
                }
            }
            _ => {
                run_base = Some(base_message);
                run_count = 1;
                out.push(entry);
            }
        }
    }
    *log = out;
    original - log.len()
}

/// Strip an existing `(xN)` suffix so re-compaction is idempotent.
fn base_message_of(msg: &str) -> String {
    if let Some(open) = msg.rfind(" (x") {
        let tail = &msg[open + 3..];
        if let Some(end) = tail.find(')')
            && tail[..end].chars().all(|c| c.is_ascii_digit())
            && end == tail.len() - 1
        {
            return msg[..open].to_string();
        }
    }
    msg.to_string()
}

const KEY_EVENT_PREFIXES: &[&str] = &[
    "STATUS:",
    "Error",
    "Session completed",
    "Session forked",
    "Tool ",
    "Tool:",
    "Bash:",
    "Write:",
    "Edit:",
    "Read:",
    "Glob:",
    "Grep:",
];

const KEY_EVENT_SUBSTRINGS: &[&str] = &["Error", "errored", "Forked", "[TurboQuant]"];

fn is_key_event(msg: &str) -> bool {
    KEY_EVENT_PREFIXES.iter().any(|p| msg.starts_with(p))
        || KEY_EVENT_SUBSTRINGS.iter().any(|s| msg.contains(s))
}

fn trim_to_key_events(log: &mut Vec<crate::session::types::ActivityEntry>) -> usize {
    let before = log.len();
    log.retain(|e| is_key_event(&e.message));
    before - log.len()
}

impl TurboQuantAdapter {
    /// Compute theoretical compression savings for a session's accumulated
    /// input tokens. Pure — no I/O. Given a non-zero `rate_per_token_usd`,
    /// also estimates dollar savings.
    ///
    /// Safe on adversarial inputs: `bit_width` of 0 is clamped to 1 (ratio
    /// degenerates to 1.0, no panic), `u64::MAX` inputs saturate.
    pub fn project_savings(
        &self,
        token_usage: &crate::session::types::TokenUsage,
        rate_per_token_usd: f64,
    ) -> SavingsProjection {
        let input = token_usage.input_tokens;
        let divisor = (self.bit_width.max(1)) as u64;
        let projected_compressed = input.div_ceil(divisor);
        let projected_saved_tokens = input.saturating_sub(projected_compressed);
        let ratio = if projected_compressed == 0 {
            1.0
        } else {
            input as f64 / projected_compressed as f64
        };
        let projected_saved_usd = projected_saved_tokens as f64 * rate_per_token_usd;
        SavingsProjection {
            input_tokens: input,
            projected_compressed,
            projected_saved_tokens,
            projected_saved_usd,
            compression_ratio: ratio,
            rate_per_token_usd,
        }
    }
}

/// Per-session savings rollup: returns `Actual` when the session has real
/// fork-handoff compression data persisted, `Projection` otherwise, or
/// `None` when there's nothing measurable (no input tokens and no handoff).
pub fn session_savings(
    session: &crate::session::types::Session,
    adapter: &TurboQuantAdapter,
) -> Option<SessionSavings> {
    let rate = implied_rate_per_token(session);
    if let (Some(orig), Some(comp)) = (
        session.tq_handoff_original_tokens,
        session.tq_handoff_compressed_tokens,
    ) {
        let saved_tokens = orig.saturating_sub(comp);
        let ratio = if comp == 0 {
            1.0
        } else {
            orig as f64 / comp as f64
        };
        return Some(SessionSavings {
            kind: SavingsKind::Actual,
            saved_tokens,
            saved_usd: saved_tokens as f64 * rate,
            ratio,
        });
    }
    if session.token_usage.input_tokens == 0 {
        return None;
    }
    let proj = adapter.project_savings(&session.token_usage, rate);
    Some(SessionSavings {
        kind: SavingsKind::Projection,
        saved_tokens: proj.projected_saved_tokens,
        saved_usd: proj.projected_saved_usd,
        ratio: proj.compression_ratio,
    })
}

/// Derive an implicit $/token rate from a session's observed cost and total
/// token consumption. Returns 0.0 when cost is non-positive or tokens are
/// zero (via `TokenUsage::cost_per_kilo_token`, which already guards both).
pub fn implied_rate_per_token(session: &crate::session::types::Session) -> f64 {
    if session.cost_usd <= 0.0 {
        return 0.0;
    }
    session.token_usage.cost_per_kilo_token(session.cost_usd) / 1000.0
}

#[cfg(test)]
mod tests {
    use super::*;

    // -- CompressionMetrics --

    #[test]
    fn metrics_log_entry_format() {
        let metrics = CompressionMetrics {
            original_tokens: 45000,
            compressed_tokens: 12000,
            compression_ratio: 3.75,
        };
        let entry = metrics.log_entry();
        assert!(entry.contains("[TurboQuant]"));
        assert!(entry.contains("45.0k"));
        assert!(entry.contains("12.0k"));
        assert!(entry.contains("3.8x") || entry.contains("3.7x"));
    }

    // -- text_to_vectors --

    #[test]
    fn text_to_vectors_produces_normalized_vectors() {
        let adapter = TurboQuantAdapter::new(4);
        let vectors =
            adapter.text_to_vectors("Hello, world! This is a test of the TurboQuant adapter.");
        assert!(!vectors.is_empty());
        for v in &vectors {
            assert_eq!(v.len(), 64);
            let norm: f32 = v.iter().map(|x| x * x).sum::<f32>().sqrt();
            assert!(
                (norm - 1.0).abs() < 0.01,
                "vector should be normalized, got norm={}",
                norm
            );
        }
    }

    // -- TextRanker --

    fn ranker() -> TurboQuantAdapter {
        TurboQuantAdapter::new(4)
    }

    #[test]
    fn rank_segments_empty_input_returns_empty_vec() {
        let r = ranker();
        let out = r.rank_segments(&[], "anything");
        assert!(out.is_empty());
    }

    #[test]
    fn rank_segments_single_segment_returns_one_entry() {
        let r = ranker();
        let out = r.rank_segments(&["only segment"], "search");
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].0, 0);
    }

    #[test]
    fn rank_segments_disabled_returns_original_order_zero_scores() {
        let mut r = ranker();
        r.set_enabled(false);
        let out = r.rank_segments(&["a", "b", "c"], "q");
        let indices: Vec<usize> = out.iter().map(|(i, _)| *i).collect();
        assert_eq!(indices, vec![0, 1, 2]);
    }

    #[test]
    fn rank_segments_scores_are_finite() {
        let r = ranker();
        let segs = ["alpha beta", "gamma delta", "epsilon zeta"];
        let out = r.rank_segments(&segs, "random query");
        for (_, s) in &out {
            assert!(s.is_finite(), "score must be finite, got {}", s);
        }
    }

    #[test]
    fn rank_segments_returns_sorted_by_descending_score() {
        let r = ranker();
        let segs = ["cargo test", "make tea", "run tests with cargo"];
        let out = r.rank_segments(&segs, "run cargo tests");
        assert_eq!(out.len(), 3);
        for w in out.windows(2) {
            assert!(
                w[0].1 >= w[1].1,
                "scores not descending: {} then {}",
                w[0].1,
                w[1].1
            );
        }
    }

    #[test]
    fn dedup_by_similarity_empty_returns_empty() {
        let r = ranker();
        let out = r.dedup_by_similarity(&[], 0.9);
        assert!(out.is_empty());
    }

    #[test]
    fn dedup_by_similarity_disabled_keeps_all() {
        let mut r = ranker();
        r.set_enabled(false);
        let out = r.dedup_by_similarity(&["a", "b", "c"], 0.9);
        assert_eq!(out, vec![0, 1, 2]);
    }

    #[test]
    fn dedup_by_similarity_collapses_identical_strings() {
        let r = ranker();
        let segs = ["Same content here.", "Same content here.", "Different."];
        let out = r.dedup_by_similarity(&segs, 0.95);
        assert_eq!(out.len(), 2);
        assert_eq!(out[0], 0);
        assert_eq!(out[1], 2);
    }

    #[test]
    fn is_ranker_enabled_tracks_adapter_state() {
        let mut r = ranker();
        assert!(r.is_ranker_enabled());
        r.set_enabled(false);
        assert!(!r.is_ranker_enabled());
    }

    // -- compact_session_history (#345) --

    use crate::session::types::{ActivityEntry, Session, SessionStatus};
    use chrono::Utc;

    fn push_entries(session: &mut Session, messages: &[&str]) {
        for m in messages {
            session.activity_log.push(ActivityEntry {
                timestamp: Utc::now(),
                message: (*m).to_string(),
            });
        }
    }

    fn make_running_session() -> Session {
        let mut s = Session::new("p".into(), "opus".into(), "orchestrator".into(), None);
        s.status = SessionStatus::Running;
        s
    }

    fn make_terminal_session() -> Session {
        let mut s = Session::new("p".into(), "opus".into(), "orchestrator".into(), None);
        s.status = SessionStatus::Completed;
        s
    }

    #[test]
    fn compact_history_collapses_consecutive_identical_entries() {
        let adapter = ranker();
        let mut s = make_running_session();
        let ten = vec!["Tool: Bash"; 10];
        push_entries(&mut s, &ten);
        let report = adapter.compact_session_history(&mut s);
        assert_eq!(s.activity_log.len(), 1);
        assert!(s.activity_log[0].message.contains("Bash"));
        assert!(s.activity_log[0].message.contains("x10"));
        assert_eq!(report.activity_before, 10);
        assert_eq!(report.activity_after, 1);
        assert!(report.dedup_collapsed >= 9);
    }

    #[test]
    fn compact_history_200_repetitive_entries_compresses_significantly() {
        let adapter = ranker();
        let mut s = make_running_session();
        // 4 distinct messages, 50 repetitions each, NOT interleaved
        let mut msgs: Vec<&str> = Vec::new();
        for _ in 0..50 {
            msgs.push("Tool: Bash");
        }
        for _ in 0..50 {
            msgs.push("Read: src/lib.rs");
        }
        for _ in 0..50 {
            msgs.push("Tool: Bash");
        }
        for _ in 0..50 {
            msgs.push("Tool: Grep");
        }
        push_entries(&mut s, &msgs);
        assert_eq!(s.activity_log.len(), 200);
        let report = adapter.compact_session_history(&mut s);
        assert!(s.activity_log.len() <= 25);
        assert!(report.dedup_collapsed > 0);
    }

    #[test]
    fn compact_history_preserves_error_entries_interleaved_in_terminal_session() {
        let adapter = ranker();
        let mut s = make_terminal_session();
        for _ in 0..20 {
            s.activity_log.push(ActivityEntry {
                timestamp: Utc::now(),
                message: "Tool: Bash".into(),
            });
        }
        s.activity_log.push(ActivityEntry {
            timestamp: Utc::now(),
            message: "Error: process exited with code 1".into(),
        });
        s.activity_log.push(ActivityEntry {
            timestamp: Utc::now(),
            message: "Tool: Bash".into(),
        });
        s.activity_log.push(ActivityEntry {
            timestamp: Utc::now(),
            message: "Error: build failed".into(),
        });
        adapter.compact_session_history(&mut s);
        let error_count = s
            .activity_log
            .iter()
            .filter(|e| e.message.contains("Error"))
            .count();
        assert_eq!(error_count, 2);
    }

    #[test]
    fn compact_history_noop_when_adapter_disabled() {
        let mut adapter = ranker();
        adapter.set_enabled(false);
        let mut s = make_running_session();
        let msgs = vec!["Tool: Bash"; 200];
        push_entries(&mut s, &msgs);
        let report = adapter.compact_session_history(&mut s);
        assert_eq!(s.activity_log.len(), 200);
        assert_eq!(report.dedup_collapsed, 0);
        assert_eq!(report.trimmed_non_key, 0);
    }

    #[test]
    fn compact_history_does_not_collapse_non_consecutive_duplicates() {
        let adapter = ranker();
        let mut s = make_running_session();
        push_entries(&mut s, &["A", "B", "A", "B", "A"]);
        adapter.compact_session_history(&mut s);
        assert_eq!(s.activity_log.len(), 5);
    }

    #[test]
    fn compact_history_empty_log_is_safe() {
        let adapter = ranker();
        let mut s = make_running_session();
        let report = adapter.compact_session_history(&mut s);
        assert_eq!(s.activity_log.len(), 0);
        assert_eq!(report.activity_before, 0);
        assert_eq!(report.activity_after, 0);
    }

    #[test]
    fn compact_history_single_entry_unchanged() {
        let adapter = ranker();
        let mut s = make_running_session();
        push_entries(&mut s, &["only entry"]);
        adapter.compact_session_history(&mut s);
        assert_eq!(s.activity_log.len(), 1);
        assert_eq!(s.activity_log[0].message, "only entry");
    }

    #[test]
    fn compact_history_terminal_session_trims_non_key_events() {
        let adapter = ranker();
        let mut s = make_terminal_session();
        push_entries(
            &mut s,
            &[
                "Random update 1",
                "Random update 2",
                "Tool: Bash",
                "Another chatty message",
                "STATUS: RUNNING -> COMPLETED",
            ],
        );
        adapter.compact_session_history(&mut s);
        // non-key "Random update" lines should be gone; key events survive
        let has_tool = s.activity_log.iter().any(|e| e.message.contains("Bash"));
        let has_status = s.activity_log.iter().any(|e| e.message.contains("STATUS:"));
        let has_random = s.activity_log.iter().any(|e| e.message.contains("Random"));
        assert!(has_tool);
        assert!(has_status);
        assert!(!has_random);
    }

    #[test]
    fn compact_history_non_terminal_session_keeps_all_entries_post_dedup() {
        let adapter = ranker();
        let mut s = make_running_session();
        push_entries(
            &mut s,
            &["Random update 1", "Another chatty message", "Tool: Bash"],
        );
        adapter.compact_session_history(&mut s);
        assert_eq!(s.activity_log.len(), 3);
    }

    // -- compress_handoff (#343) --

    #[test]
    fn compress_handoff_empty_history_returns_empty_text() {
        let a = ranker();
        let out = a.compress_handoff("", "any task", 4096);
        assert!(out.text.is_empty());
        assert_eq!(out.segments_total, 0);
        assert_eq!(out.segments_selected, 0);
    }

    #[test]
    fn compress_handoff_disabled_returns_raw_history() {
        let mut a = ranker();
        a.set_enabled(false);
        let history = "Tool: Bash\n\nAssistant: done";
        let out = a.compress_handoff(history, "task", 100);
        assert_eq!(out.text, history);
        assert!((out.metrics.compression_ratio - 1.0).abs() < f64::EPSILON);
    }

    #[test]
    fn compress_handoff_respects_token_budget() {
        let a = ranker();
        let mut history = String::new();
        for i in 0..100 {
            history.push_str(&format!(
                "Segment {} has some unique payload. xxxxxxxxxxx\n\n",
                i
            ));
        }
        let out = a.compress_handoff(&history, "pick relevant work", 256);
        assert!(out.text.len() / 4 <= 266);
        assert!(out.segments_selected < out.segments_total);
    }

    #[test]
    fn compress_handoff_zero_budget_yields_empty_text() {
        let a = ranker();
        let out = a.compress_handoff("some history", "task", 0);
        assert!(out.text.is_empty());
        assert_eq!(out.segments_selected, 0);
    }

    #[test]
    fn compress_handoff_metrics_match_estimates() {
        let a = ranker();
        let history = "a".repeat(400);
        let out = a.compress_handoff(&history, "task", 4096);
        assert_eq!(out.metrics.original_tokens, 100);
    }

    #[test]
    fn compress_handoff_single_segment_under_budget_passes_through() {
        let a = ranker();
        let history = "Tool: Bash\n$ echo hello";
        let out = a.compress_handoff(history, "run bash", 4096);
        assert_eq!(out.segments_total, 1);
        assert_eq!(out.segments_selected, 1);
        assert!(out.text.contains("echo hello"));
    }

    #[test]
    fn compress_handoff_segments_split_at_blank_lines() {
        let a = ranker();
        let history = "[Tool: Read]\nsrc/main.rs\n\n[Assistant]\nFile has 100 lines";
        let out = a.compress_handoff(history, "inspect file", 4096);
        assert_eq!(out.segments_total, 2);
    }

    #[test]
    fn compress_handoff_truncated_flag_set_when_first_segment_exceeds_budget() {
        let a = ranker();
        let big = "y".repeat(4000);
        let out = a.compress_handoff(&big, "task", 100);
        assert!(out.truncated);
        assert_eq!(out.segments_selected, 1);
    }

    #[test]
    fn compress_handoff_ranks_relevant_segment_first() {
        let a = ranker();
        let history = "I made a cup of tea today and read a book.\n\n\
                       cargo test suite ran with 200 tests passing in 3 seconds.";
        let out = a.compress_handoff(history, "cargo test suite results", 20);
        assert!(out.segments_selected >= 1);
        assert!(out.text.contains("cargo") || out.text.contains("tests"));
    }

    // -- compact_system_prompt (#344) --

    #[test]
    fn compact_system_prompt_empty_input_returns_empty_string() {
        let a = ranker();
        assert_eq!(a.compact_system_prompt(&[], 4096), "");
    }

    #[test]
    fn compact_system_prompt_single_component_passes_through() {
        let a = ranker();
        let out = a.compact_system_prompt(&["You are a helpful assistant."], 4096);
        assert_eq!(out, "You are a helpful assistant.");
    }

    #[test]
    fn compact_system_prompt_disabled_returns_joined_raw() {
        let mut a = ranker();
        a.set_enabled(false);
        let out = a.compact_system_prompt(&["A", "B", "C"], 4096);
        assert!(out.contains("A"));
        assert!(out.contains("B"));
        assert!(out.contains("C"));
    }

    #[test]
    fn compact_system_prompt_identical_components_collapse_to_one() {
        let a = ranker();
        let out = a.compact_system_prompt(&["same text", "same text", "same text"], 4096);
        assert_eq!(out.matches("same text").count(), 1);
    }

    #[test]
    fn compact_system_prompt_preserves_distinct_components() {
        let a = ranker();
        let out = a.compact_system_prompt(
            &[
                "File claims: src/a.rs, src/b.rs locked by this session.",
                "NEVER bypass authentication checks in the codebase.",
            ],
            4096,
        );
        assert!(out.contains("File claims"));
        assert!(out.contains("NEVER bypass authentication"));
    }

    #[test]
    fn compact_system_prompt_respects_token_budget() {
        let a = ranker();
        let big = "x".repeat(8000);
        let big_s: &str = &big;
        let out = a.compact_system_prompt(&[big_s, big_s, big_s], 1000);
        assert!(out.len() / 4 <= 1010);
    }

    #[test]
    fn compact_system_prompt_zero_budget_is_unbounded() {
        let a = ranker();
        let out = a.compact_system_prompt(
            &[
                "File claims for this session: src/a.rs",
                "NEVER bypass authentication in the production code paths.",
            ],
            0,
        );
        assert!(out.contains("File claims") && out.contains("NEVER bypass"));
    }

    #[test]
    fn compact_system_prompt_single_oversized_is_truncated_not_dropped() {
        let a = ranker();
        let big = "y".repeat(10_000);
        let out = a.compact_system_prompt(&[&big], 100);
        assert!(!out.is_empty());
        assert!(out.len() / 4 <= 110);
    }

    // -- project_savings + session_savings + implied_rate_per_token (#346) --

    use crate::session::types::TokenUsage;

    fn projection_adapter(bit_width: u8) -> TurboQuantAdapter {
        TurboQuantAdapter::new(bit_width)
    }

    fn usage_with_input(input: u64) -> TokenUsage {
        TokenUsage {
            input_tokens: input,
            ..Default::default()
        }
    }

    #[test]
    fn project_savings_zero_input_tokens() {
        let a = projection_adapter(4);
        let p = a.project_savings(&usage_with_input(0), 0.000_001);
        assert_eq!(p.input_tokens, 0);
        assert_eq!(p.projected_compressed, 0);
        assert_eq!(p.projected_saved_tokens, 0);
        assert!((p.projected_saved_usd - 0.0).abs() < f64::EPSILON);
        assert!((p.compression_ratio - 1.0).abs() < f64::EPSILON);
    }

    #[test]
    fn project_savings_zero_rate() {
        let a = projection_adapter(4);
        let p = a.project_savings(&usage_with_input(1000), 0.0);
        assert!(p.projected_saved_tokens > 0);
        assert!((p.projected_saved_usd - 0.0).abs() < f64::EPSILON);
    }

    #[test]
    fn project_savings_typical_bit_width_4() {
        let a = projection_adapter(4);
        let p = a.project_savings(&usage_with_input(1000), 0.0);
        assert_eq!(p.projected_compressed, 250);
        assert_eq!(p.projected_saved_tokens, 750);
        assert!((p.compression_ratio - 4.0).abs() < f64::EPSILON);
    }

    #[test]
    fn project_savings_bit_width_8() {
        let a = projection_adapter(8);
        let p = a.project_savings(&usage_with_input(800), 0.0);
        assert_eq!(p.projected_compressed, 100);
        assert!((p.compression_ratio - 8.0).abs() < f64::EPSILON);
    }

    #[test]
    fn project_savings_bit_width_zero_guards() {
        let a = projection_adapter(0);
        let p = a.project_savings(&usage_with_input(500), 0.0);
        // ratio degenerates to 1.0; no panic; compressed == input
        assert!((p.compression_ratio - 1.0).abs() < f64::EPSILON);
    }

    #[test]
    fn project_savings_saturating_on_overflow() {
        let a = projection_adapter(4);
        let p = a.project_savings(&usage_with_input(u64::MAX), 0.0);
        // Does not panic and remains a sane projection (compressed <= input).
        assert_eq!(p.input_tokens, u64::MAX);
        assert!(p.projected_compressed <= p.input_tokens);
        assert!(p.projected_saved_tokens <= p.input_tokens);
    }

    #[test]
    fn project_savings_rate_math() {
        let a = projection_adapter(4);
        let p = a.project_savings(&usage_with_input(1000), 0.000_002);
        let expected = 750.0 * 0.000_002;
        assert!((p.projected_saved_usd - expected).abs() < 1e-9);
    }

    #[test]
    fn project_savings_small_inputs_div_ceil() {
        let a = projection_adapter(4);
        let p = a.project_savings(&usage_with_input(1), 0.0);
        assert_eq!(p.projected_compressed, 1);
        assert_eq!(p.projected_saved_tokens, 0);
    }

    fn session_with_handoff(
        input_tokens: u64,
        original: u64,
        compressed: u64,
        cost_usd: f64,
    ) -> Session {
        let mut s = Session::new("p".into(), "m".into(), "orchestrator".into(), None);
        s.token_usage = usage_with_input(input_tokens);
        s.tq_handoff_original_tokens = Some(original);
        s.tq_handoff_compressed_tokens = Some(compressed);
        s.cost_usd = cost_usd;
        s
    }

    fn session_no_handoff(input_tokens: u64, cost_usd: f64) -> Session {
        let mut s = Session::new("p".into(), "m".into(), "orchestrator".into(), None);
        s.token_usage = usage_with_input(input_tokens);
        s.cost_usd = cost_usd;
        s
    }

    #[test]
    fn session_savings_actual_when_handoff_set() {
        let a = projection_adapter(4);
        let s = session_with_handoff(500, 1000, 250, 0.002);
        let r = session_savings(&s, &a).unwrap();
        assert_eq!(r.kind, SavingsKind::Actual);
        assert_eq!(r.saved_tokens, 750);
        assert!(r.saved_usd > 0.0);
    }

    #[test]
    fn session_savings_projection_when_handoff_absent() {
        let a = projection_adapter(4);
        let s = session_no_handoff(1000, 0.002);
        let r = session_savings(&s, &a).unwrap();
        assert_eq!(r.kind, SavingsKind::Projection);
    }

    #[test]
    fn session_savings_none_when_empty() {
        let a = projection_adapter(4);
        let s = session_no_handoff(0, 0.0);
        assert!(session_savings(&s, &a).is_none());
    }

    #[test]
    fn session_savings_actual_uses_real_ratio() {
        let a = projection_adapter(4);
        let s = session_with_handoff(0, 2000, 400, 0.0);
        let r = session_savings(&s, &a).unwrap();
        assert_eq!(r.kind, SavingsKind::Actual);
        assert!((r.ratio - 5.0).abs() < f64::EPSILON);
    }

    #[test]
    fn implied_rate_per_token_zero_tokens_returns_zero() {
        let mut s = session_no_handoff(0, 0.05);
        s.token_usage = TokenUsage::default();
        assert!((implied_rate_per_token(&s) - 0.0).abs() < f64::EPSILON);
    }

    #[test]
    fn implied_rate_per_token_zero_cost_returns_zero() {
        let s = session_no_handoff(1000, 0.0);
        assert!((implied_rate_per_token(&s) - 0.0).abs() < f64::EPSILON);
    }

    #[test]
    fn implied_rate_per_token_normal_case() {
        let s = session_no_handoff(1000, 0.002);
        assert!((implied_rate_per_token(&s) - 0.000_002).abs() < 1e-12);
    }

    #[test]
    fn compact_history_is_idempotent_on_already_compacted_log() {
        let adapter = ranker();
        let mut s = make_running_session();
        push_entries(&mut s, &vec!["Tool: Bash"; 5]);
        adapter.compact_session_history(&mut s);
        let after_first = s.activity_log.clone();
        adapter.compact_session_history(&mut s);
        assert_eq!(s.activity_log.len(), after_first.len());
        assert_eq!(s.activity_log[0].message, after_first[0].message);
    }
}
