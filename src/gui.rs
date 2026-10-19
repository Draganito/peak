//! The window. Open a print, sharpen it, and look at both at the same pixels.

use crate::job;
use crate::sheet::Print;
use anyhow::{Context, Result};
use egui::{Color32, ColorImage, Pos2, Rect, Sense, TextureHandle, TextureOptions, Vec2};
use egui_file_dialog::FileDialog;
use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, Receiver};
use std::sync::Arc;
use std::thread;

const ZOOMS: [f32; 5] = [0.25, 0.5, 0.75, 1.0, 2.0];

#[derive(Clone)]
struct Plate {
    grey: Arc<Vec<u8>>,
    width: usize,
    height: usize,
    name: String,
}

struct Loaded {
    path: PathBuf,
    print: Option<Print>,
    original: Plate,
}

enum Note {
    Opened(Loaded),
    Sharpened {
        plate: Plate,
        lines: Vec<String>,
        path: PathBuf,
    },
    /// `print` comes back when sharpening failed, so the button can be pressed again.
    Failed {
        message: String,
        print: Option<Print>,
    },
}

pub fn start() -> Result<()> {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("peak")
            .with_app_id("peak")
            .with_inner_size([1400.0, 860.0])
            .with_min_inner_size([720.0, 480.0])
            .with_drag_and_drop(true),
        renderer: eframe::Renderer::Glow,
        ..Default::default()
    };
    eframe::run_native("peak", options, Box::new(|_cc| Ok(Box::new(App::new()))))
        .map_err(|err| anyhow::anyhow!("{err}"))
}

struct App {
    dialog: FileDialog,
    inbox: Option<Receiver<Note>>,
    loaded: Option<Loaded>,
    sharpened: Option<Plate>,
    lines: Vec<String>,
    wrote: Option<PathBuf>,
    status: String,
    busy: bool,
    zoom: usize,
    /// Image pixel under the top-left of each pane. Both panes share it.
    origin_x: f32,
    origin_y: f32,
    left_tex: Option<TextureHandle>,
    right_tex: Option<TextureHandle>,
    left_key: Option<ViewKey>,
    right_key: Option<ViewKey>,
    /// Trackpad wheel, accumulated until one zoom step.
    scroll_accum: f32,
}

#[derive(PartialEq)]
struct ViewKey {
    w: usize,
    h: usize,
    ox: i32,
    oy: i32,
    zoom: u8,
}

impl App {
    fn new() -> Self {
        let dialog = FileDialog::new().add_file_filter_extensions("TIFF", vec!["tif", "tiff"]);
        Self {
            dialog,
            inbox: None,
            loaded: None,
            sharpened: None,
            lines: Vec::new(),
            wrote: None,
            status: "Open a print.".to_string(),
            busy: false,
            zoom: 3,
            origin_x: 0.0,
            origin_y: 0.0,
            left_tex: None,
            right_tex: None,
            left_key: None,
            right_key: None,
            scroll_accum: 0.0,
        }
    }

    fn open(&mut self, path: PathBuf, ctx: &egui::Context) {
        if self.busy {
            return;
        }
        self.busy = true;
        self.status = format!("reading {}", path.display());
        self.sharpened = None;
        self.wrote = None;
        self.lines.clear();
        self.right_tex = None;
        self.right_key = None;
        let (tx, rx) = mpsc::channel();
        self.inbox = Some(rx);
        let ctx = ctx.clone();
        thread::spawn(move || {
            let note = match load(&path) {
                Ok(loaded) => Note::Opened(loaded),
                Err(err) => Note::Failed {
                    message: format!("{err:#}"),
                    print: None,
                },
            };
            let _ = tx.send(note);
            ctx.request_repaint();
        });
    }

    fn sharpen(&mut self, ctx: &egui::Context) {
        let Some(loaded) = self.loaded.as_mut() else {
            return;
        };
        let Some(print) = loaded.print.take() else {
            return;
        };
        self.busy = true;
        self.status = "sharpening…".to_string();
        let out = job::beside(&loaded.path);
        let (tx, rx) = mpsc::channel();
        self.inbox = Some(rx);
        let ctx = ctx.clone();
        thread::spawn(move || {
            let note = match sharpen(print, &out) {
                Ok((plate, lines)) => Note::Sharpened {
                    plate,
                    lines,
                    path: out,
                },
                Err((err, print)) => Note::Failed {
                    message: format!("{err:#}"),
                    print: Some(print),
                },
            };
            let _ = tx.send(note);
            ctx.request_repaint();
        });
    }

