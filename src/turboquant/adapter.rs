//! TurboQuant adapter for session context compression.
//!
//! Bridges the raw quantization pipeline with the session layer by converting
//! text chunks into pseudo-embeddings, compressing them, and tracking metrics.

use super::pipeline::turbo_quantize;
use super::types::{QuantStrategy, TurboQuantized};

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
        let orig = format_token_count(self.original_tokens);
        let comp = format_token_count(self.compressed_tokens);
        format!(
            "[TurboQuant] Compressed context: {} → {} tokens ({:.1}x)",
            orig, comp, self.compression_ratio
        )
    }
}

/// Result of compressing context.
#[derive(Debug, Clone)]
#[allow(dead_code)] // vectors and strategy used for future decompression path
pub struct CompressedContext {
    /// The compressed vectors (one per chunk).
    pub vectors: Vec<TurboQuantized>,
    /// Compression metrics.
    pub metrics: CompressionMetrics,
    /// Strategy used.
    pub strategy: QuantStrategy,
}

/// Trait for context compression. Mockable for testing.
pub trait ContextCompressor: Send + Sync {
    /// Compress a prompt string into a compact representation.
    /// Returns None if compression is not beneficial (below threshold).
    fn compress(&self, prompt: &str, context_pct: f64) -> Option<CompressedContext>;

    /// Check if the compressor is currently active.
    #[allow(dead_code)]
    fn is_active(&self) -> bool;
}

/// TurboQuant adapter that compresses session context using vector quantization.
pub struct TurboQuantAdapter {
    /// Bit width for quantization.
    bit_width: u8,
    /// Quantization strategy.
    strategy: QuantStrategy,
    /// Overflow threshold percentage (0-100) at which compression activates.
    overflow_threshold_pct: f64,
    /// Whether auto_on_overflow is enabled.
    auto_on_overflow: bool,
    /// Whether the adapter is enabled.
    enabled: bool,
}

impl TurboQuantAdapter {
    pub fn new(
        bit_width: u8,
        strategy: QuantStrategy,
        overflow_threshold_pct: f64,
        auto_on_overflow: bool,
    ) -> Self {
        Self {
            bit_width,
            strategy,
            overflow_threshold_pct,
            auto_on_overflow,
            enabled: true,
        }
    }

    /// Enable or disable the adapter at runtime.
    #[allow(dead_code)] // Used by tests and future runtime toggle integration
    pub fn set_enabled(&mut self, enabled: bool) {
        self.enabled = enabled;
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
    fn estimate_tokens(text: &str) -> u64 {
        (text.len() as u64).div_ceil(4)
    }
}

impl ContextCompressor for TurboQuantAdapter {
    fn compress(&self, prompt: &str, context_pct: f64) -> Option<CompressedContext> {
        if !self.enabled {
            return None;
        }

        // Only activate if auto_on_overflow is set and threshold is approached
        if self.auto_on_overflow && context_pct < self.overflow_threshold_pct {
            return None;
        }

        let original_tokens = Self::estimate_tokens(prompt);
        if original_tokens == 0 {
            return None;
        }

        let vectors = self.text_to_vectors(prompt);
        let compressed: Vec<TurboQuantized> = vectors
            .iter()
            .map(|v| turbo_quantize(v, self.bit_width.max(2)))
            .collect();

        // Estimate compressed size: each TurboQuantized is much smaller than raw text
        let compressed_tokens =
            (original_tokens as f64 / self.bit_width.max(1) as f64).ceil() as u64;
        let compressed_tokens = compressed_tokens.max(1);

        let compression_ratio = original_tokens as f64 / compressed_tokens as f64;

        Some(CompressedContext {
            vectors: compressed,
            metrics: CompressionMetrics {
                original_tokens,
                compressed_tokens,
                compression_ratio,
            },
            strategy: self.strategy,
        })
    }

