use super::types::QuantizedVector;
use std::f32::consts::PI;

/// Quantize a vector using recursive polar decomposition.
///
/// The algorithm:
/// 1. Pad vector to next power of 2 if needed
/// 2. Pair adjacent elements: (x[0], x[1]), (x[2], x[3]), ...
/// 3. Convert each pair to polar: r = sqrt(a² + b²), θ = atan2(b, a)
/// 4. Quantize each θ to nearest value in 2^bits uniform grid over [-π, π]
/// 5. Collect radii as new vector, recurse
/// 6. Final single element becomes the global scale
pub fn polar_quantize(vector: &[f32], bits: u8) -> QuantizedVector {
    assert!((1..=8).contains(&bits), "bits must be 1-8");

    let original_len = vector.len();
    if original_len == 0 {
        return QuantizedVector {
            scale: 0.0,
            codes: Vec::new(),
            bits,
            original_len: 0,
        };
    }
    if original_len == 1 {
        return QuantizedVector {
            scale: vector[0],
            codes: Vec::new(),
            bits,
            original_len: 1,
        };
    }

    // Pad to even length
    let mut current: Vec<f32> = vector.to_vec();
    if !current.len().is_multiple_of(2) {
        current.push(0.0);
    }

    let levels = (current.len() as f64).log2().ceil() as usize;
    let grid_size = 1u16 << bits; // 2^bits
    let mut all_codes = Vec::new();

    for _ in 0..levels {
        if current.len() <= 1 {
            break;
        }

        // Pad to even if needed at this level
        if !current.len().is_multiple_of(2) {
            current.push(0.0);
        }

        let pair_count = current.len() / 2;
        let mut radii = Vec::with_capacity(pair_count);
        let mut level_codes = Vec::with_capacity(pair_count);

        for i in 0..pair_count {
            let a = current[2 * i];
            let b = current[2 * i + 1];
            let r = (a * a + b * b).sqrt();
            let theta = b.atan2(a); // [-π, π]

            // Quantize θ to uniform grid
            let normalized = (theta + PI) / (2.0 * PI); // [0, 1)
            let code = (normalized * grid_size as f32).round() as u16 % grid_size;
            level_codes.push(code);
            radii.push(r);
        }

        all_codes.extend(level_codes);
        current = radii;
    }

    let scale = if current.is_empty() {
        0.0
    } else {
        current[0]
    };

    QuantizedVector {
        scale,
        codes: all_codes,
        bits,
        original_len,
    }
}