    fn poll(&mut self) {
        let Some(rx) = self.inbox.as_ref() else {
            return;
        };
        let Ok(note) = rx.try_recv() else {
            return;
        };
        self.inbox = None;
        self.busy = false;
        match note {
            Note::Opened(loaded) => {
                self.origin_x = 0.0;
                self.origin_y = 0.0;
                self.zoom = 3;
                self.status = loaded.path.display().to_string();
                self.left_tex = None;
                self.left_key = None;
                self.loaded = Some(loaded);
            }
            Note::Sharpened { plate, lines, path } => {
                self.status = format!("wrote {}", path.display());
                self.lines = lines;
                self.wrote = Some(path);
                self.right_tex = None;
                self.right_key = None;
                self.sharpened = Some(plate);
            }
            Note::Failed { message, print } => {
                if let (Some(print), Some(loaded)) = (print, self.loaded.as_mut()) {
                    loaded.print = Some(print);
                }
                self.status = message;
            }
        }
    }
}

fn load(path: &Path) -> Result<Loaded> {
    let print = crate::sheet::read(path).with_context(|| format!("reading {}", path.display()))?;
    let grey = job::grey_u8(&print.samples);
    let name = path
        .file_name()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_else(|| "print".to_string());
    Ok(Loaded {
        path: path.to_path_buf(),
        original: Plate {
            grey: Arc::new(grey),
            width: print.width,
            height: print.height,
            name,
        },
        print: Some(print),
    })
}

fn sharpen(
    print: Print,
    out: &Path,
) -> std::result::Result<(Plate, Vec<String>), (anyhow::Error, Print)> {
    let outcome = job::compute(&print);
    if let Err(err) = job::write(out, &print, &outcome) {
        return Err((err, print));
    }
    let plate = Plate {
        grey: Arc::new(job::grey_u8(&outcome.samples)),
        width: print.width,
        height: print.height,
        name: out
            .file_name()
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_else(|| "peak".to_string()),
    };
    Ok((plate, outcome.lines))
}

const SCROLL_STEP: f32 = 40.0;

impl eframe::App for App {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        self.poll();
        if self.busy {
            ctx.request_repaint();
        }
        self.dialog.update(&ctx);
        if let Some(path) = self.dialog.take_picked() {
            self.open(path, &ctx);
        }
        for file in ctx.input(|i| i.raw.dropped_files.clone()) {
            self.open(file.path().to_path_buf(), &ctx);
        }
        self.keys(&ctx);

        egui::Panel::top("bar").show(ui, |ui| {
            ui.horizontal(|ui| {
                if ui
                    .add_enabled(!self.busy, egui::Button::new("Open"))
                    .clicked()
                {
                    self.dialog.pick_file();
                }
                let ready = self.loaded.as_ref().is_some_and(|l| l.print.is_some()) && !self.busy;
                if ui
                    .add_enabled(ready, egui::Button::new("Sharpen"))
                    .clicked()
                {
                    self.sharpen(&ctx);
                }
                for (i, zoom) in ZOOMS.iter().enumerate() {
                    let label = format!("{}", (*zoom * 100.0).round() as i32);
                    if ui
                        .selectable_label(self.zoom == i, format!("{label} %"))
                        .clicked()
                    {
                        self.zoom = i;
                    }
                }
                ui.label(&self.status);
            });
        });
        egui::Panel::bottom("report").show(ui, |ui| {
            ui.horizontal(|ui| {
                if self.lines.is_empty() {
                    ui.label(" ");
                } else {
                    ui.label(self.lines.join("   "));
                }
            });
        });
        egui::CentralPanel::default().show(ui, |ui| {
            let rect = ui.max_rect();
            let gap = 8.0;
            let half = (rect.width() - gap) * 0.5;
            let left = Rect::from_min_size(rect.min, Vec2::new(half, rect.height()));
            let right = Rect::from_min_size(
                Pos2::new(rect.min.x + half + gap, rect.min.y),
                Vec2::new(rect.width() - half - gap, rect.height()),
            );
            let original = self.loaded.as_ref().map(|loaded| loaded.original.clone());
            let sharpened = self.sharpened.clone();
            if original.is_some() {
                let steps = self.wheel_steps(&ctx);
                if steps != 0 {
                    let pointer = ctx.input(|i| i.pointer.hover_pos());
                    let pane = pointer.and_then(|p| {
                        if left.contains(p) {
                            Some(left)
                        } else if right.contains(p) {
                            Some(right)
                        } else {
                            None
                        }
                    });
                    if let Some(pane) = pane {
                        self.zoom_around(&ctx, pane, steps);
                    }
                }
            }
            if let Some(original) = original {
                self.paint(ui, &ctx, left, &original, true);
                if let Some(sharp) = sharpened {
                    self.paint(ui, &ctx, right, &sharp, false);
                } else {
                    empty(ui, right, "Sharpen writes the file, then it sits here.");
                }
            } else {
                empty(ui, rect, "Open a print, or drop one here.");
            }
        });
    }
}