    fn is_active(&self) -> bool {
        self.enabled
    }
}

/// No-op compressor for when TurboQuant is disabled.
pub struct NoOpCompressor;

impl ContextCompressor for NoOpCompressor {
    fn compress(&self, _prompt: &str, _context_pct: f64) -> Option<CompressedContext> {
        None
    }

    fn is_active(&self) -> bool {
        false
    }
}

fn format_token_count(n: u64) -> String {
    if n >= 1_000_000 {
        format!("{:.1}M", n as f64 / 1_000_000.0)
    } else if n >= 1_000 {
        format!("{:.1}k", n as f64 / 1_000.0)
    } else {
        format!("{}", n)
    }
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

    // -- TurboQuantAdapter --

    #[test]
    fn adapter_compresses_when_enabled_and_above_threshold() {
        let adapter = TurboQuantAdapter::new(4, QuantStrategy::TurboQuant, 80.0, true);
        let prompt = "x".repeat(1000);
        let result = adapter.compress(&prompt, 85.0);
        assert!(result.is_some());
        let ctx = result.unwrap();
        assert!(ctx.metrics.original_tokens > 0);
        assert!(ctx.metrics.compressed_tokens > 0);
        assert!(ctx.metrics.compression_ratio > 1.0);
    }

    #[test]
    fn adapter_skips_when_below_threshold() {
        let adapter = TurboQuantAdapter::new(4, QuantStrategy::TurboQuant, 80.0, true);
        let prompt = "x".repeat(1000);
        let result = adapter.compress(&prompt, 50.0);
        assert!(result.is_none());
    }

    #[test]
    fn adapter_skips_when_disabled() {
        let mut adapter = TurboQuantAdapter::new(4, QuantStrategy::TurboQuant, 80.0, true);
        adapter.set_enabled(false);
        let prompt = "x".repeat(1000);
        let result = adapter.compress(&prompt, 95.0);
        assert!(result.is_none());
    }

    #[test]
    fn adapter_skips_empty_prompt() {
        let adapter = TurboQuantAdapter::new(4, QuantStrategy::TurboQuant, 80.0, true);
        let result = adapter.compress("", 90.0);
        assert!(result.is_none());
    }

    #[test]
    fn adapter_is_active_reflects_enabled_state() {
        let mut adapter = TurboQuantAdapter::new(4, QuantStrategy::TurboQuant, 80.0, true);
        assert!(adapter.is_active());
        adapter.set_enabled(false);
        assert!(!adapter.is_active());
    }

    #[test]
    fn adapter_compresses_without_auto_overflow_at_any_pct() {
        let adapter = TurboQuantAdapter::new(4, QuantStrategy::TurboQuant, 80.0, false);
        let prompt = "hello world ".repeat(100);
        // auto_on_overflow is false, so it should compress regardless of context_pct
        let result = adapter.compress(&prompt, 10.0);
        assert!(result.is_some());
    }

    #[test]
    fn adapter_metrics_have_correct_strategy() {
        let adapter = TurboQuantAdapter::new(4, QuantStrategy::PolarQuant, 80.0, false);
        let prompt = "x".repeat(500);
        let ctx = adapter.compress(&prompt, 90.0).unwrap();
        assert_eq!(ctx.strategy, QuantStrategy::PolarQuant);
    }

    // -- NoOpCompressor --

    #[test]
    fn noop_compressor_returns_none() {
        let compressor = NoOpCompressor;
        assert!(compressor.compress("anything", 99.0).is_none());
        assert!(!compressor.is_active());
    }

    // -- format_token_count --

    #[test]
    fn format_token_count_small() {
        assert_eq!(format_token_count(500), "500");
    }

    #[test]
    fn format_token_count_thousands() {
        assert_eq!(format_token_count(45000), "45.0k");
    }

    // -- text_to_vectors --

    #[test]
    fn text_to_vectors_produces_normalized_vectors() {
        let adapter = TurboQuantAdapter::new(4, QuantStrategy::TurboQuant, 80.0, false);
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
}
