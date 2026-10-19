//! One high-pass for a print from Mimizan Lab.
//!
//! With a file and `-o`, it writes the sharpened print and exits.
//! With no arguments, it opens the window.

mod gui;
mod highpass;
mod job;
mod mask;
mod sheet;
mod tune;

use anyhow::{bail, Context, Result};
use std::env;
use std::path::PathBuf;

fn main() {
    if env::args().len() <= 1 {
        if let Err(err) = gui::start() {
            eprintln!("peak: {err:#}");
            std::process::exit(1);
        }
        return;
    }
    if let Err(err) = run() {
        eprintln!("peak: {err:#}");
        std::process::exit(1);
    }
}

fn run() -> Result<()> {
    let args = Args::parse()?;
    let print =
        sheet::read(&args.input).with_context(|| format!("reading {}", args.input.display()))?;
    let outcome = job::compute(&print);
    job::write(&args.output, &print, &outcome)?;
    for line in &outcome.lines {
        println!("{line}");
    }
    println!("wrote {}", args.output.display());
    Ok(())
}

struct Args {
    input: PathBuf,
    output: PathBuf,
}

impl Args {
    fn parse() -> Result<Self> {
        let mut input = None;
        let mut output = None;
        let mut it = env::args().skip(1);
        while let Some(arg) = it.next() {
            match arg.as_str() {
                "-o" | "--output" => {
                    output = Some(PathBuf::from(it.next().context("-o needs a path")?));
                }
                "-h" | "--help" => {
                    println!(
                        "peak PRINT.tif -o PRINT-peak.tif\n\
                         peak\n\
                         peak --version\n\n\
                         High-pass on a Mimizan print. The sharpest edges in the picture\n\
                         set the radius and the strength, so their MTF50 lands as close\n\
                         as it can to 0.35 cycles per pixel without a bright rim.\n\
                         One radius and one strength for the whole sheet.\n\
                         The paper border is left as it was.\n\
                         With no arguments, a window opens."
                    );
                    std::process::exit(0);
                }
                "-V" | "--version" => {
                    println!("peak {}", env!("CARGO_PKG_VERSION"));
                    std::process::exit(0);
                }
                s if s.starts_with('-') => bail!("unknown argument {s}"),
                s => {
                    if input.is_some() {
                        bail!("unexpected argument {s}");
                    }
                    input = Some(PathBuf::from(s));
                }
            }
        }
        Ok(Args {
            input: input.context("need a print TIFF")?,
            output: output.context("need -o")?,
        })
    }
}
