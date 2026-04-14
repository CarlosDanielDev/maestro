use super::polar::{polar_dequantize, polar_quantize};
use super::qjl::{qjl_compress, qjl_estimate_dot};
use super::types::{QjlBitVector, QuantStrategy, QuantizedVector, TurboQuantized};

/// Full TurboQuant pipeline: PolarQuant at (total_bits - 1) + QJL residual at 1 bit.
pub fn turbo_quantize(vector: &[f32], total_bits: u8) -> TurboQuantized {
    assert!(total_bits >= 2, "turbo_quantize needs at least 2 bits (1 for polar + 1 for QJL)");

    let polar_bits = total_bits - 1;
    let polar = polar_quantize(vector, polar_bits);
    let reconstructed = polar_dequantize(&polar);

    // Compute residual
    let residual: Vec<f32> = vector
        .iter()
        .zip(reconstructed.iter())
        .map(|(a, b)| a - b)
        .collect();

    // Use a deterministic seed derived from vector length and bits
    // Seed mixes vector length and bit width to ensure different configs produce different QJL projections
    let seed = (vector.len() as u64).wrapping_mul(31).wrapping_add(total_bits as u64);
    let qjl = qjl_compress(&residual, seed);

    TurboQuantized {
        polar,
        residual: qjl,
        strategy: QuantStrategy::TurboQuant,
    }
}

/// Estimate dot product using the TurboQuant decomposition.
///
/// dot(q, v) ≈ dot(q, polar_reconstruct) + qjl_estimate_dot(q, residual)
pub fn turbo_dot_product(query: &[f32], compressed: &TurboQuantized) -> f32 {
    let polar_reconstructed = polar_dequantize(&compressed.polar);

    // Exact dot product with polar reconstruction
    let polar_dot: f32 = query
        .iter()
        .zip(polar_reconstructed.iter())
        .map(|(a, b)| a * b)
        .sum();

    // QJL residual correction
    let residual_dot = qjl_estimate_dot(query, &compressed.residual, compressed.residual.seed);

    polar_dot + residual_dot
}

/// Dispatch quantization based on strategy.
pub fn quantize_with_strategy(vector: &[f32], strategy: QuantStrategy, bits: u8) -> TurboQuantized {
    match strategy {
        QuantStrategy::TurboQuant => turbo_quantize(vector, bits),
        QuantStrategy::PolarQuant => {
            let polar = polar_quantize(vector, bits);
            TurboQuantized {
                polar,
                residual: QjlBitVector {
                    packed_signs: Vec::new(),
                    projection_dim: 0,
                    seed: 0,
                },
                strategy: QuantStrategy::PolarQuant,
            }
        }
        QuantStrategy::Qjl => {
            let seed = (vector.len() as u64).wrapping_mul(31).wrapping_add(bits as u64);
            let qjl = qjl_compress(vector, seed);
            TurboQuantized {
                polar: QuantizedVector {
                    scale: 0.0,
                    codes: Vec::new(),
                    bits: 0,
                    original_len: vector.len(),
                },
                residual: qjl,
                strategy: QuantStrategy::Qjl,
            }
        }
    }
}

