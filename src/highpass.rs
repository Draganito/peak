//! A high-pass added back onto the print. The blur is one pixel, so the
//! lift is still rising at the pixel itself.
//!
//! `out = x + amount · (x − blur(σ))`. A flat field is unchanged. The amount
//! is solved on the discrete kernel, so half a cycle per pixel lands on
//! [`NYQUIST_GAIN`].

use std::f64::consts::PI;

/// Blur radius, in pixels of the file.
pub const SIGMA_PX: f64 = 0.7;
/// Where a measured edge should land, in cycles per pixel. 0.35 is seventy
/// percent of the pixel. Past that the edge reads as oversharpened.
pub const TARGET_MTF50: f64 = 0.35;
/// Contrast at a quarter cycle per pixel. The rim appears above 0.80.
/// The search stops a tenth earlier.
pub const M25_CAP: f64 = 0.70;
/// Highest lift the search will try. 2.20 is one hundred and twenty percent.
pub const FALLBACK_GAIN: f64 = 2.20;

#[derive(Clone, Debug)]
pub struct Highpass {
    pub sigma: f64,
    pub amount: f64,
    /// Contrast at half a cycle per pixel, on the kernel that is applied.
    pub gain: f64,
}

/// The same weights `mask` blurs with: a sampled Gaussian, cut at three sigma.
fn kernel(sigma: f64) -> Vec<f64> {
    let r = (3.0 * sigma).ceil().max(1.0) as usize;
    let mut k = Vec::with_capacity(2 * r + 1);
    let s2 = sigma * sigma;
    for i in 0..=2 * r {
        let t = i as f64 - r as f64;
        k.push((-0.5 * t * t / s2).exp());
    }
    let sum: f64 = k.iter().sum();
    for v in &mut k {
        *v /= sum;
    }
    k
}

/// Gain of one blur at `freq` cycles per pixel.
fn blurred(freq: f64, sigma: f64) -> f64 {
    let k = kernel(sigma);
    let r = k.len() / 2;
    k.iter()
        .enumerate()
        .map(|(i, w)| w * (2.0 * PI * freq * (i as f64 - r as f64)).cos())
        .sum()
}

/// `1 + amount · (1 − blur)` at `freq` cycles per pixel, on the discrete kernel.
pub fn gain(freq: f64, sigma: f64, amount: f64) -> f64 {
    1.0 + amount * (1.0 - blurred(freq, sigma))
}

/// The mask for one chosen lift at the standard radius. `nyquist_gain` is the
/// contrast at half a cycle per pixel, 1.20 for twenty percent.
pub fn for_gain(nyquist_gain: f64) -> Highpass {
    for_sigma_gain(SIGMA_PX, nyquist_gain)
}

/// The mask for one radius and one lift. `nyquist_gain` is the contrast at
/// half a cycle per pixel.
pub fn for_sigma_gain(sigma: f64, nyquist_gain: f64) -> Highpass {
    let pass = 1.0 - blurred(0.5, sigma);
    let amount = (nyquist_gain - 1.0) / pass.max(1e-6);
    Highpass {
        sigma,
        amount,
        gain: gain(0.5, sigma, amount),
    }
}

/// The mask at the top of the search. The kernel tests use it.
#[cfg(test)]
pub fn design() -> Highpass {
    for_gain(FALLBACK_GAIN)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_lift_sits_on_the_pixel() {
        let h = design();
        let at_pixel = gain(0.5, h.sigma, h.amount);
        let coarser = gain(0.05, h.sigma, h.amount);
        assert!((at_pixel - FALLBACK_GAIN).abs() < 1e-9, "{at_pixel}");
        assert!(at_pixel > coarser, "pixel {at_pixel} coarse {coarser}");
        assert!(
            coarser - 1.0 < 0.05 * (at_pixel - 1.0),
            "coarse {coarser} pixel {at_pixel}"
        );
    }
}
