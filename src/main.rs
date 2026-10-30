#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod convert;
mod heic;

use convert::{Output, Photo};
use eframe::egui::{self, Color32, RichText, Vec2};
use std::{
    path::PathBuf,
    sync::{Arc, mpsc},
    thread,
};

const INK: Color32 = Color32::from_gray(20);
const MUTED: Color32 = Color32::from_gray(115);
const ACCENT: Color32 = Color32::BLACK;
const LINE: Color32 = Color32::from_gray(228);

fn main() -> eframe::Result {
    eframe::run_native(
        "Photo Convr",
        eframe::NativeOptions {
            viewport: egui::ViewportBuilder::default()
                .with_inner_size([680.0, 560.0])
                .with_min_inner_size([420.0, 480.0]),
            ..Default::default()
        },
        Box::new(|cc| Ok(Box::new(App::new(cc)))),
    )
}

enum Event {
    Loaded(Result<Photo, String>),
    Saved(Result<PathBuf, String>),
}
struct App {
    photo: Option<Arc<Photo>>,
    preview: Option<egui::TextureHandle>,
    output: Output,
    quality: u8,
    busy: bool,
    status: Option<(bool, String)>,
    tx: mpsc::Sender<Event>,
    rx: mpsc::Receiver<Event>,
    #[cfg(test)]
    geometry: Geometry,
}
impl App {
    fn new(cc: &eframe::CreationContext<'_>) -> Self {
        Self::with_context(&cc.egui_ctx)
    }

    fn with_context(ctx: &egui::Context) -> Self {
        let mut style = (*ctx.style()).clone();
        style.visuals = egui::Visuals::light();
        style.visuals.override_text_color = Some(INK);
        style.visuals.panel_fill = Color32::WHITE;
        style.visuals.selection.bg_fill = ACCENT;
        style.visuals.selection.stroke = egui::Stroke::new(1.0_f32, Color32::WHITE);
        style.visuals.hyperlink_color = INK;
        style.visuals.widgets.hovered.bg_fill = Color32::from_gray(238);
        style.visuals.widgets.hovered.weak_bg_fill = Color32::from_gray(238);
        style.visuals.widgets.hovered.bg_stroke =
            egui::Stroke::new(1.0_f32, Color32::from_gray(170));
        style.visuals.widgets.active.bg_fill = Color32::from_gray(220);
        style.visuals.widgets.active.weak_bg_fill = Color32::from_gray(220);
        style.visuals.widgets.active.bg_stroke = egui::Stroke::new(1.0_f32, INK);
        style.visuals.widgets.active.fg_stroke = egui::Stroke::new(1.0_f32, INK);
        style.visuals.widgets.inactive.bg_fill = Color32::from_gray(248);
        style.visuals.widgets.inactive.bg_stroke = egui::Stroke::new(1.0_f32, LINE);
        style.spacing.item_spacing = Vec2::new(10.0, 8.0);
        style.spacing.button_padding = Vec2::new(12.0, 6.0);
        style
            .text_styles
            .insert(egui::TextStyle::Body, egui::FontId::proportional(15.0));
        style
            .text_styles
            .insert(egui::TextStyle::Button, egui::FontId::proportional(15.0));
        ctx.set_style(style);
        let (tx, rx) = mpsc::channel();
        Self {
            photo: None,
            preview: None,
            output: Output::Png,
            quality: 90,
            busy: false,
            status: None,
            tx,
            rx,
            #[cfg(test)]
            geometry: Geometry::default(),
        }
    }
    fn load(&mut self, path: PathBuf, ctx: &egui::Context) {
        if self.busy {
            return;
        }
        self.busy = true;
        self.status = None;
        let tx = self.tx.clone();
        let ctx = ctx.clone();
        thread::spawn(move || {
            let _ = tx.send(Event::Loaded(convert::load(&path)));
            ctx.request_repaint();
        });
    }
    fn browse(&mut self, ctx: &egui::Context) {
        if let Some(path) = rfd::FileDialog::new()
            .set_title("Choose a photo")
            .add_filter(
                "Images",
                &[
                    "heic", "heif", "hif", "png", "jpg", "jpeg", "webp", "gif", "tif", "tiff",
                    "bmp", "ico", "tga", "pnm", "ppm", "pgm", "pbm", "qoi", "hdr", "exr", "dds",
                    "ff",
                ],
            )
            .pick_file()
        {
            self.load(path, ctx);
        }
    }
    fn save(&mut self, ctx: &egui::Context) {
        let Some(photo) = self.photo.clone() else {
            return;
        };
        let stem = photo.path.file_stem().unwrap_or_default().to_string_lossy();
        let Some(path) = rfd::FileDialog::new()
            .set_title("Save converted photo")
            .set_file_name(format!("{stem}-converted.{}", self.output.extension()))
            .add_filter(self.output.label(), &[self.output.extension()])
            .save_file()
        else {
            return;
        };
        let path = path.with_extension(self.output.extension());
        // The dialog must confirm the exact destination, including its final extension.
        if path.exists()
            && !rfd::MessageDialog::new()
                .set_title("Replace existing file?")
                .set_description(format!("{} already exists. Replace it?", path.display()))
                .set_buttons(rfd::MessageButtons::YesNo)
                .show()
                .eq(&rfd::MessageDialogResult::Yes)
        {
            return;
        }
        self.busy = true;
        self.status = None;
        let (tx, ctx, output, quality) = (self.tx.clone(), ctx.clone(), self.output, self.quality);
        thread::spawn(move || {
            let _ = tx.send(Event::Saved(convert::save(&photo, &path, output, quality)));
            ctx.request_repaint();
        });
    }
    fn poll(&mut self, ctx: &egui::Context) {
        while let Ok(event) = self.rx.try_recv() {
            self.busy = false;
            match event {
                Event::Loaded(Ok(photo)) => {
                    let thumb = photo.image.thumbnail(320, 320).to_rgba8();
                    self.preview = Some(ctx.load_texture(
                        "photo",
                        egui::ColorImage::from_rgba_unmultiplied(
                            [thumb.width() as usize, thumb.height() as usize],
                            thumb.as_raw(),
                        ),
                        egui::TextureOptions::LINEAR,
                    ));
                    self.photo = Some(Arc::new(photo));
                }
                Event::Loaded(Err(error)) | Event::Saved(Err(error)) => {
                    self.status = Some((false, error))
                }
                Event::Saved(Ok(path)) => {
                    self.status = Some((true, format!("Saved to {}", path.display())))
                }
            }
        }
    }
}

