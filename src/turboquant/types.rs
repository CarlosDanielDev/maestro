use serde::{Deserialize, Serialize};

/// Quantization strategy selector for TurboQuant.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum QuantStrategy {
    #[default]
    TurboQuant,
    PolarQuant,
    Qjl,
}

/// Which vector components to apply quantization to.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ApplyTarget {
    Keys,
    Values,
    #[default]
    Both,
}

/// Result of PolarQuant encoding.
#[derive(Debug, Clone)]
pub struct QuantizedVector {
    /// The single global magnitude (radius at top of recursion).
    pub scale: f32,
    /// Quantized angle codes, one per pair at each recursion level.
    pub codes: Vec<u16>,
    /// Bit width used for quantization.
    pub bits: u8,
    /// Original vector length (needed for dequantization).
    pub original_len: usize,
}

/// Result of QJL sign-bit compression.
#[derive(Debug, Clone)]
pub struct QjlBitVector {
    /// Packed sign bits from the JL projection (64 bits per u64).
    pub packed_signs: Vec<u64>,
    /// Number of projection dimensions used.
    pub projection_dim: usize,
    /// Seed for reproducing the random projection matrix.
    pub seed: u64,
}

/// Combined TurboQuant result: PolarQuant + QJL residual.
#[derive(Debug, Clone)]
pub struct TurboQuantized {
    /// PolarQuant-compressed vector.
    pub polar: QuantizedVector,
    /// QJL-compressed residual.
    pub residual: QjlBitVector,
    /// Strategy used.
    pub strategy: QuantStrategy,
}

/// Benchmark result for a single strategy run.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BenchmarkResult {
    pub strategy: String,
    pub dimensions: usize,
    pub vector_count: usize,
    pub bits: u8,
    pub compression_ratio: f64,
    pub mean_dot_distortion: f64,
    pub recall_at_10: f64,
    pub throughput_vecs_per_sec: f64,
}
