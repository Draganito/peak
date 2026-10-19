//! Sharpen one print. The command line and the window both call this.

use crate::highpass::Highpass;
use crate::sheet::Print;
use crate::tune;
use anyhow::Result;
use std::path::{Path, PathBuf};

pub struct Outcome {
    pub samples: Vec<f32>,
    pub description: String,
    /// What the command line prints, apart from the path that was written.
    pub lines: Vec<String>,
}

pub fn compute(print: &Print) -> Outcome {
    let frame = frame_px(&print.description);
    let (x0, y0, pw, ph) = picture(print.width, print.height, frame);
    let mut picture_px = Vec::with_capacity(pw * ph);
    for y in 0..ph {
        let row = (y0 + y) * print.width + x0;
        picture_px.extend_from_slice(&print.samples[row..row + pw]);
    }
    let tuned = tune::tune(&picture_px, pw, ph);
    let mask = &tuned.mask;
    let sharpened = crate::mask::apply(&picture_px, pw, ph, mask.sigma, mask.amount);
    let mut out = print.samples.clone();
    for y in 0..ph {
        let src = y * pw;
        let dst = (y0 + y) * print.width + x0;
        out[dst..dst + pw].copy_from_slice(&sharpened[src..src + pw]);
    }
    let description = describe(print, mask, &tuned, frame);
    let lines = report_lines(mask, &tuned, frame);
    Outcome {
        samples: out,
        description,
        lines,
    }
}

pub fn write(path: &Path, print: &Print, outcome: &Outcome) -> Result<()> {
    crate::sheet::write(
        path,
        print.width,
        print.height,
        &outcome.samples,
        print,
        &outcome.description,
    )
}

/// `NikonZF.tif` is written beside itself as `NikonZF-peak.tif`.
pub fn beside(source: &Path) -> PathBuf {
    let stem = source
        .file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_else(|| "print".to_string());
    let name = if stem.ends_with("-peak") {
        format!("{stem}.tif")
    } else {
        format!("{stem}-peak.tif")
    };
    source.with_file_name(name)
}

pub fn grey_u8(samples: &[f32]) -> Vec<u8> {
    samples
        .iter()
        .map(|&v| (v.clamp(0.0, 1.0) * 255.0).round() as u8)
        .collect()
}

fn frame_px(description: &Option<String>) -> usize {
    let Some(text) = description else { return 0 };
    let Ok(v) = serde_json::from_str::<serde_json::Value>(text) else {
        return 0;
    };
    let n = |key: &str| v.get(key).and_then(|x| x.as_u64()).unwrap_or(0) as usize;
    n("border_px").saturating_add(n("black_px"))
}

fn picture(width: usize, height: usize, frame: usize) -> (usize, usize, usize, usize) {
    if frame == 0 || width <= 2 * frame || height <= 2 * frame {
        return (0, 0, width, height);
    }
    (frame, frame, width - 2 * frame, height - 2 * frame)
}

fn describe(print: &Print, mask: &Highpass, tuned: &tune::Tuned, frame: usize) -> String {
    serde_json::json!({
        "peak": env!("CARGO_PKG_VERSION"),
        "method": "highpass",
        "sigma_px": mask.sigma,
        "nyquist_gain": mask.gain,
        "amount": mask.amount,
        "target_mtf50": crate::highpass::TARGET_MTF50,
        "mtf50_before": tuned.mtf50_before,
        "mtf50_after": tuned.mtf50_after,
        "frame_px": frame,
        "source_description": print.description.as_deref().and_then(|s| serde_json::from_str::<serde_json::Value>(s).ok()),
    })
    .to_string()
}

fn report_lines(mask: &Highpass, tuned: &tune::Tuned, frame: usize) -> Vec<String> {
    let mut lines = Vec::new();
    match (tuned.mtf50_before, tuned.mtf50_after) {
        (Some(before), Some(after)) => {
            lines.push(format!("high-pass, radius {:.1} px", mask.sigma));
            lines.push(format!(
                "edges {:.2} cycles per pixel, lift {:.0} percent, MTF50 {:.2}",
                before,
                (mask.gain - 1.0) * 100.0,
                after
            ));
        }
        _ => lines.push("no clean edge, picture left as it was".to_string()),
    }
    if frame > 0 {
        lines.push(format!("paper border of {frame} px left untouched"));
    }
    lines
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_sharpened_file_sits_beside_the_source() {
        let path = beside(Path::new("/tmp/NikonZF.tif"));
        assert_eq!(path, PathBuf::from("/tmp/NikonZF-peak.tif"));
    }
}