fn muted(ui: &mut egui::Ui, text: impl Into<String>) {
    ui.label(RichText::new(text.into()).color(MUTED).size(13.0));
}

impl App {
    fn format_controls(&mut self, ui: &mut egui::Ui) {
        ui.label(RichText::new("Convert to").strong());
        ui.add_enabled_ui(!self.busy, |ui| {
            let dropdown = egui::ComboBox::from_id_salt("output")
                .selected_text(self.output.label())
                .width((ui.available_width() - 8.0).max(100.0))
                .show_ui(ui, |ui| {
                    for output in Output::ALL {
                        ui.selectable_value(&mut self.output, output, output.label());
                    }
                });
            #[cfg(test)]
            {
                self.geometry.dropdown = Some(dropdown.response.rect);
            }
            #[cfg(not(test))]
            let _ = dropdown;
            if self.output == Output::Jpeg {
                ui.add(egui::Slider::new(&mut self.quality, 1..=100).text("Quality"));
            } else {
                muted(ui, self.output.description());
            }
        });
    }

    fn save_control(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        let button = ui.add_enabled(
            self.photo.is_some() && !self.busy,
            egui::Button::new(
                RichText::new("Convert & save  →")
                    .color(Color32::WHITE)
                    .strong(),
            )
            .fill(ACCENT)
            .min_size(Vec2::new(ui.available_width(), 44.0)),
        );
        #[cfg(test)]
        {
            self.geometry.save = Some(button.rect);
        }
        if button.clicked() {
            self.save(ctx);
        }
        if self.busy {
            ui.horizontal(|ui| {
                ui.spinner();
                muted(ui, "Working on your photo…");
            });
        } else {
            muted(ui, "Choose where to save. Your original is kept.");
        }
    }

