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

/// What the window was told on the way in. Everything here will eventually be
/// a setting behind Options; until then it is the only way to point it at a
/// sound card, and it is useful to keep even afterwards for a fixed install.
#[derive(Default)]
struct Startup {
    device: Option<String>,
    channel: usize,
    osc: Option<String>,
    cues: Option<String>,
    fps: Option<f64>,
    screenshot: Option<String>,
    /// Seconds to wait before taking it, so the shot can catch a running show
    /// rather than an empty window.
    screenshot_after: f32,
    /// Start armed. For an unattended installation — a machine that boots and
    /// runs a show with nobody in front of it — and for testing the whole chain
    /// without somebody having to be there to click. Nothing is hidden by it:
    /// the window says ARMED in green either way, and the log says where it
    /// came from.
    arm: bool,
    /// Force a flash on the first frame, so a picture can be taken of it.
    /// Documentation and eyeballing only — nothing fires here.
    demo_flash: Option<String>,
}

fn parse_startup() -> Startup {
    let arguments: Vec<String> = std::env::args().collect();
    let value = |flag: &str| {
        arguments
            .windows(2)
            .find(|pair| pair[0] == flag)
            .map(|pair| pair[1].clone())
    };
    Startup {
        device: value("--device"),
        channel: value("--channel")
            .and_then(|text| text.parse().ok())
            .unwrap_or(1),
        osc: value("--osc"),
        cues: value("--cues"),
        fps: value("--fps").and_then(|text| text.parse().ok()),
        screenshot: value("--screenshot"),
        screenshot_after: value("--screenshot-after")
            .and_then(|text| text.parse().ok())
            .unwrap_or(0.3),
        demo_flash: value("--demo-flash"),
        arm: arguments.iter().any(|argument| argument == "--arm"),
    }
}

fn main() -> eframe::Result {
    install_crash_log();
    let startup = parse_startup();
    let shoot_to = startup
        .screenshot
        .clone()
        .map(|path| (path, startup.screenshot_after));

    let viewport = egui::ViewportBuilder::default()
        .with_inner_size([404.0, 182.0])
        .with_min_inner_size([396.0, 178.0])
        .with_always_on_top()
        .with_title("Chasefire");

    eframe::run_native(
        "Chasefire",
        eframe::NativeOptions {
            viewport,
            ..Default::default()
        },
        Box::new(move |_context| Ok(Box::new(Window::new(startup, shoot_to)))),
    )
}

/// Write down why we died, somewhere findable.
///
/// A window that vanishes without a word is no use to anyone, and it is worse
/// than useless to someone in a dark room ten minutes before doors. Whatever
/// kills this goes to stderr *and* to a file next to the executable, because
/// the person it happens to will not have started it from a terminal.
fn install_crash_log() {
    let previous = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        let message = format!("chasefire died: {info}\n");
        eprint!("{message}");
        if let Some(path) = crash_log_path() {
            use std::io::Write;
            if let Ok(mut file) = std::fs::OpenOptions::new()
                .create(true)
                .append(true)
                .open(&path)
            {
                let _ = file.write_all(message.as_bytes());
                eprintln!("(also written to {})", path.display());
            }
        }
        previous(info);
    }));
}

fn crash_log_path() -> Option<std::path::PathBuf> {
    let executable = std::env::current_exe().ok()?;
    Some(executable.with_file_name("chasefire-crash.log"))
}

struct Window {
    runner: Runner,
    pablo: pablo_view::PabloView,
    presentation: Presentation,
    always_on_top: bool,
    /// Where to write a picture of the window, and when.
    screenshot: Option<(String, f32)>,
    /// The last couple of things that happened. Two lines is deliberate: it
    /// is enough to see a cue fire and still read what came before it, and not
    /// enough to turn the corner of someone's screen into a log window.
    log: std::collections::VecDeque<String>,
    /// A wash of colour across the whole window when something fires.
    flash: Option<Flash>,
}

