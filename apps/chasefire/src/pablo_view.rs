//! Drawing Pablo.
//!
//! Every frame of the sheet is uploaded once as its own texture and then
//! picked between — 46 tiny textures cost nothing and it keeps the drawing
//! code down to one line. Scaling is nearest-neighbour at whole multiples
//! only: pixel art scaled by 1.5 turns to porridge.

use eframe::egui;
use pablo::{Flourish, Mood, Presentation, Strum};

/// Roughly how big he should end up on screen, in points.
const TARGET_SIZE: f32 = 96.0;

/// Whole-number scaling only. A sprite drawn at 1.5x turns to porridge, so the
/// scale is whatever integer gets closest to [`TARGET_SIZE`] without going
/// under 1 — which also means the art can change size and nothing else has to.
fn scale_for(cell: usize) -> usize {
    ((TARGET_SIZE / cell as f32).round() as usize).max(1)
}

pub struct PabloView {
    /// Textures per skin, loaded the first time that skin is shown.
    textures: Vec<egui::TextureHandle>,
    loaded: Option<Presentation>,
    cell: usize,
    /// Which frame of the current loop we are on.
    step: usize,
    last_step_at: f64,
    strum: Option<Strum>,
    previous: f64,
}

impl PabloView {
    pub fn new() -> Self {
        Self {
            textures: Vec::new(),
            loaded: None,
            cell: 0,
            step: 0,
            last_step_at: 0.0,
            strum: None,
            previous: 0.0,
        }
    }

    /// A cue just went out. Give him something to throw.
    pub fn fire(&mut self, flourish: Flourish) {
        self.strum = Some(Strum::new(flourish));
    }

    fn ensure_loaded(&mut self, ui: &egui::Ui, presentation: Presentation) {
        if self.loaded == Some(presentation) {
            return;
        }
        self.textures.clear();
        self.loaded = Some(presentation);
        let sheet = pablo::sprites::Sheet::shared(presentation);
        self.cell = sheet.cell();
        for frame in 0..sheet.frame_count() {
            let image = egui::ColorImage::from_rgba_unmultiplied(
                [sheet.cell(), sheet.cell()],
                &sheet.frame_rgba(frame),
            );
            self.textures.push(ui.ctx().load_texture(
                format!("{presentation:?}-{frame}"),
                image,
                egui::TextureOptions::NEAREST,
            ));
        }
    }

    pub fn show(
        &mut self,
        ui: &mut egui::Ui,
        mood: Mood,
        presentation: Presentation,
        since_last_frame: Option<f32>,
    ) {
        let now = ui.input(|input| input.time);
        let delta = (now - self.previous).max(0.0) as f32;
        self.previous = now;

        if let Some(strum) = &mut self.strum {
            if !strum.tick(delta) {
                self.strum = None;
            }
        }

        self.ensure_loaded(ui, presentation);
        if self.textures.is_empty() {
            self.show_plain(ui, mood);
            return;
        }

        // Step the loop on its own clock, so a mood with slow breathing does
        // not animate at the same rate as one with a strumming arm.
        let interval = 1.0 / mood.animation_fps() as f64;
        if now - self.last_step_at >= interval {
            self.last_step_at = now;
            self.step = self.step.wrapping_add(1);
        }

        let range = pablo::sprites::frames_for(mood);
        let frame = range.start + self.step % range.len();

        let scale = scale_for(self.cell);
        let size = (self.cell * scale) as f32;
        let (rect, _) = ui.allocate_exact_size(egui::vec2(size, size), egui::Sense::hover());

        // The nod comes from the timecode, not from a free-running clock: he
        // dips on the beat of the seconds actually arriving. So if the signal
        // starts limping, Pablo limps with it and you see it before you read it.
        let mut offset = 0.0;
        if matches!(mood, Mood::Playing | Mood::Shivering) {
            if let Some(seconds) = since_last_frame {
                let beat = (seconds * 4.0).fract();
                offset = if beat < 0.25 { -(scale as f32) } else { 0.0 };
            }
        }
        // Shivering has its own drawn frames now, so nothing is faked here.
        let rect = rect.translate(egui::vec2(0.0, offset));
        ui.painter().image(
            self.textures[frame].id(),
            rect,
            full_uv(),
            tint(mood, presentation),
        );

        // The burst goes over the top, on the same square, stepping through its
        // own frames and fading as it goes.
        if let Some(strum) = self.strum {
            let range = pablo::sprites::frames_for_flourish(strum.flourish);
            let through = 1.0 - strum.intensity();
            let step = ((through * range.len() as f32) as usize).min(range.len() - 1);
            let fade = (strum.intensity() * 1.6).min(1.0);
            ui.painter().image(
                self.textures[range.start + step].id(),
                rect,
                full_uv(),
                egui::Color32::WHITE.gamma_multiply(fade),
            );
        }
    }

    /// Pablo switched off. The same five states, in sober clothes — a colour
    /// and a word, because losing the cartoon must not mean losing the news.
    fn show_plain(&mut self, ui: &mut egui::Ui, mood: Mood) {
        let size = TARGET_SIZE;
        let (rect, _) = ui.allocate_exact_size(egui::vec2(size, size), egui::Sense::hover());
        let (red, green, blue) = mood.colour();
        let colour = egui::Color32::from_rgb(red, green, blue);

        ui.painter()
            .rect_filled(rect, 4.0, colour.gamma_multiply(0.25));
        ui.painter().rect_stroke(
            rect,
            4.0,
            egui::Stroke::new(2.0, colour),
            egui::StrokeKind::Inside,
        );
        ui.painter().text(
            rect.center(),
            egui::Align2::CENTER_CENTER,
            mood.badge(),
            egui::FontId::proportional(11.0),
            colour,
        );
    }
}

fn full_uv() -> egui::Rect {
    egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0))
}

/// A wash of the mood's colour over him, so the state reads even at a glance
/// too quick to make out what he is doing.
fn tint(mood: Mood, presentation: Presentation) -> egui::Color32 {
    // The transport marks are already drawn in their state's colour. Washing
    // them again would only mute them.
    if presentation == Presentation::Plain {
        return egui::Color32::WHITE;
    }
    let (red, green, blue) = mood.colour();
    egui::Color32::from_rgba_unmultiplied(
        200u8.saturating_add(red / 5),
        200u8.saturating_add(green / 5),
        200u8.saturating_add(blue / 5),
        255,
    )
}
