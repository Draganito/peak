//! Read and write the 16-bit grey print Mimizan Lab exports.

use anyhow::{bail, Context, Result};
use std::fs::File;
use std::io::{BufReader, BufWriter};
use std::path::Path;
use tiff::decoder::{Decoder, DecodingResult, Limits};
use tiff::encoder::{colortype, Rational, TiffEncoder};
use tiff::tags::{ResolutionUnit, Tag};

pub struct Print {
    pub width: usize,
    pub height: usize,
    /// Code values, 0..=1, as stored in the file.
    pub samples: Vec<f32>,
    pub dpi: Option<f64>,
    pub description: Option<String>,
    pub icc: Option<Vec<u8>>,
    pub orientation: u16,
}

pub fn read(path: &Path) -> Result<Print> {
    let mut dec = Decoder::new(BufReader::new(File::open(path)?))?.with_limits(read_limits());
    let (w, h) = dec.dimensions()?;
    let description = dec.get_tag_ascii_string(Tag::ImageDescription).ok();
    let orientation = dec
        .get_tag_u32(Tag::Orientation)
        .ok()
        .filter(|o| (1..=8).contains(o))
        .unwrap_or(1) as u16;
    let dpi = resolution_dpi(&mut dec);
    let icc = dec.get_tag(Tag::IccProfile).ok().and_then(icc_bytes);
    let samples: Vec<f32> = match dec.read_image()? {
        DecodingResult::U16(v) => v.iter().map(|&x| f32::from(x) / 65535.0).collect(),
        DecodingResult::U8(v) => v.iter().map(|&x| f32::from(x) / 255.0).collect(),
        _ => bail!("the print needs to be 8- or 16-bit grey"),
    };
    if samples.len() != (w as usize) * (h as usize) {
        bail!("the TIFF is not a single grey channel");
    }
    Ok(Print {
        width: w as usize,
        height: h as usize,
        samples,
        dpi,
        description,
        icc,
        orientation,
    })
}

/// The crate stops a decoded image at 256 MiB. A large sheet is already past
/// that: this limit is two gigabytes of samples, enough for a big print,
/// and it still refuses a file that would not fit beside the working copies.
fn read_limits() -> Limits {
    let mut limits = Limits::default();
    limits.decoding_buffer_size = 2 * 1024 * 1024 * 1024;
    limits.intermediate_buffer_size = 2 * 1024 * 1024 * 1024;
    limits
}

fn icc_bytes(value: tiff::decoder::ifd::Value) -> Option<Vec<u8>> {
    match value {
        tiff::decoder::ifd::Value::List(items) => {
            let bytes: Vec<u8> = items
                .into_iter()
                .map(|item| match item {
                    tiff::decoder::ifd::Value::Byte(b) => Some(b),
                    _ => None,
                })
                .collect::<Option<_>>()?;
            if bytes.is_empty() {
                None
            } else {
                Some(bytes)
            }
        }
        tiff::decoder::ifd::Value::Byte(b) => Some(vec![b]),
        _ => None,
    }
}

fn resolution_dpi(dec: &mut Decoder<BufReader<File>>) -> Option<f64> {
    let dpi = match dec.get_tag(Tag::XResolution).ok()? {
        tiff::decoder::ifd::Value::Rational(n, d) if d != 0 => f64::from(n) / f64::from(d),
        _ => return None,
    };
    if dpi > 0.0 {
        Some(dpi)
    } else {
        None
    }
}

pub fn write(
    path: &Path,
    width: usize,
    height: usize,
    samples: &[f32],
    meta: &Print,
    description: &str,
) -> Result<()> {
    let data: Vec<u16> = samples
        .iter()
        .map(|&v| (v.clamp(0.0, 1.0) * 65535.0).round() as u16)
        .collect();
    let mut enc = TiffEncoder::new(BufWriter::new(File::create(path)?))?;
    let mut img = enc.new_image::<colortype::Gray16>(width as u32, height as u32)?;
    img.rows_per_strip(64)?;
    {
        let d = img.encoder();
        if let Some(icc) = &meta.icc {
            d.write_tag(Tag::IccProfile, icc.as_slice())?;
        }
        d.write_tag(Tag::ImageDescription, description)?;
        d.write_tag(Tag::Software, concat!("peak ", env!("CARGO_PKG_VERSION")))?;
        if (1..=8).contains(&meta.orientation) {
            d.write_tag(Tag::Orientation, meta.orientation)?;
        }
    }
    if let Some(dpi) = meta.dpi {
        img.resolution(
            ResolutionUnit::Inch,
            Rational {
                n: (dpi * 1000.0).round() as u32,
                d: 1000,
            },
        );
    }
    img.write_data(&data).context("writing the print")?;
    Ok(())
}
