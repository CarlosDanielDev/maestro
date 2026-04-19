//! TurboQuant — vector quantization for context compression.
//!
//! Combines **PolarQuant** (recursive polar decomposition) with a
//! **QJL** (Quantized Johnson-Lindenstrauss) residual sketch to compress
//! high-dimensional embedding vectors into compact bit representations
//! while preserving dot-product similarity.
//!
//! The pipeline is gated by the `TurboQuant` feature flag and invoked
//! during session context overflow to compress token embeddings before
//! re-injection.
//!
//! ## Pipeline stages
//!
//! 1. **PolarQuant** (`polar.rs`) — encodes the direction via recursive
//!    polar angle quantization at `total_bits - 1` bits.
//! 2. **QJL** (`qjl.rs`) — sketches the residual using a seeded random
//!    projection at 1 bit per dimension.
//! 3. **Combine** (`pipeline.rs`) — merges both representations and
//!    provides dot-product estimation for similarity search.

pub mod adapter;
pub mod budget;
pub mod pipeline;
pub mod polar;
pub mod qjl;
pub mod types;

// Re-exports for the library crate (benchmarks, external consumers).
// In the binary crate these are accessed via submodule paths directly.
#[allow(unused_imports)]
pub use pipeline::{
    dot_product_with_strategy, quantize_with_strategy, turbo_dot_product, turbo_quantize,
};
#[allow(unused_imports)]
pub use polar::{polar_dequantize, polar_quantize};
#[allow(unused_imports)]
pub use qjl::{qjl_compress, qjl_estimate_dot};
#[allow(unused_imports)]
pub use types::*;