    fn draw(&mut self, ctx: &egui::Context) {
        let margin = if ctx.content_rect().width() < 720.0 {
            16
        } else {
            24
        };
        // Reserve the controls before allocating the preview. Image size can never
        // push the format dropdown or save button out of the window.
        egui::TopBottomPanel::top("header")
            .frame(egui::Frame::new().fill(Color32::WHITE).inner_margin(margin))
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    ui.label(RichText::new("photo convr").size(22.0).strong());
                });
            });
        egui::TopBottomPanel::bottom("conversion")
            .frame(
                egui::Frame::new()
                    .fill(Color32::WHITE)
                    .stroke(egui::Stroke::new(1.0_f32, LINE))
                    .inner_margin(margin),
            )
            .show(ctx, |ui| {
                ui.set_max_width(ui.available_width());
                if ui.available_width() >= 600.0 {
                    ui.columns(2, |columns| {
                        self.format_controls(&mut columns[0]);
                        columns[1].add_space(20.0);
                        self.save_control(&mut columns[1], ctx);
                    });
                } else {
                    self.format_controls(ui);
                    ui.add_space(4.0);
                    self.save_control(ui, ctx);
                }
                if let Some((_, message)) = &self.status {
                    ui.add_space(4.0);
                    egui::ScrollArea::vertical()
                        .id_salt("status")
                        .max_height(48.0)
                        .show(ui, |ui| {
                            ui.label(RichText::new(message).color(INK).size(13.0));
                        });
                }
            });
        egui::CentralPanel::default()
            .frame(egui::Frame::new().fill(Color32::WHITE).inner_margin(margin))
            .show(ctx, |ui| {
                ui.label(RichText::new("Convert").size(26.0).strong());
                let hovering = ctx.input(|i| !i.raw.hovered_files.is_empty());
                let height = ui
                    .available_height()
                    .clamp(0.0, if self.photo.is_some() { 104.0 } else { 140.0 });
                let size = Vec2::new(ui.available_width(), height);
                let (rect, response) = ui.allocate_exact_size(size, egui::Sense::click());
                #[cfg(test)]
                {
                    self.geometry.preview = Some(rect);
                }
                let fill = if hovering {
                    Color32::from_gray(234)
                } else {
                    Color32::from_gray(249)
                };
                ui.painter().rect(
                    rect,
                    12.0,
                    fill,
                    egui::Stroke::new(1.0_f32, if hovering { INK } else { LINE }),
                    egui::StrokeKind::Inside,
                );
                if let (Some(texture), Some(photo)) = (&self.preview, &self.photo) {
                    let thumbnail = egui::Rect::from_min_size(
                        rect.min + Vec2::splat(12.0),
                        Vec2::splat((height - 24.0).clamp(0.0, 80.0)),
                    );
                    if thumbnail.is_positive() {
                        let image_size = texture.size_vec2();
                        let scale = (thumbnail.width() / image_size.x)
                            .min(thumbnail.height() / image_size.y);
                        ui.painter().image(
                            texture.id(),
                            egui::Rect::from_center_size(thumbnail.center(), image_size * scale),
                            egui::Rect::from_min_max(egui::Pos2::ZERO, egui::pos2(1.0, 1.0)),
                            Color32::WHITE,
                        );
                    }
                    let text_rect = egui::Rect::from_min_max(
                        egui::pos2(thumbnail.right() + 16.0, rect.top() + 18.0),
                        rect.max - Vec2::splat(12.0),
                    );
                    ui.scope_builder(egui::UiBuilder::new().max_rect(text_rect), |ui| {
                        ui.set_clip_rect(text_rect.intersect(rect));
                        ui.add(
                            egui::Label::new(
                                RichText::new(
                                    photo.path.file_name().unwrap_or_default().to_string_lossy(),
                                )
                                .strong(),
                            )
                            .truncate(),
                        );
                        muted(
                            ui,
                            format!(
                                "{}  ·  {} × {} px",
                                photo.format,
                                photo.image.width(),
                                photo.image.height()
                            ),
                        );
                        muted(ui, convert::size_label(photo.bytes));
                    });
                } else if height >= 80.0 {
                    ui.painter().text(
                        rect.center() - Vec2::new(0.0, 13.0),
                        egui::Align2::CENTER_CENTER,
                        if hovering {
                            "Drop to open"
                        } else {
                            "Drop a photo, or click to browse"
                        },
                        egui::FontId::proportional(16.0),
                        INK,
                    );
                    ui.painter().text(
                        rect.center() + Vec2::new(0.0, 16.0),
                        egui::Align2::CENTER_CENTER,
                        "HEIC · JPG · PNG · WebP + more",
                        egui::FontId::proportional(12.0),
                        MUTED,
                    );
                }
                if response.clicked() && !self.busy {
                    self.browse(ctx);
                }
                if self.photo.is_some() {
                    ui.add_space(4.0);
                    if ui
                        .add_enabled(
                            !self.busy,
                            egui::Button::new("Choose another photo").small(),
                        )
                        .clicked()
                    {
                        self.browse(ctx);
                    }
                }
            });
    }
}