/// The window flashing to say a cue went out.
///
/// A whole window changing colour is visible from much further away than a
/// 48-pixel sprite, which is the point: this is for the operator who is looking
/// somewhere else entirely.
/// Brief: this happens a lot during a show and must not become a strobe.
const FIRED_LENGTH: f32 = 0.34;
/// Longer: a cue that failed is worth interrupting somebody for.
const FAILED_LENGTH: f32 = 0.9;

#[derive(Clone, Copy)]
struct Flash {
    colour: egui::Color32,
    remaining: f32,
    length: f32,
    /// How opaque it gets at the moment of firing, out of 255.
    peak: f32,
}

impl Flash {
    /// A cue went out. Brief and gentle — this happens a lot during a show and
    /// must not become a strobe in the corner of somebody's eye.
    fn fired() -> Self {
        Self {
            colour: egui::Color32::from_rgb(70, 235, 125),
            remaining: FIRED_LENGTH,
            length: FIRED_LENGTH,
            peak: 150.0,
        }
    }

    /// A cue did NOT go out. Longer and stronger, because a cue that failed is
    /// the one thing in this window worth interrupting somebody for.
    fn failed() -> Self {
        Self {
            colour: egui::Color32::from_rgb(245, 70, 60),
            remaining: FAILED_LENGTH,
            length: FAILED_LENGTH,
            peak: 185.0,
        }
    }

    /// The colour to lay over the window right now.
    ///
    /// Full strength the instant it fires and then a fade, which is what makes
    /// it read as a flash rather than as a tint. Over the top of the content it
    /// can be much stronger than it could underneath and still leave the
    /// timecode legible through it — and it is over for good in a third of a
    /// second.
    fn wash(&self) -> egui::Color32 {
        let through = (self.remaining / self.length).clamp(0.0, 1.0);
        let alpha = (through.powf(0.75) * self.peak) as u8;
        egui::Color32::from_rgba_unmultiplied(
            self.colour.r(),
            self.colour.g(),
            self.colour.b(),
            alpha,
        )
    }
}

impl Window {
    fn new(startup: Startup, screenshot: Option<(String, f32)>) -> Self {
        let mut runner = Runner::new(25);
        // Nothing is armed because a window opened. Arming is a decision, and
        // the only way to make it by accident should be to say so out loud.
        runner.set_armed(startup.arm);
        runner.pin_frame_rate(startup.fps);

        let mut notes = Vec::new();
        if startup.arm {
            notes.push("armed from the command line".to_string());
        }

        if let Some(path) = &startup.cues {
            match std::fs::read_to_string(path)
                .map_err(|error| error.to_string())
                .and_then(|text| serde_json::from_str(&text).map_err(|error| error.to_string()))
            {
                Ok(cues) => {
                    let cues: Vec<cue::Cue> = cues;
                    notes.push(format!("{} cues loaded", cues.len()));
                    runner.set_cues(cues);
                }
                Err(error) => notes.push(format!("cues: {error}")),
            }
        }

        if let Some(target) = &startup.osc {
            match runner.connect_osc(target) {
                // The left-hand strip already shows where it goes, so
                // only a failure is worth a line in the log.
                Ok(()) => {}
                Err(error) => notes.push(format!("OSC: {error}")),
            }
        }

        // Open an input straight away. Left to itself it takes the default
        // one, so launching the thing with no arguments still does something
        // rather than sitting there looking broken.
        match runner.open_input(startup.device.as_deref(), startup.channel) {
            Ok(()) => {}
            // Not fatal, and deliberately so. No sound card is a thing to say
            // in the window, not a reason for the window to vanish.
            Err(error) => notes.push(format!("no input: {error}")),
        }

        Self {
            runner,
            pablo: pablo_view::PabloView::new(),
            presentation: Presentation::default(),
            always_on_top: true,
            // A few frames of grace so the layout has settled before the shot.
            screenshot,
            log: notes.into_iter().rev().take(2).collect(),
            flash: match startup.demo_flash.as_deref() {
                Some("failed") => Some(Flash::failed()),
                Some(_) => Some(Flash::fired()),
                None => None,
            },
        }
    }

