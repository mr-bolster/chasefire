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
    textures: Vec<egui::TextureHandle>,
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

    fn ensure_loaded(&mut self, ui: &egui::Ui) {
        if !self.textures.is_empty() {
            return;
        }
        let sheet = pablo::sprites::Sheet::shared();
        self.cell = sheet.cell();
        for frame in 0..sheet.frame_count() {
            let image = egui::ColorImage::from_rgba_unmultiplied(
                [sheet.cell(), sheet.cell()],
                &sheet.frame_rgba(frame),
            );
            self.textures.push(ui.ctx().load_texture(
                format!("pablo-{frame}"),
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

        if presentation == Presentation::Plain {
            self.show_plain(ui, mood);
            return;
        }

        self.ensure_loaded(ui);
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
        // Shivering wobbles sideways: same animation, visibly unwell.
        if mood == Mood::Shivering {
            let jitter = if (now * 18.0) as i64 % 2 == 0 {
                1.0
            } else {
                -1.0
            };
            offset += 0.0;
            let rect = rect.translate(egui::vec2(jitter, offset));
            ui.painter()
                .image(self.textures[frame].id(), rect, full_uv(), tint(mood));
        } else {
            let rect = rect.translate(egui::vec2(0.0, offset));
            ui.painter()
                .image(self.textures[frame].id(), rect, full_uv(), tint(mood));
        }

        if let Some(strum) = self.strum {
            paint_flourish(ui, rect, strum);
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
fn tint(mood: Mood) -> egui::Color32 {
    let (red, green, blue) = mood.colour();
    egui::Color32::from_rgba_unmultiplied(
        200u8.saturating_add(red / 5),
        200u8.saturating_add(green / 5),
        200u8.saturating_add(blue / 5),
        255,
    )
}

/// The burst when a cue fires, drawn over him and fading.
fn paint_flourish(ui: &egui::Ui, rect: egui::Rect, strum: Strum) {
    let intensity = strum.intensity();
    let colour = match strum.flourish {
        Flourish::Midi => egui::Color32::from_rgb(240, 200, 60),
        Flourish::Osc => egui::Color32::from_rgb(90, 200, 230),
        Flourish::NetworkMidi => egui::Color32::from_rgb(180, 140, 240),
    };
    let colour = colour.gamma_multiply(intensity);

    // Until the real art arrives this is drawn rather than blitted: three
    // marks flying out and up from where the guitar is, growing as they fade.
    let origin = rect.center() + egui::vec2(rect.width() * 0.15, rect.height() * 0.1);
    let travel = (1.0 - intensity) * rect.width() * 0.55;
    for (index, angle) in [-0.9f32, -0.5, -0.1].iter().enumerate() {
        let position = origin + egui::vec2(angle.cos() * travel, angle.sin() * travel);
        let radius = 2.0 + index as f32 * 0.6 + (1.0 - intensity) * 2.0;
        ui.painter().circle_filled(position, radius, colour);
    }
}
