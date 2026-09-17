#![feature(portable_simd)]

use std::simd::Simd;

pub fn apply_gain_8(samples: [f32; 8], gain: f32) -> [f32; 8] {
    let frame = Simd::<f32, 8>::from_array(samples);
    let gain = Simd::<f32, 8>::splat(gain);
    (frame * gain).to_array()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn simd_matches_scalar_gain() {
        let input = [0.0, 0.1, -0.2, 0.5, -0.75, 1.0, -1.0, 0.25];
        let got = apply_gain_8(input, 0.5);
        let expected = input.map(|sample| sample * 0.5);
        assert_eq!(got, expected);
    }
}
