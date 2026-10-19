//! Separable Gaussian mask. A flat field is unchanged, because both blurs
//! of a constant are the constant and their difference is zero.

/// Reflect a sample index. `-1` lands on `1`, `n` lands on `n - 2`.
fn reflect(mut i: isize, n: isize) -> isize {
    if n <= 1 {
        return 0;
    }
    loop {
        if i < 0 {
            i = -i;
        } else if i >= n {
            i = 2 * n - 2 - i;
        } else {
            return i;
        }
    }
}

fn kernel(sigma: f64) -> Vec<f32> {
    let r = (3.0 * sigma).ceil().max(1.0) as usize;
    let mut k = Vec::with_capacity(2 * r + 1);
    let s2 = (sigma * sigma) as f32;
    for i in 0..=2 * r {
        let t = i as f32 - r as f32;
        k.push((-0.5 * t * t / s2).exp());
    }
    let sum: f32 = k.iter().sum();
    for v in &mut k {
        *v /= sum;
    }
    k
}

fn blur_axis(
    src: &[f32],
    dst: &mut [f32],
    width: usize,
    height: usize,
    horizontal: bool,
    k: &[f32],
) {
    let r = (k.len() / 2) as isize;
    if horizontal {
        for y in 0..height {
            let row = y * width;
            for x in 0..width {
                let mut acc = 0.0f32;
                for (t, &w) in k.iter().enumerate() {
                    let i = reflect(x as isize + t as isize - r, width as isize) as usize;
                    acc += w * src[row + i];
                }
                dst[row + x] = acc;
            }
        }
    } else {
        for y in 0..height {
            for x in 0..width {
                let mut acc = 0.0f32;
                for (t, &w) in k.iter().enumerate() {
                    let i = reflect(y as isize + t as isize - r, height as isize) as usize;
                    acc += w * src[i * width + x];
                }
                dst[y * width + x] = acc;
            }
        }
    }
}

fn blur(src: &[f32], width: usize, height: usize, sigma: f64) -> Vec<f32> {
    let k = kernel(sigma);
    let mut tmp = vec![0.0f32; src.len()];
    let mut out = vec![0.0f32; src.len()];
    blur_axis(src, &mut tmp, width, height, true, &k);
    blur_axis(&tmp, &mut out, width, height, false, &k);
    out
}

/// `out = x + amount · (x − blur(σ))`, in the code values of the sheet.
pub fn apply(sheet: &[f32], width: usize, height: usize, sigma: f64, amount: f64) -> Vec<f32> {
    let blurred = blur(sheet, width, height, sigma);
    blurred
        .iter()
        .zip(sheet)
        .map(|(&b, &x)| x + amount as f32 * (x - b))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::highpass::{design, gain, FALLBACK_GAIN};

    #[test]
    fn a_flat_sheet_is_unchanged() {
        let sheet = vec![0.4f32; 48 * 32];
        let out = apply(&sheet, 48, 32, 0.7, 0.5);
        for (a, b) in out.iter().zip(&sheet) {
            assert!((a - b).abs() < 1e-5, "{a} {b}");
        }
    }

    #[test]
    fn a_sine_at_the_pixel_is_lifted() {
        let h = design();
        let (w, height) = (256, 48);
        let mut sheet = vec![0.0f32; w * height];
        let freq = 0.5;
        for x in 0..w {
            let v = (2.0 * std::f64::consts::PI * freq * x as f64).cos();
            for y in 0..height {
                sheet[y * w + x] = (0.5 + 0.2 * v) as f32;
            }
        }
        let out = apply(&sheet, w, height, h.sigma, h.amount);
        let y = height / 2;
        let sl = 32..w - 32;
        let mut num = 0.0f64;
        let mut den = 0.0f64;
        for x in sl {
            let src = sheet[y * w + x] as f64 - 0.5;
            let got = out[y * w + x] as f64 - 0.5;
            num += got * src;
            den += src * src;
        }
        let measured = num / den;
        assert!(
            (measured - FALLBACK_GAIN as f64).abs() < 0.01,
            "measured {measured}"
        );
        let predicted = gain(freq, h.sigma, h.amount);
        assert!(
            (measured - predicted).abs() < 0.01,
            "measured {measured} predicted {predicted}"
        );
    }
}