/// Reconstruct an approximate vector from a PolarQuant encoding.
pub fn polar_dequantize(qvec: &QuantizedVector) -> Vec<f32> {
    if qvec.original_len == 0 {
        return Vec::new();
    }
    if qvec.original_len == 1 {
        return vec![qvec.scale];
    }

    let grid_size = 1u16 << qvec.bits;

    // Determine the structure: figure out how many codes per level
    let mut padded_len = qvec.original_len;
    if !padded_len.is_multiple_of(2) {
        padded_len += 1;
    }

    let mut level_sizes = Vec::new();
    let mut len = padded_len;
    while len > 1 {
        if !len.is_multiple_of(2) {
            len += 1;
        }
        level_sizes.push(len / 2);
        len /= 2;
    }

    // Reconstruct from top (scale) down through each level
    let mut current = vec![qvec.scale];

    let mut code_offset = qvec.codes.len();
    for &level_size in level_sizes.iter().rev() {
        code_offset -= level_size;
        let codes = &qvec.codes[code_offset..code_offset + level_size];

        let mut expanded = Vec::with_capacity(level_size * 2);
        for (i, &code) in codes.iter().enumerate() {
            let r = if i < current.len() { current[i] } else { 0.0 };
            let theta = (code as f32 / grid_size as f32) * 2.0 * PI - PI;
            let a = r * theta.cos();
            let b = r * theta.sin();
            expanded.push(a);
            expanded.push(b);
        }
        current = expanded;
    }

    current.truncate(qvec.original_len);
    current
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trip_zero_vector() {
        let v = vec![0.0; 8];
        let q = polar_quantize(&v, 4);
        let r = polar_dequantize(&q);
        assert_eq!(r.len(), 8);
        for val in &r {
            assert!(val.abs() < 1e-6, "expected ~0, got {val}");
        }
    }

    #[test]
    fn round_trip_single_element() {
        let v = vec![3.14];
        let q = polar_quantize(&v, 4);
        let r = polar_dequantize(&q);
        assert_eq!(r.len(), 1);
        assert!((r[0] - 3.14).abs() < 1e-6);
    }

    #[test]
    fn round_trip_empty_vector() {
        let v: Vec<f32> = Vec::new();
        let q = polar_quantize(&v, 4);
        let r = polar_dequantize(&q);
        assert!(r.is_empty());
    }

    #[test]
    fn round_trip_unit_vector() {
        let v = vec![1.0, 0.0];
        let q = polar_quantize(&v, 8);
        let r = polar_dequantize(&q);
        assert_eq!(r.len(), 2);
        assert!((r[0] - 1.0).abs() < 0.05, "got {}", r[0]);
        assert!(r[1].abs() < 0.05, "got {}", r[1]);
    }

    #[test]
    fn scale_equals_l2_norm() {
        let v = vec![3.0, 4.0];
        let q = polar_quantize(&v, 8);
        assert!((q.scale - 5.0).abs() < 1e-5, "scale should be L2 norm");
    }

    #[test]
    fn round_trip_128d_random_within_tolerance() {
        // Seeded deterministic "random" using simple LCG
        let mut seed: u64 = 42;
        let mut rng = || -> f32 {
            seed = seed.wrapping_mul(6364136223846793005).wrapping_add(1);
            (seed >> 33) as f32 / (1u64 << 31) as f32 - 0.5
        };

        let v: Vec<f32> = (0..128).map(|_| rng()).collect();
        let q = polar_quantize(&v, 4);
        let r = polar_dequantize(&q);
        assert_eq!(r.len(), 128);

        // Compute MSE
        let mse: f32 = v
            .iter()
            .zip(r.iter())
            .map(|(a, b)| (a - b) * (a - b))
            .sum::<f32>()
            / v.len() as f32;
        assert!(mse < 0.1, "MSE too high: {mse}");
    }

    #[test]
    fn odd_length_vector_round_trips() {
        let v = vec![1.0, 2.0, 3.0];
        let q = polar_quantize(&v, 4);
        let r = polar_dequantize(&q);
        assert_eq!(r.len(), 3);
    }

    #[test]
    fn different_bit_widths_work() {
        let v = vec![1.0, 2.0, 3.0, 4.0];
        for bits in 1..=8 {
            let q = polar_quantize(&v, bits);
            let r = polar_dequantize(&q);
            assert_eq!(r.len(), 4, "failed for bits={bits}");
        }
    }

    #[test]
    fn deterministic_output() {
        let v = vec![1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0];
        let q1 = polar_quantize(&v, 4);
        let q2 = polar_quantize(&v, 4);
        assert_eq!(q1.scale, q2.scale);
        assert_eq!(q1.codes, q2.codes);
    }

    #[test]
    #[should_panic(expected = "bits must be 1-8")]
    fn bits_zero_panics() {
        polar_quantize(&[1.0, 2.0], 0);
    }

    #[test]
    #[should_panic(expected = "bits must be 1-8")]
    fn bits_nine_panics() {
        polar_quantize(&[1.0, 2.0], 9);
    }

    #[test]
    fn higher_bits_gives_lower_distortion() {
        let mut seed: u64 = 123;
        let mut rng = || -> f32 {
            seed = seed.wrapping_mul(6364136223846793005).wrapping_add(1);
            (seed >> 33) as f32 / (1u64 << 31) as f32 - 0.5
        };
        let v: Vec<f32> = (0..64).map(|_| rng()).collect();

        let mse_low = {
            let r = polar_dequantize(&polar_quantize(&v, 2));
            v.iter()
                .zip(r.iter())
                .map(|(a, b)| (a - b) * (a - b))
                .sum::<f32>()
                / v.len() as f32
        };
        let mse_high = {
            let r = polar_dequantize(&polar_quantize(&v, 8));
            v.iter()
                .zip(r.iter())
                .map(|(a, b)| (a - b) * (a - b))
                .sum::<f32>()
                / v.len() as f32
        };
        assert!(
            mse_high <= mse_low,
            "8-bit MSE ({mse_high}) should be <= 2-bit MSE ({mse_low})"
        );
    }
}