impl App {
    fn keys(&mut self, ctx: &egui::Context) {
        let mut step = 0i32;
        ctx.input_mut(|i| {
            if i.consume_key(egui::Modifiers::NONE, egui::Key::Plus)
                || i.consume_key(egui::Modifiers::NONE, egui::Key::Equals)
            {
                step = 1;
            }
            if i.consume_key(egui::Modifiers::NONE, egui::Key::Minus) {
                step = -1;
            }
            let nudge = if i.modifiers.shift { 256.0 } else { 32.0 };
            let zoom = ZOOMS[self.zoom];
            if i.consume_key(egui::Modifiers::NONE, egui::Key::ArrowLeft)
                || i.consume_key(egui::Modifiers::SHIFT, egui::Key::ArrowLeft)
            {
                self.origin_x -= nudge / zoom;
            }
            if i.consume_key(egui::Modifiers::NONE, egui::Key::ArrowRight)
                || i.consume_key(egui::Modifiers::SHIFT, egui::Key::ArrowRight)
            {
                self.origin_x += nudge / zoom;
            }
            if i.consume_key(egui::Modifiers::NONE, egui::Key::ArrowUp)
                || i.consume_key(egui::Modifiers::SHIFT, egui::Key::ArrowUp)
            {
                self.origin_y -= nudge / zoom;
            }
            if i.consume_key(egui::Modifiers::NONE, egui::Key::ArrowDown)
                || i.consume_key(egui::Modifiers::SHIFT, egui::Key::ArrowDown)
            {
                self.origin_y += nudge / zoom;
            }
        });
        if step != 0 {
            self.zoom = (self.zoom as i32 + step).clamp(0, ZOOMS.len() as i32 - 1) as usize;
        }
    }

    /// One mouse notch is one step. A trackpad accumulates until [`SCROLL_STEP`].
    fn wheel_steps(&mut self, ctx: &egui::Context) -> i32 {
        let mut steps = 0i32;
        ctx.input(|i| {
            for ev in &i.raw.events {
                if let egui::Event::MouseWheel { unit, delta, .. } = ev {
                    match unit {
                        egui::MouseWheelUnit::Line | egui::MouseWheelUnit::Page => {
                            steps += delta.y.signum() as i32;
                            self.scroll_accum = 0.0;
                        }
                        egui::MouseWheelUnit::Point => {
                            if delta.y.signum() != self.scroll_accum.signum() {
                                self.scroll_accum = 0.0;
                            }
                            self.scroll_accum += delta.y;
                            if self.scroll_accum.abs() >= SCROLL_STEP {
                                steps += self.scroll_accum.signum() as i32;
                                self.scroll_accum = 0.0;
                            }
                        }
                    }
                }
            }
        });
        steps
    }

    fn zoom_around(&mut self, ctx: &egui::Context, rect: Rect, steps: i32) {
        let ppp = ctx.pixels_per_point();
        let pointer = ctx
            .input(|i| i.pointer.hover_pos())
            .unwrap_or(rect.center());
        let rel_x = (pointer.x - rect.min.x) * ppp;
        let rel_y = (pointer.y - rect.min.y) * ppp;
        let zoom = ZOOMS[self.zoom];
        let ix = self.origin_x + rel_x / zoom;
        let iy = self.origin_y + rel_y / zoom;
        let next = self.zoom as i32 + steps;
        self.zoom = next.clamp(0, ZOOMS.len() as i32 - 1) as usize;
        let zoom = ZOOMS[self.zoom];
        self.origin_x = ix - rel_x / zoom;
        self.origin_y = iy - rel_y / zoom;
    }