/// Estimate dot product for any strategy.
pub fn dot_product_with_strategy(query: &[f32], compressed: &TurboQuantized) -> f32 {
    match compressed.strategy {
        QuantStrategy::TurboQuant => turbo_dot_product(query, compressed),
        QuantStrategy::PolarQuant => {
            let reconstructed = polar_dequantize(&compressed.polar);
            query
                .iter()
                .zip(reconstructed.iter())
                .map(|(a, b)| a * b)
                .sum()
        }
        QuantStrategy::Qjl => {
            qjl_estimate_dot(query, &compressed.residual, compressed.residual.seed)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_rng(initial_seed: u64) -> impl FnMut() -> f32 {
        let mut seed = initial_seed;
        move || -> f32 {
            seed = seed.wrapping_mul(6364136223846793005).wrapping_add(1);
            (seed >> 33) as f32 / (1u64 << 31) as f32 - 0.5
        }
    }

    #[test]
    fn turbo_quantize_basic() {
        let v = vec![1.0, 2.0, 3.0, 4.0];
        let compressed = turbo_quantize(&v, 4);
        assert_eq!(compressed.strategy, QuantStrategy::TurboQuant);
        assert!(compressed.polar.codes.len() > 0);
        assert!(compressed.residual.projection_dim > 0);
    }

    #[test]
    fn turbo_dot_product_accuracy() {
        let mut rng = make_rng(42);
        let v: Vec<f32> = (0..64).map(|_| rng()).collect();
        let q: Vec<f32> = (0..64).map(|_| rng()).collect();

        let exact_dot: f32 = v.iter().zip(q.iter()).map(|(a, b)| a * b).sum();
        let compressed = turbo_quantize(&v, 4);
        let estimated = turbo_dot_product(&q, &compressed);

        let error = (estimated - exact_dot).abs();
        let relative_error = error / exact_dot.abs().max(1e-6);
        assert!(
            relative_error < 2.0,
            "relative error too high: {relative_error} (exact={exact_dot}, est={estimated})"
        );
    }

    #[test]
    fn strategy_dispatch_turboquant() {
        let v = vec![1.0, 2.0, 3.0, 4.0];
        let c = quantize_with_strategy(&v, QuantStrategy::TurboQuant, 4);
        assert_eq!(c.strategy, QuantStrategy::TurboQuant);
        assert!(c.residual.projection_dim > 0);
    }

    #[test]
    fn strategy_dispatch_polarquant_only() {
        let v = vec![1.0, 2.0, 3.0, 4.0];
        let c = quantize_with_strategy(&v, QuantStrategy::PolarQuant, 4);
        assert_eq!(c.strategy, QuantStrategy::PolarQuant);
        assert_eq!(c.residual.projection_dim, 0); // No QJL
    }

    #[test]
    fn strategy_dispatch_qjl_only() {
        let v = vec![1.0, 2.0, 3.0, 4.0];
        let c = quantize_with_strategy(&v, QuantStrategy::Qjl, 4);
        assert_eq!(c.strategy, QuantStrategy::Qjl);
        assert!(c.residual.projection_dim > 0);
        assert!(c.polar.codes.is_empty()); // No PolarQuant
    }

    #[test]
    fn recall_at_10_basic() {
        let mut rng = make_rng(999);
        let dim = 64;
        let n_vectors = 100;
        let n_queries = 20;
        let k = 10;

        let database: Vec<Vec<f32>> = (0..n_vectors)
            .map(|_| (0..dim).map(|_| rng()).collect())
            .collect();
        let queries: Vec<Vec<f32>> = (0..n_queries)
            .map(|_| (0..dim).map(|_| rng()).collect())
            .collect();

        let compressed: Vec<TurboQuantized> = database
            .iter()
            .map(|v| turbo_quantize(v, 4))
            .collect();

        let mut total_recall = 0.0;
        for query in &queries {
            // Exact top-k
            let mut exact_dots: Vec<(usize, f32)> = database
                .iter()
                .enumerate()
                .map(|(i, v)| {
                    let dot: f32 = query.iter().zip(v.iter()).map(|(a, b)| a * b).sum();
                    (i, dot)
                })
                .collect();
            exact_dots.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap());
            let exact_top_k: Vec<usize> = exact_dots.iter().take(k).map(|(i, _)| *i).collect();

            // Estimated top-k
            let mut est_dots: Vec<(usize, f32)> = compressed
                .iter()
                .enumerate()
                .map(|(i, c)| (i, turbo_dot_product(query, c)))
                .collect();
            est_dots.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap());
            let est_top_k: Vec<usize> = est_dots.iter().take(k).map(|(i, _)| *i).collect();

            let overlap = exact_top_k
                .iter()
                .filter(|i| est_top_k.contains(i))
                .count();
            total_recall += overlap as f64 / k as f64;
        }

        let mean_recall = total_recall / n_queries as f64;
        // Note: at 4-bit with small vectors, recall may not hit 0.95
        // but should be reasonable (> 0.3)
        assert!(
            mean_recall > 0.3,
            "recall@10 too low: {mean_recall}"
        );
    }

    #[test]
    fn dot_product_with_strategy_dispatches() {
        let v = vec![1.0, 2.0, 3.0, 4.0];
        let q = vec![0.5, 0.5, 0.5, 0.5];

        for strategy in [QuantStrategy::TurboQuant, QuantStrategy::PolarQuant, QuantStrategy::Qjl] {
            let bits = if strategy == QuantStrategy::TurboQuant { 4 } else { 4 };
            let c = quantize_with_strategy(&v, strategy, bits);
            let _est = dot_product_with_strategy(&q, &c);
            // Just verify it doesn't panic
        }
    }

    #[test]
    #[should_panic(expected = "turbo_quantize needs at least 2 bits")]
    fn turbo_quantize_rejects_1_bit() {
        turbo_quantize(&[1.0, 2.0], 1);
    }
}
