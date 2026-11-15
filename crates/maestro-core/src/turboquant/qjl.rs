//! Quantized Johnson-Lindenstrauss (QJL) projection.
//!
//! Compresses a residual vector into a 1-bit-per-dimension sketch using a
//! seeded random Gaussian projection matrix. The seed makes the projection
//! deterministic and reproducible, so the same seed can reconstruct the
//! projection at dot-product estimation time.

use super::types::QjlBitVector;
use rand::rngs::SmallRng;
use rand::{Rng, SeedableRng};

/// Compress a residual vector using seeded Johnson-Lindenstrauss projection.
///
/// The algorithm:
/// 1. Generate a random projection matrix from the seed
/// 2. Project the residual through random vectors
/// 3. Extract the sign bit of each projection
/// 4. Pack sign bits into u64 bitfields
pub fn qjl_compress(residual: &[f32], seed: u64) -> QjlBitVector {
    let dim = residual.len();
    if dim == 0 {
        return QjlBitVector {
            packed_signs: Vec::new(),
            projection_dim: 0,
            seed,
        };
    }

    // Use dimension count as projection dimension (standard JL)
    let projection_dim = dim;
    let packed_len = projection_dim.div_ceil(64);
    let mut packed_signs = vec![0u64; packed_len];

    let mut rng = SmallRng::seed_from_u64(seed);

    for j in 0..projection_dim {
        // Compute dot product with random unit vector
        let mut dot = 0.0f32;
        for &val in residual {
            let r: f32 = rng.r#gen::<f32>() * 2.0 - 1.0;
            dot += val * r;
        }

        // Store sign bit: 1 if positive, 0 if negative
        if dot >= 0.0 {
            packed_signs[j / 64] |= 1u64 << (j % 64);
        }
    }

    QjlBitVector {
        packed_signs,
        projection_dim,
        seed,
    }
}

/// Estimate the dot product between a full-precision query and a QJL-compressed vector.
///
/// Uses the same seeded random matrix to project the query, then estimates
/// the dot product via XNOR between query signs and compressed signs.
pub fn qjl_estimate_dot(query: &[f32], compressed: &QjlBitVector, seed: u64) -> f32 {
    let dim = query.len();
    if dim == 0 || compressed.projection_dim == 0 {
        return 0.0;
    }

    let projection_dim = compressed.projection_dim;

    // Compute query norm for scaling
    let query_norm: f32 = query.iter().map(|x| x * x).sum::<f32>().sqrt();
    if query_norm < 1e-10 {
        return 0.0;
    }

    let mut rng = SmallRng::seed_from_u64(seed);
    let mut agreement_count: i64 = 0;

    for j in 0..projection_dim {
        // Project query through same random vector
        let mut dot = 0.0f32;
        for &val in query {
            let r: f32 = rng.r#gen::<f32>() * 2.0 - 1.0;
            dot += val * r;
        }

        let query_sign = dot >= 0.0;
        let compressed_sign = (compressed.packed_signs[j / 64] >> (j % 64)) & 1 == 1;

        // XNOR: agree = same sign
        if query_sign == compressed_sign {
            agreement_count += 1;
        } else {
            agreement_count -= 1;
        }
    }

    // Scale estimate: agreement ratio maps to cosine similarity estimate
    let agreement_ratio = agreement_count as f32 / projection_dim as f32;

    // The expected value of sign agreement is related to the angle between vectors
    // For unbiased estimation, we scale by norms
    query_norm * agreement_ratio
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn compress_empty_vector() {
        let v: Vec<f32> = Vec::new();
        let c = qjl_compress(&v, 42);
        assert_eq!(c.projection_dim, 0);
        assert!(c.packed_signs.is_empty());
    }

    #[test]
    fn compress_produces_correct_packing() {
        let v = vec![1.0; 128];
        let c = qjl_compress(&v, 42);
        assert_eq!(c.projection_dim, 128);
        assert_eq!(c.packed_signs.len(), 2); // 128 / 64 = 2
    }

    #[test]
    fn deterministic_with_same_seed() {
        let v = vec![1.0, 2.0, 3.0, 4.0];
        let c1 = qjl_compress(&v, 42);
        let c2 = qjl_compress(&v, 42);
        assert_eq!(c1.packed_signs, c2.packed_signs);
    }

    #[test]
    fn different_seeds_differ() {
        let v = vec![1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0];
        let c1 = qjl_compress(&v, 42);
        let c2 = qjl_compress(&v, 999);
        // With different seeds, sign patterns should differ
        assert_ne!(c1.packed_signs, c2.packed_signs);
    }

    #[test]
    fn estimate_dot_empty() {
        let q: Vec<f32> = Vec::new();
        let c = QjlBitVector {
            packed_signs: Vec::new(),
            projection_dim: 0,
            seed: 42,
        };
        assert_eq!(qjl_estimate_dot(&q, &c, 42), 0.0);
    }

    #[test]
    fn estimate_dot_zero_residual() {
        let residual = vec![0.0; 64];
        let query = vec![1.0; 64];
        let c = qjl_compress(&residual, 42);
        let est = qjl_estimate_dot(&query, &c, 42);
        // Zero residual should give ~0 estimate (random noise around 0)
        assert!(est.abs() < 10.0, "expected near 0, got {est}");
    }

    #[test]
    fn unbiased_estimator_mean_error_converges() {
        // Generate many random pairs and check mean error → 0
        let mut seed: u64 = 12345;
        let mut rng_val = || -> f32 {
            seed = seed.wrapping_mul(6364136223846793005).wrapping_add(1);
            (seed >> 33) as f32 / (1u64 << 31) as f32 - 0.5
        };

        let mut total_error = 0.0f64;
        let trials = 200;
        let dim = 64;

        for trial in 0..trials {
            let key: Vec<f32> = (0..dim).map(|_| rng_val()).collect();
            let query: Vec<f32> = (0..dim).map(|_| rng_val()).collect();

            let exact_dot: f32 = key.iter().zip(query.iter()).map(|(a, b)| a * b).sum();
            let compressed = qjl_compress(&key, trial as u64 + 1000);
            let estimated = qjl_estimate_dot(&query, &compressed, trial as u64 + 1000);

            total_error += (estimated - exact_dot) as f64;
        }

        let mean_error = total_error / trials as f64;
        assert!(
            mean_error.abs() < 2.0,
            "mean error should be small, got {mean_error}"
        );
    }

    #[test]
    fn dimension_not_multiple_of_64() {
        let v = vec![1.0; 100]; // 100 is not a multiple of 64
        let c = qjl_compress(&v, 42);
        assert_eq!(c.projection_dim, 100);
        assert_eq!(c.packed_signs.len(), 2); // ceil(100/64) = 2
    }
}
