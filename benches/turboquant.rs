use criterion::{Criterion, black_box, criterion_group, criterion_main};
use maestro::turboquant::pipeline::{dot_product_with_strategy, quantize_with_strategy};
use maestro::turboquant::polar::{polar_dequantize, polar_quantize};
use maestro::turboquant::types::QuantStrategy;

fn make_vectors(count: usize, dim: usize) -> Vec<Vec<f32>> {
    let mut seed: u64 = 42;
    (0..count)
        .map(|_| {
            (0..dim)
                .map(|_| {
                    seed = seed.wrapping_mul(6364136223846793005).wrapping_add(1);
                    (seed >> 33) as f32 / (1u64 << 31) as f32 - 0.5
                })
                .collect()
        })
        .collect()
}

fn bench_polar_quantize(c: &mut Criterion) {
    let vectors = make_vectors(100, 128);
    c.bench_function("polar_quantize_128d_4bit", |b| {
        b.iter(|| {
            for v in &vectors {
                black_box(polar_quantize(v, 4));
            }
        })
    });
}

fn bench_polar_round_trip(c: &mut Criterion) {
    let vectors = make_vectors(100, 128);
    c.bench_function("polar_round_trip_128d_4bit", |b| {
        b.iter(|| {
            for v in &vectors {
                let q = polar_quantize(v, 4);
                black_box(polar_dequantize(&q));
            }
        })
    });
}

fn bench_turbo_quantize(c: &mut Criterion) {
    let vectors = make_vectors(100, 128);
    c.bench_function("turbo_quantize_128d_4bit", |b| {
        b.iter(|| {
            for v in &vectors {
                black_box(quantize_with_strategy(v, QuantStrategy::TurboQuant, 4));
            }
        })
    });
}

fn bench_turbo_dot_product(c: &mut Criterion) {
    let vectors = make_vectors(100, 128);
    let query = make_vectors(1, 128).into_iter().next().unwrap();
    let compressed: Vec<_> = vectors
        .iter()
        .map(|v| quantize_with_strategy(v, QuantStrategy::TurboQuant, 4))
        .collect();
    c.bench_function("turbo_dot_product_128d_4bit", |b| {
        b.iter(|| {
            for c in &compressed {
                black_box(dot_product_with_strategy(&query, c));
            }
        })
    });
}

criterion_group!(
    benches,
    bench_polar_quantize,
    bench_polar_round_trip,
    bench_turbo_quantize,
    bench_turbo_dot_product,
);
criterion_main!(benches);
