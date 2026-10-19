# peak

A high-pass for a print from Mimizan Lab.

It adds `image − blur` back onto the print. The sharpest edges in the picture are measured, and the radius and the strength are set so those edges come as close as they can to 0.35 cycles per pixel without a bright rim. A soft picture gets a wider radius. A picture with no such edge is left as it was.

One radius and one strength apply to the whole sheet. A hard edge receives the same lift as a soft one.

The paper border is not touched. The profile, the orientation and the resolution of the TIFF are kept.

## Run

```bash
peak print.tif -o print-peak.tif
peak
peak --version
```

`peak` with no arguments opens a window. Open a print, or drop one on the window, and press Sharpen. The sharpened file is written beside the original, as `print-peak.tif`. Both sit next to each other at the same pixels. The zoom steps are 25, 50, 75, 100 and 200 percent. Drag and the scroll wheel move both. At 100 percent one image pixel is one screen pixel.

The command prints the radius, the measured edges, the lift and the MTF50 after the lift. When it finds no clean edge it says so and leaves the picture as it was.

## Install

The `.deb` is built from this repository. Pushing a tag `vX.Y.Z` that matches the version in `Cargo.toml` runs [`.github/workflows/release.yml`](.github/workflows/release.yml) and attaches `peak_<version>-1_amd64.deb` to the GitHub release.

```bash
sudo apt install ./peak_0.5.0-1_amd64.deb
```

Then `peak`, or the Peak entry in the application menu. The package needs OpenGL and a Wayland or X11 session.

## Build

```bash
cargo test
cargo build --release
dist/make_release.sh
```

`dist/make_release.sh` writes `dist/peak_<version>-1_amd64.deb` and a tarball of the same binary. The `.deb` needs `cargo-deb` (`cargo install cargo-deb --locked`). A Rust toolchain is enough for `cargo build`.

## License

GPL-3.0-or-later. See [LICENSE](LICENSE).
