//! Chasefire, the window.
//!
//! Two modes, and the small one is the one that matters. It holds four things:
//! whether the show is armed, a way into the settings, the timecode, and
//! Pablo. That is the whole of it, because that is all anyone glances at while
//! doing three other jobs. Everything else lives behind Options.

mod pablo_view;

use eframe::egui;
use pablo::{Mood, Presentation};
use show::{Event, Runner};

fn main() -> eframe::Result {
    let arguments: Vec<String> = std::env::args().collect();
    // A hidden way to grab a picture of the window without a screenshot tool,
    // used for the documentation and for looking at it from a terminal.
    let shoot_to = arguments
        .windows(2)
        .find(|pair| pair[0] == "--screenshot")
        .map(|pair| pair[1].clone());

    let viewport = egui::ViewportBuilder::default()
        .with_inner_size([360.0, 132.0])
        .with_min_inner_size([300.0, 120.0])
        .with_always_on_top()
        .with_title("Chasefire");

    eframe::run_native(
        "Chasefire",
        eframe::NativeOptions {
            viewport,
            ..Default::default()
        },
        Box::new(|_context| Ok(Box::new(Window::new(shoot_to)))),
    )
}

struct Window {
    runner: Runner,
    pablo: pablo_view::PabloView,
    presentation: Presentation,
    always_on_top: bool,
    /// Frames left before taking a picture and quitting, if asked to.
    screenshot: Option<(String, u32)>,
    last_message: Option<String>,
}

impl Window {
    fn new(screenshot: Option<String>) -> Self {
        let mut runner = Runner::new(25);
        // Nothing is armed because a window opened. Arming is a decision.
        runner.set_armed(false);

        Self {
            runner,
            pablo: pablo_view::PabloView::new(),
            presentation: Presentation::default(),
            always_on_top: true,
            // A few frames of grace so the layout has settled before the shot.
            screenshot: screenshot.map(|path| (path, 8)),
            last_message: None,
        }
    }

    fn timecode_text(&self) -> String {
        match self.runner.timecode() {
            Some(timecode) => timecode.to_string(),
            None => "--:--:--:--".to_string(),
        }
    }
}

impl eframe::App for Window {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let context = ui.ctx().clone();
        let context = &context;
        for event in self.runner.poll() {
            self.last_message = Some(match event {
                Event::Fired { firing, sent } => match sent {
                    Ok(()) => {
                        self.pablo.fire(Runner::flourish_of(&firing));
                        format!("{} — {}", firing.at, firing.name)
                    }
                    Err(error) => format!("{} FAILED: {error}", firing.name),
                },
                Event::Locked { fps, nominal } => {
                    format!("locked {fps:.2} fps, counting at {nominal}")
                }
                Event::SignalLost => "signal lost".to_string(),
            });
        }

        let situation = self.runner.situation();
        let mood = Mood::read(situation);

        {
            ui.horizontal(|ui| {
                self.pablo
                    .show(ui, mood, self.presentation, self.runner.since_last_frame());

                ui.vertical(|ui| {
                    ui.add_space(4.0);

                    // The one thing that has to be readable across a dark room.
                    ui.label(
                        egui::RichText::new(self.timecode_text())
                            .monospace()
                            .size(30.0)
                            .strong(),
                    );

                    let (red, green, blue) = mood.colour();
                    ui.label(
                        egui::RichText::new(mood.describe())
                            .size(11.0)
                            .color(egui::Color32::from_rgb(red, green, blue)),
                    );

                    ui.add_space(2.0);
                    ui.horizontal(|ui| {
                        let armed = self.runner.is_armed();
                        let label = if armed { "ARMED" } else { "DISARMED" };
                        let colour = if armed {
                            egui::Color32::from_rgb(70, 200, 110)
                        } else {
                            egui::Color32::from_rgb(210, 150, 40)
                        };
                        let button = egui::Button::new(
                            egui::RichText::new(label)
                                .strong()
                                .color(egui::Color32::BLACK),
                        )
                        .fill(colour)
                        .min_size(egui::vec2(96.0, 26.0));

                        if ui.add(button).clicked() {
                            self.runner.set_armed(!armed);
                        }

                        if ui
                            .add(egui::Button::new("Options").min_size(egui::vec2(70.0, 26.0)))
                            .clicked()
                        {
                            self.last_message = Some("Options: not built yet".into());
                        }

                        // Always-on-top is the whole reason this window exists,
                        // but somebody will want it off while they work.
                        let pin = if self.always_on_top { "📌" } else { "📍" };
                        if ui
                            .add(egui::Button::new(pin).min_size(egui::vec2(28.0, 26.0)))
                            .on_hover_text("Keep this window above the others")
                            .clicked()
                        {
                            self.always_on_top = !self.always_on_top;
                            let level = if self.always_on_top {
                                egui::WindowLevel::AlwaysOnTop
                            } else {
                                egui::WindowLevel::Normal
                            };
                            ui.ctx()
                                .send_viewport_cmd(egui::ViewportCommand::WindowLevel(level));
                        }
                    });
                });
            });

            if let Some(message) = &self.last_message {
                ui.add_space(2.0);
                ui.label(egui::RichText::new(message).size(10.0).weak());
            }
        }

        // The space bar arms and disarms: the panic button must not need a
        // mouse, and it must not need the window to be the right size.
        if context.input(|input| input.key_pressed(egui::Key::Space)) {
            let armed = self.runner.is_armed();
            self.runner.set_armed(!armed);
        }

        if let Some((path, remaining)) = &mut self.screenshot {
            if *remaining == 0 {
                let path = path.clone();
                context.send_viewport_cmd(egui::ViewportCommand::Screenshot(egui::UserData::new(
                    path,
                )));
                self.screenshot = None;
            } else {
                *remaining -= 1;
            }
        }
        save_any_screenshot(context);

        // Animation, not a game: repaint often enough to move, rarely enough
        // to leave the machine alone. The audio thread is the one with a
        // deadline and it must never wait on this.
        context.request_repaint_after(std::time::Duration::from_millis(33));
    }
}

/// If the frame we just drew was captured, write it out and quit.
fn save_any_screenshot(context: &egui::Context) {
    let shot = context.input(|input| {
        input
            .events
            .iter()
            .rev()
            .filter_map(|event| match event {
                egui::Event::Screenshot {
                    image, user_data, ..
                } => Some((image.clone(), user_data.clone())),
                _ => None,
            })
            .next()
    });

    let Some((image, user_data)) = shot else {
        return;
    };
    let Some(path) = user_data
        .data
        .and_then(|data| data.downcast_ref::<String>().cloned())
    else {
        return;
    };

    let file = std::fs::File::create(&path).expect("screenshot path");
    let mut encoder = png::Encoder::new(
        std::io::BufWriter::new(file),
        image.width() as u32,
        image.height() as u32,
    );
    encoder.set_color(png::ColorType::Rgba);
    encoder.set_depth(png::BitDepth::Eight);
    let mut writer = encoder.write_header().expect("png header");
    let bytes: Vec<u8> = image
        .pixels
        .iter()
        .flat_map(|pixel| pixel.to_array())
        .collect();
    writer.write_image_data(&bytes).expect("png data");
    println!("screenshot written to {path}");
    std::process::exit(0);
}