    /// Add a line, keeping only the newest two.
    fn note(&mut self, line: String) {
        self.log.push_front(line);
        self.log.truncate(2);
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
            let line = match event {
                Event::Fired { firing, sent } => match sent {
                    Ok(()) => {
                        self.pablo.fire(Runner::flourish_of(&firing));
                        self.flash = Some(Flash::fired());
                        format!("{} — {}", firing.at, firing.name)
                    }
                    Err(error) => {
                        self.flash = Some(Flash::failed());
                        format!("{} FAILED: {error}", firing.name)
                    }
                },
                Event::Locked { fps, nominal } => {
                    format!("locked {fps:.2} fps, counting at {nominal}")
                }
                Event::SignalLost => "signal lost".to_string(),
            };
            self.note(line);
        }

        let situation = self.runner.situation();
        let mood = Mood::read(situation);

        // One set of measurements for the whole window, and one width that
        // everything else is derived from. Asking egui for the space left over
        // inside a nested layout gives a different answer at every depth, which
        // is how things end up hanging off the right-hand edge.
        const MARGIN: f32 = 10.0;
        const GAP: f32 = 8.0;
        const ROW: f32 = 30.0;
        // Measured from the panel's own rectangle, not from "space left over":
        // available_width changes with whatever has already been placed, which
        // is how one row ends at 393 and the next at 401.
        let panel = ui.max_rect();
        let content = panel.width() - MARGIN * 2.0;
        let left = panel.left() + MARGIN;
        let right = panel.right() - MARGIN;
        let pablo_width = 96.0;
        let column = content - pablo_width - GAP;