impl eframe::App for App {
    fn update(&mut self, ctx: &egui::Context, _: &mut eframe::Frame) {
        self.poll(ctx);
        if let Some(path) = ctx.input(|i| i.raw.dropped_files.first().and_then(|f| f.path.clone()))
        {
            self.load(path, ctx);
        }
        self.draw(ctx);
    }
}

#[cfg(test)]
#[derive(Default)]
struct Geometry {
    dropdown: Option<egui::Rect>,
    save: Option<egui::Rect>,
    preview: Option<egui::Rect>,
}

#[cfg(test)]
mod layout_tests {
    use super::*;

    #[test]
    fn conversion_controls_remain_visible_with_large_previews() {
        for window in [
            [420.0, 480.0],
            [640.0, 480.0],
            [680.0, 560.0],
            [800.0, 650.0],
            [1060.0, 740.0],
            [1440.0, 900.0],
        ] {
            for image_size in [None, Some([1600, 900]), Some([900, 1600])] {
                let ctx = egui::Context::default();
                let mut app = App::with_context(&ctx);
                if let Some(size) = image_size {
                    app.preview = Some(ctx.load_texture(
                        "test",
                        egui::ColorImage::filled(size, Color32::RED),
                        egui::TextureOptions::LINEAR,
                    ));
                    app.photo = Some(Arc::new(Photo {
                        path: PathBuf::from(format!("{}.heic", "long-filename".repeat(30))),
                        image: image::DynamicImage::new_rgb8(16, 9),
                        format: "HEIC".into(),
                        bytes: 1_000_000,
                    }));
                }
                app.output = Output::Jpeg;
                let screen = egui::Rect::from_min_size(egui::Pos2::ZERO, Vec2::from(window));
                for _ in 0..3 {
                    let _ = ctx.run(
                        egui::RawInput {
                            screen_rect: Some(screen),
                            ..Default::default()
                        },
                        |ctx| app.draw(ctx),
                    );
                }
                let dropdown = app.geometry.dropdown.unwrap();
                let save = app.geometry.save.unwrap();
                let preview = app.geometry.preview.unwrap();
                assert!(
                    screen.contains_rect(dropdown),
                    "dropdown clipped at {window:?}: {dropdown:?}"
                );
                assert!(
                    screen.contains_rect(save),
                    "save clipped at {window:?}: {save:?}"
                );
                assert!(
                    screen.contains_rect(preview),
                    "preview overflow at {window:?}: {preview:?}"
                );
                assert!(
                    !preview.intersects(dropdown),
                    "preview covers dropdown at {window:?}"
                );
                assert!(
                    !preview.intersects(save),
                    "preview covers save at {window:?}"
                );
                assert!(
                    preview.height() <= if image_size.is_some() { 104.0 } else { 140.0 },
                    "preview should stay compact"
                );
                assert!(
                    preview.height() >= 60.0,
                    "preview collapsed at {window:?}: {preview:?}, dropdown {dropdown:?}, save {save:?}"
                );
            }
        }
    }
}