    fn paint(
        &mut self,
        ui: &mut egui::Ui,
        ctx: &egui::Context,
        rect: Rect,
        plate: &Plate,
        left: bool,
    ) {
        let response = ui.interact(rect, ui.id().with(left), Sense::click_and_drag());
        let ppp = ctx.pixels_per_point();
        if response.dragged() {
            let d = response.drag_delta();
            let zoom = ZOOMS[self.zoom];
            self.origin_x -= d.x * ppp / zoom;
            self.origin_y -= d.y * ppp / zoom;
        }
        let zoom = ZOOMS[self.zoom];
        self.origin_x = snap(self.origin_x, zoom);
        self.origin_y = snap(self.origin_y, zoom);

        let pw = (rect.width() * ppp).round().max(1.0) as usize;
        let ph = (rect.height() * ppp).round().max(1.0) as usize;
        let key = ViewKey {
            w: pw,
            h: ph,
            ox: (self.origin_x * 100.0).round() as i32,
            oy: (self.origin_y * 100.0).round() as i32,
            zoom: (zoom * 100.0).round() as u8,
        };
        let slot = if left {
            &mut self.left_tex
        } else {
            &mut self.right_tex
        };
        let cached = if left {
            &mut self.left_key
        } else {
            &mut self.right_key
        };
        if cached.as_ref() != Some(&key) {
            let image = raster(plate, self.origin_x, self.origin_y, zoom, pw, ph);
            let options = TextureOptions::NEAREST;
            if let Some(tex) = slot.as_mut() {
                tex.set(image, options);
            } else {
                *slot = Some(ctx.load_texture(if left { "left" } else { "right" }, image, options));
            }
            *cached = Some(key);
        }
        if let Some(tex) = slot.as_ref() {
            ui.painter().image(
                tex.id(),
                rect,
                Rect::from_min_max(Pos2::ZERO, Pos2::new(1.0, 1.0)),
                Color32::WHITE,
            );
        }
        ui.painter().text(
            rect.min + Vec2::new(8.0, 6.0),
            egui::Align2::LEFT_TOP,
            &plate.name,
            egui::FontId::proportional(13.0),
            Color32::from_rgb(220, 220, 220),
        );
    }
}

fn snap(origin: f32, zoom: f32) -> f32 {
    if zoom >= 1.0 {
        (origin * zoom).round() / zoom
    } else {
        origin
    }
}

fn raster(
    plate: &Plate,
    origin_x: f32,
    origin_y: f32,
    zoom: f32,
    out_w: usize,
    out_h: usize,
) -> ColorImage {
    let mut rgba = vec![0u8; out_w * out_h * 4];
    for y in 0..out_h {
        let iy = (origin_y + y as f32 / zoom).floor() as isize;
        for x in 0..out_w {
            let ix = (origin_x + x as f32 / zoom).floor() as isize;
            let g = sample(&plate.grey, plate.width, plate.height, ix, iy);
            let i = (y * out_w + x) * 4;
            rgba[i] = g;
            rgba[i + 1] = g;
            rgba[i + 2] = g;
            rgba[i + 3] = 255;
        }
    }
    ColorImage::from_rgba_unmultiplied([out_w, out_h], &rgba)
}

fn sample(grey: &[u8], width: usize, height: usize, x: isize, y: isize) -> u8 {
    if x < 0 || y < 0 || x as usize >= width || y as usize >= height {
        return 24;
    }
    grey[y as usize * width + x as usize]
}

fn empty(ui: &mut egui::Ui, rect: Rect, text: &str) {
    ui.painter()
        .rect_filled(rect, 0.0, Color32::from_rgb(28, 28, 28));
    ui.painter().text(
        rect.center(),
        egui::Align2::CENTER_CENTER,
        text,
        egui::FontId::proportional(16.0),
        Color32::from_rgb(180, 180, 180),
    );
}