        ui.add_space(2.0);
        ui.horizontal(|ui| {
            // Same trap as every other row: egui's item spacing is added on top
            // of the distances set here, so the right-hand column started eight
            // pixels late and everything in it overhung the margin by eight.
            ui.spacing_mut().item_spacing.x = 0.0;
            ui.add_space(MARGIN);
            self.pablo
                .show(ui, mood, self.presentation, self.runner.since_last_frame());
            ui.add_space(GAP);

            ui.vertical(|ui| {
                // The only thing that has to be legible across a dark room.
                // The numbers carry the arm state too. It is the largest thing
                // in the window, so if it is green the show is live and if it
                // is amber it is not — readable from further away than the
                // button underneath it.
                let armed_colour = if self.runner.is_armed() {
                    egui::Color32::from_rgb(120, 235, 160)
                } else {
                    egui::Color32::from_rgb(235, 185, 95)
                };
                ui.label(
                    egui::RichText::new(self.timecode_text())
                        .monospace()
                        .size(40.0)
                        .strong()
                        .color(armed_colour),
                );

                let width = column;

                // What is being read, and what state it is in: two answers on
                // one line, pushed to opposite ends so neither has to be hunted.
                ui.allocate_ui(egui::vec2(width, 16.0), |ui| {
                    ui.horizontal(|ui| {
                        // Otherwise egui's own item spacing is added on top of
                        // the distances set here, and every row overflows by
                        // exactly as much as it has widgets.
                        ui.spacing_mut().item_spacing.x = 0.0;
                        let source = match (self.runner.source(), self.runner.frame_rate()) {
                            (Some(source), Some(rate)) => {
                                format!("{}  ·  {rate:.2} fps", source.label())
                            }
                            (Some(source), None) => format!("{}  ·  — fps", source.label()),
                            (None, _) => "no input".to_string(),
                        };
                        ui.label(
                            egui::RichText::new(source)
                                .size(12.0)
                                .strong()
                                .color(egui::Color32::from_rgb(185, 190, 200)),
                        );
                        // The short word, not the sentence: it balances against
                        // the source on the left instead of running off the
                        // edge. The full sentence is a hover away, and Pablo is
                        // saying the same thing in pictures anyway.
                        let (red, green, blue) = mood.colour();
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            ui.label(
                                egui::RichText::new(mood.badge())
                                    .size(12.0)
                                    .strong()
                                    .color(egui::Color32::from_rgb(red, green, blue)),
                            )
                            .on_hover_text(mood.describe());
                        });
                    });
                });

                // The countdown gets a line of its own. After the timecode it
                // is the number people actually want, and squeezing it in
                // beside something else was what made this look thrown together.
                ui.allocate_ui(egui::vec2(width, 20.0), |ui| {
                    match self.runner.countdown() {
                        Some((name, seconds)) => {
                            let colour = if seconds < 3.0 {
                                egui::Color32::from_rgb(235, 110, 90)
                            } else if seconds < 10.0 {
                                egui::Color32::from_rgb(235, 170, 70)
                            } else {
                                egui::Color32::from_rgb(140, 145, 155)
                            };
                            ui.horizontal(|ui| {
                                ui.spacing_mut().item_spacing.x = 0.0;
                                ui.label(
                                    egui::RichText::new(format!("next  {}", trim_to(&name, 24)))
                                        .size(13.0)
                                        .color(colour),
                                );
                                ui.with_layout(
                                    egui::Layout::right_to_left(egui::Align::Center),
                                    |ui| {
                                        ui.label(
                                            egui::RichText::new(format!("{seconds:.1}s"))
                                                .size(13.0)
                                                .strong()
                                                .color(colour),
                                        );
                                    },
                                );
                            });
                        }
                        None => {
                            ui.label(egui::RichText::new("no cue ahead").size(13.0).weak());
                        }
                    }
                });
            });
            ui.add_space(MARGIN);
        });

        ui.add_space(3.0);

        // The buttons span the whole width, not just the column beside Pablo.
        // The one that matters is the panic button, and it should be the
        // largest thing on screen after the timecode.
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 0.0;
            ui.add_space(MARGIN);
            let pin_width = 36.0;
            let options_width = 88.0;
            let arm_width = content - pin_width - options_width - GAP * 2.0;

            let armed = self.runner.is_armed();
            let (label, colour) = if armed {
                ("ARMED", egui::Color32::from_rgb(70, 200, 110))
            } else {
                ("DISARMED", egui::Color32::from_rgb(210, 150, 40))
            };
            // The only way to arm or disarm, on purpose. There is no keyboard
            // shortcut and there should not be: this window sits above
            // everything else, so it can take focus without anyone noticing,
            // and a stray key that silently disarms a running show is a worse
            // problem than having to aim at a button.
            let button = egui::Button::new(
                egui::RichText::new(label)
                    .size(15.0)
                    .strong()
                    .color(egui::Color32::BLACK),
            )
            .fill(colour)
            .min_size(egui::vec2(arm_width, ROW));
            if ui.add(button).clicked() {
                self.runner.set_armed(!armed);
            }

            ui.add_space(GAP);
            if ui
                .add(egui::Button::new("Options").min_size(egui::vec2(options_width, ROW)))
                .clicked()
            {
                self.note("Options: not built yet".into());
            }

            ui.add_space(GAP);
            let pin_button = if self.always_on_top {
                egui::Button::new(
                    egui::RichText::new("PIN")
                        .strong()
                        .color(egui::Color32::BLACK),
                )
                .fill(egui::Color32::from_rgb(70, 200, 110))
            } else {
                egui::Button::new(egui::RichText::new("pin").weak())
            };
            if ui
                .add(pin_button.min_size(egui::vec2(pin_width, ROW)))
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

        // A rule drawn between the margins. egui's own separator bleeds to the
        // panel edge, which left it as the only thing in the window not lining
        // up with everything else.
        ui.add_space(5.0);
        let rule = ui.cursor().top();
        ui.painter().hline(
            left..=right,
            rule,
            egui::Stroke::new(1.0, ui.visuals().weak_text_color().gamma_multiply(0.4)),
        );
        ui.add_space(4.0);

        // The footer, split. Left: what this is wired to, so nobody has to
        // remember. Right: what just happened.
        ui.horizontal_top(|ui| {
            ui.spacing_mut().item_spacing.x = 0.0;
            ui.add_space(MARGIN);
            let half = (content - GAP) * 0.5;

            // Left: what this is wired to. The labels are a fixed-width column
            // so the values line up under each other instead of stepping in and
            // out with the length of the word before them.
            ui.allocate_ui(egui::vec2(half, 30.0), |ui| {
                ui.vertical(|ui| {
                    // add_sized forces the label column to a fixed width;
                    // allocate_ui only reserves what the content happens to
                    // use, which with spacing switched off glued "in" straight
                    // onto the device name.
                    let mut row = |label: &str, value: String| {
                        ui.horizontal(|ui| {
                            ui.spacing_mut().item_spacing.x = 0.0;
                            ui.add_sized(
                                [30.0, 13.0],
                                egui::Label::new(small(label)).halign(egui::Align::LEFT),
                            );
                            ui.label(small(&value));
                        });
                    };
                    row(
                        "in",
                        match self.runner.device_name() {
                            Some(name) => format!(
                                "{}  ch {}",
                                shorten(name),
                                self.runner.channel().unwrap_or(1)
                            ),
                            None => "—".to_string(),
                        },
                    );
                    row(
                        "out",
                        match self.runner.output_target() {
                            Some(target) => format!("OSC {target}"),
                            None => "—".to_string(),
                        },
                    );
                });
            });

            // A hairline between the two halves, placed from the same geometry
            // as everything else rather than from wherever the cursor happens
            // to have ended up.
            let divider = left + half + GAP * 0.5;
            let top = ui.cursor().top();
            ui.painter().vline(
                divider,
                top..=(top + 26.0),
                egui::Stroke::new(1.0, ui.visuals().weak_text_color().gamma_multiply(0.4)),
            );

            ui.add_space(GAP);
            ui.allocate_ui(egui::vec2(half, 30.0), |ui| {
                ui.vertical(|ui| {
                    ui.spacing_mut().item_spacing.y = 1.0;
                    // Clipped to its half. A log line long enough to reach
                    // across the divider makes the split look accidental.
                    for line in &self.log {
                        ui.label(small(&trim_to(line, 34)));
                    }
                });
            });
        });

        // The flash goes on a foreground layer, painted after everything and
        // above it. Behind the content it was almost invisible: the widgets
        // cover most of the window, so the wash only showed in the gaps.
        if let Some(flash) = &mut self.flash {
            flash.remaining -= context.input(|input| input.stable_dt);
            if flash.remaining <= 0.0 {
                self.flash = None;
            } else {
                let painter = context.layer_painter(egui::LayerId::new(
                    egui::Order::Foreground,
                    egui::Id::new("cue-flash"),
                ));
                painter.rect_filled(ui.max_rect(), 0.0, flash.wash());
            }
        }

        if let Some((path, remaining)) = &mut self.screenshot {
            *remaining -= context.input(|input| input.stable_dt);
            if *remaining <= 0.0 {
                let path = path.clone();
                context.send_viewport_cmd(egui::ViewportCommand::Screenshot(egui::UserData::new(
                    path,
                )));
                self.screenshot = None;
            }
        }
        save_any_screenshot(context);

        // Animation, not a game: repaint often enough to move, rarely enough
        // to leave the machine alone. The audio thread is the one with a
        // deadline and it must never wait on this.
        context.request_repaint_after(std::time::Duration::from_millis(33));
    }
}

/// Cue names are written for a cue list, not for a strip of window this wide.
fn trim_to(text: &str, room: usize) -> String {
    if text.chars().count() <= room {
        return text.to_string();
    }
    text.chars()
        .take(room.saturating_sub(1))
        .collect::<String>()
        + "…"
}

/// Small, dim text: present when looked for, invisible when not.
fn small(text: &str) -> egui::RichText {
    egui::RichText::new(text).size(10.0).weak()
}

/// Device names carry the whole ALSA or WASAPI incantation, which is no use in
/// a strip this wide. Keep the end, which is the part that identifies the card.
fn shorten(name: &str) -> String {
    const ROOM: usize = 22;
    if name.chars().count() <= ROOM {
        return name.to_string();
    }
    let tail: String = name
        .chars()
        .skip(name.chars().count().saturating_sub(ROOM - 1))
        .collect();
    format!("…{tail}")
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
