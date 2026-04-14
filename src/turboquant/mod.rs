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
