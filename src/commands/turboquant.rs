use crate::cli::BenchmarkOutputFormat;
use crate::turboquant::pipeline::{dot_product_with_strategy, quantize_with_strategy};
use crate::turboquant::types::{BenchmarkResult, QuantStrategy};
use rand::rngs::SmallRng;
use rand::{Rng, SeedableRng};
use std::time::Instant;

/// Run TurboQuant compression benchmarks and print a report.
pub fn cmd_turboquant_benchmark(
    dim: usize,
    count: usize,
    bits: u8,
    output: BenchmarkOutputFormat,
) -> anyhow::Result<()> {
    let strategies = [
        ("turboquant", QuantStrategy::TurboQuant),
        ("polarquant", QuantStrategy::PolarQuant),
        ("qjl", QuantStrategy::Qjl),
    ];

    // Generate random vectors
    let mut rng = SmallRng::seed_from_u64(42);
    let vectors: Vec<Vec<f32>> = (0..count)
        .map(|_| (0..dim).map(|_| rng.r#gen::<f32>() * 2.0 - 1.0).collect())
        .collect();

    // Generate queries for recall test
    let n_queries = 50.min(count);
    let queries: Vec<Vec<f32>> = (0..n_queries)
        .map(|_| (0..dim).map(|_| rng.r#gen::<f32>() * 2.0 - 1.0).collect())
        .collect();

    let mut results = Vec::new();

    for (name, strategy) in &strategies {
        let effective_bits = if *strategy == QuantStrategy::TurboQuant && bits < 2 {
            2
        } else {
            bits
        };

        // Compression benchmark
        let start = Instant::now();
        let compressed: Vec<_> = vectors
            .iter()
            .map(|v| quantize_with_strategy(v, *strategy, effective_bits))
            .collect();
        let elapsed = start.elapsed();
        let throughput = count as f64 / elapsed.as_secs_f64();

        // Compression ratio (approximate)
        let original_bytes = count * dim * 4; // f32 = 4 bytes
        let compressed_bits_per_vec = match strategy {
            QuantStrategy::TurboQuant => {
                32 + (dim as u64 * effective_bits as u64) + dim as u64 // polar + qjl
            }
            QuantStrategy::PolarQuant => {
                32 + dim as u64 * effective_bits as u64 // scale + codes
            }
            QuantStrategy::Qjl => dim as u64, // 1 bit per dimension
        };
        let compressed_bytes = (count as u64 * compressed_bits_per_vec).div_ceil(8);
        let compression_ratio = original_bytes as f64 / compressed_bytes.max(1) as f64;

        // Dot product distortion
        let mut total_distortion = 0.0f64;
        let sample_size = 100.min(count);
        for i in 0..sample_size {
            let query = &queries[i % n_queries];
            let exact: f32 = vectors[i].iter().zip(query.iter()).map(|(a, b)| a * b).sum();
            let estimated = dot_product_with_strategy(query, &compressed[i]);
            let error = (estimated - exact).abs() as f64;
            total_distortion += error;
        }
        let mean_distortion = total_distortion / sample_size as f64;

        // Recall@10
        let k = 10.min(count);
        let mut total_recall = 0.0;
        for query in &queries {
            let mut exact_dots: Vec<(usize, f32)> = vectors
                .iter()
                .enumerate()
                .map(|(i, v)| {
                    let dot: f32 = query.iter().zip(v.iter()).map(|(a, b)| a * b).sum();
                    (i, dot)
                })
                .collect();
            exact_dots.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap());
            let exact_top_k: Vec<usize> = exact_dots.iter().take(k).map(|(i, _)| *i).collect();

            let mut est_dots: Vec<(usize, f32)> = compressed
                .iter()
                .enumerate()
                .map(|(i, c)| (i, dot_product_with_strategy(query, c)))
                .collect();
            est_dots.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap());
            let est_top_k: Vec<usize> = est_dots.iter().take(k).map(|(i, _)| *i).collect();

            let overlap = exact_top_k
                .iter()
                .filter(|i| est_top_k.contains(i))
                .count();
            total_recall += overlap as f64 / k as f64;
        }
        let recall_at_10 = total_recall / n_queries as f64;

        results.push(BenchmarkResult {
            strategy: name.to_string(),
            dimensions: dim,
            vector_count: count,
            bits: effective_bits,
            compression_ratio,
            mean_dot_distortion: mean_distortion,
            recall_at_10,
            throughput_vecs_per_sec: throughput,
        });
    }

    if output == BenchmarkOutputFormat::Json {
        let json = serde_json::to_string_pretty(&results)?;
        println!("{json}");
    } else {
        print_table(&results);
    }

    Ok(())
}

fn print_table(results: &[BenchmarkResult]) {
    println!("TurboQuant Benchmark Report");
    println!("{}", "=".repeat(80));
    println!(
        "{:<14} {:>5} {:>8} {:>12} {:>10} {:>12}",
        "Strategy", "Bits", "Ratio", "Distortion", "Recall@10", "Vec/sec"
    );
    println!("{}", "-".repeat(80));

    for r in results {
        println!(
            "{:<14} {:>5} {:>8.2}x {:>12.6} {:>10.4} {:>12.0}",
            r.strategy,
            r.bits,
            r.compression_ratio,
            r.mean_dot_distortion,
            r.recall_at_10,
            r.throughput_vecs_per_sec,
        );
    }

    println!("{}", "-".repeat(80));
    if let Some(first) = results.first() {
        println!(
            "Vectors: {} × {}d",
            first.vector_count, first.dimensions
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn benchmark_produces_results_for_all_strategies() {
        // Use small dimensions for fast test
        let dim = 16;
        let count = 20;
        let bits = 4;

        let mut rng = SmallRng::seed_from_u64(42);
        let vectors: Vec<Vec<f32>> = (0..count)
            .map(|_| (0..dim).map(|_| rng.r#gen::<f32>() * 2.0 - 1.0).collect())
            .collect();

        // Just verify the strategies all produce valid results
        for strategy in [QuantStrategy::TurboQuant, QuantStrategy::PolarQuant, QuantStrategy::Qjl] {
            let effective_bits = if strategy == QuantStrategy::TurboQuant && bits < 2 { 2 } else { bits };
            let compressed: Vec<_> = vectors
                .iter()
                .map(|v| quantize_with_strategy(v, strategy, effective_bits))
                .collect();
            assert_eq!(compressed.len(), count);
        }
    }

    #[test]
    fn json_output_is_valid() {
        let results = vec![BenchmarkResult {
            strategy: "turboquant".into(),
            dimensions: 768,
            vector_count: 100,
            bits: 4,
            compression_ratio: 3.5,
            mean_dot_distortion: 0.01,
            recall_at_10: 0.95,
            throughput_vecs_per_sec: 50000.0,
        }];
        let json = serde_json::to_string_pretty(&results).unwrap();
        let parsed: Vec<BenchmarkResult> = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.len(), 1);
        assert_eq!(parsed[0].strategy, "turboquant");
    }
}
