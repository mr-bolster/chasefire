//! Options: everything the corner window deliberately does not show.
//!
//! It opens as a second window rather than growing the first one. The small
//! window is meant to be glanced at while doing something else; a settings
//! screen is meant to be read while doing nothing else. Putting both in one
//! frame would spoil whichever you were not using.
//!
//! Laid out on a grid from the start, with one set of measurements and one
//! label column that every field lines up against. The corner window was built
//! by adding rows until it looked wrong and then measuring it back into shape;
//! this one is not going to repeat that.

use eframe::egui;
use pablo::Presentation;
use show::Runner;

/// The whole layout, in four numbers.
const MARGIN: f32 = 14.0;
const GAP: f32 = 10.0;
/// Every label sits in a column this wide, so every field starts at the same x.
const LABEL: f32 = 118.0;
/// And every field is this wide unless it has a reason not to be.
const FIELD: f32 = 220.0;
/// One row of the cue list, near enough, for working out how many fit.
const CUE_ROW: f32 = 23.0;
/// How many cues should be visible without scrolling when there is room.
const CUES_VISIBLE: f32 = 20.0;

/// Where the money goes. Straight there, one click, no detour through a page
/// that only exists to hold a link.
///
/// A paypal.me handle and not an email address, which matters: the handle is
/// public by design and made to be shared, while an email address in a public
/// repository is an address that harvesters will find.
pub const DONATE_URL: &str = "https://paypal.me/mrbolster";
const HOME_PAGE: &str = "https://mrbolster.app";

/// Rates worth offering, and what each is called on a spec sheet.
const RATES: [(&str, f64); 7] = [
    ("23.98", 24_000.0 / 1001.0),
    ("24", 24.0),
    ("25", 25.0),
    ("29.97", 30_000.0 / 1001.0),
    ("30", 30.0),
    ("50", 50.0),
    ("60", 60.0),
];

pub struct Options {
    pub open: bool,
    devices: Vec<audio::DeviceInfo>,
    devices_listed: bool,
    chosen_device: Option<String>,
    channel: usize,
    /// `None` means work the frame rate out from the signal.
    pinned_fps: Option<f64>,
    osc_target: String,
    cue_path: String,
    message: Option<String>,
}

impl Options {
    pub fn new(
        device: Option<String>,
        channel: usize,
        osc: Option<String>,
        cues: Option<String>,
    ) -> Self {
        Self {
            open: false,
            devices: Vec::new(),
            devices_listed: false,
            chosen_device: device,
            channel: channel.max(1),
            pinned_fps: None,
            osc_target: osc.unwrap_or_else(|| "127.0.0.1:7000".into()),
            cue_path: cues.unwrap_or_default(),
            message: None,
        }
    }

    /// Enumerating audio devices is slow enough to be worth doing once, and
    /// slow enough that doing it every frame would make the window stutter.
    fn devices(&mut self) -> &[audio::DeviceInfo] {
        if !self.devices_listed {
            self.devices_listed = true;
            self.devices = audio::list_input_devices().unwrap_or_default();
        }
        &self.devices
    }

    pub fn show(
        &mut self,
        context: &egui::Context,
        runner: &mut Runner,
        presentation: &mut Presentation,
    ) {
        if !self.open {
            return;
        }

        let viewport = egui::ViewportId::from_hash_of("chasefire-options");
        let builder = egui::ViewportBuilder::default()
            .with_title("Chasefire — Options")
            // Tall enough that twenty cues fit without scrolling, and wide
            // enough that an OSC address is readable without dragging it out.
            .with_inner_size([720.0, 860.0])
            .with_min_inner_size([540.0, 420.0]);

        let mut still_open = true;
        context.show_viewport_immediate(viewport, builder, |context, _class| {
            egui::CentralPanel::default()
                .frame(egui::Frame::central_panel(context.style().as_ref()).inner_margin(MARGIN))
                .show(context, |ui| {
                    // Measured here, from the panel itself, and handed down.
                    // Asking for "the width still available" inside a scroll
                    // area inside a grid gives an answer that has nothing to do
                    // with how wide the window is — the same trap that had
                    // things hanging off the edge of the main window.
                    let width = ui.max_rect().width();
                    egui::ScrollArea::vertical().show(ui, |ui| {
                        self.contents(ui, runner, presentation, width);
                    });
                });
            if context.input(|input| input.viewport().close_requested()) {
                still_open = false;
            }
        });
        self.open = still_open;
    }

    fn contents(
        &mut self,
        ui: &mut egui::Ui,
        runner: &mut Runner,
        presentation: &mut Presentation,
        width: f32,
    ) {
        self.input_section(ui, runner);
        self.outputs_section(ui, runner);
        self.cues_section(ui, runner, width);
        self.timing_section(ui, runner);
        appearance_section(ui, presentation);
        support_section(ui);

        if let Some(message) = &self.message {
            ui.add_space(GAP);
            ui.separator();
            ui.add_space(4.0);
            ui.label(egui::RichText::new(message).size(12.0));
        }
    }

    fn input_section(&mut self, ui: &mut egui::Ui, runner: &mut Runner) {
        section(ui, "Input");
        grid(ui, "input", |ui| {
            label(ui, "Device");
            let current = self
                .chosen_device
                .clone()
                .unwrap_or_else(|| "system default".into());
            egui::ComboBox::from_id_salt("device")
                .selected_text(shorten(&current, 44))
                .width(FIELD)
                .show_ui(ui, |ui| {
                    ui.selectable_value(&mut self.chosen_device, None, "system default");
                    let names: Vec<String> = self
                        .devices()
                        .iter()
                        .map(|device| device.name.clone())
                        .collect();
                    for name in names {
                        ui.selectable_value(&mut self.chosen_device, Some(name.clone()), &name);
                    }
                });
            ui.end_row();

            label(ui, "Channel");
            ui.horizontal(|ui| {
                ui.add(egui::DragValue::new(&mut self.channel).range(1..=64));
                hint(ui, "which channel of that input carries the timecode");
            });
            ui.end_row();

            label(ui, "Frame rate");
            ui.horizontal(|ui| {
                let current = match self.pinned_fps {
                    None => "work it out".to_string(),
                    Some(rate) => RATES
                        .iter()
                        .find(|(_, value)| (value - rate).abs() < 0.01)
                        .map(|(name, _)| name.to_string())
                        .unwrap_or_else(|| format!("{rate:.2}")),
                };
                egui::ComboBox::from_id_salt("fps")
                    .selected_text(current)
                    .width(120.0)
                    .show_ui(ui, |ui| {
                        ui.selectable_value(&mut self.pinned_fps, None, "work it out");
                        for (name, rate) in RATES {
                            ui.selectable_value(&mut self.pinned_fps, Some(rate), name);
                        }
                    });
                // Not a detail: it is two frames of lock time, and on a stage
                // that is the difference between catching the first cue and
                // missing it.
                hint(
                    ui,
                    "told the rate it locks on the first frame; left to work it out, the third",
                );
            });
            ui.end_row();

            label(ui, "");
            ui.horizontal(|ui| {
                if ui.button("Apply and listen").clicked() {
                    runner.pin_frame_rate(self.pinned_fps);
                    match runner.open_input(self.chosen_device.as_deref(), self.channel) {
                        Ok(()) => self.message = Some("listening".into()),
                        Err(error) => self.message = Some(error.to_string()),
                    }
                }
                if runner.is_listening() {
                    if ui.button("Stop").clicked() {
                        runner.close_input();
                        self.message = Some("input closed".into());
                    }
                    hint(ui, "applying restarts the input");
                }
            });
            ui.end_row();
        });
    }

    fn outputs_section(&mut self, ui: &mut egui::Ui, runner: &mut Runner) {
        section(ui, "Outputs");
        grid(ui, "outputs", |ui| {
            label(ui, "OSC");
            ui.horizontal(|ui| {
                ui.add(
                    egui::TextEdit::singleline(&mut self.osc_target).desired_width(FIELD - 80.0),
                );
                if ui.button("Connect").clicked() {
                    match runner.connect_osc(&self.osc_target) {
                        Ok(()) => self.message = Some(format!("sending to {}", self.osc_target)),
                        Err(error) => self.message = Some(error),
                    }
                }
            });
            ui.end_row();

            // The other two are listed rather than hidden. Somebody deciding
            // whether this tool fits their rig needs to know what it will and
            // will not do, and finding that out by not finding a setting is a
            // poor way to learn it.
            label(ui, "MIDI");
            ui.horizontal(|ui| {
                ui.add_enabled_ui(false, |ui| {
                    let mut empty = String::from("— no port —");
                    ui.add(egui::TextEdit::singleline(&mut empty).desired_width(FIELD - 80.0));
                });
                hint(ui, "not built yet");
            });
            ui.end_row();

            label(ui, "MIDI over network");
            ui.horizontal(|ui| {
                ui.add_enabled_ui(false, |ui| {
                    let mut empty = String::from("— no session —");
                    ui.add(egui::TextEdit::singleline(&mut empty).desired_width(FIELD - 80.0));
                });
                hint(ui, "not built yet");
            });
            ui.end_row();
        });
        ui.add_space(2.0);
        hint(
            ui,
            "RTP-MIDI will speak the protocol itself, so there is nothing to install and no driver for an update to break.",
        );
    }

    fn cues_section(&mut self, ui: &mut egui::Ui, runner: &mut Runner, width: f32) {
        section(ui, "Cues");
        grid(ui, "cuefile", |ui| {
            label(ui, "File");
            ui.horizontal(|ui| {
                ui.add(egui::TextEdit::singleline(&mut self.cue_path).desired_width(FIELD));
                if ui.button("Load").clicked() {
                    match runner.load_cues(std::path::Path::new(&self.cue_path)) {
                        Ok(count) => self.message = Some(format!("{count} cues loaded")),
                        Err(error) => self.message = Some(error),
                    }
                }
            });
            ui.end_row();
        });

        ui.add_space(4.0);
        self.cue_table(ui, runner, width);
    }

    /// The cue list, editable in place.
    ///
    /// Editing writes straight through to the engine, which re-arms everything
    /// and forgets where it was. That is correct, and it means editing during a
    /// show is a real interruption rather than a quiet one — hence the warning.
    fn cue_table(&mut self, ui: &mut egui::Ui, runner: &mut Runner, width: f32) {
        let mut cues = runner.cues().to_vec();
        let mut changed = false;
        let mut remove = None;

        // Every text field grows with the window, each with a floor below which
        // it stops being usable. The timecode needs the least — it is always
        // eleven characters — but cramming it into exactly eleven characters
        // makes it fiddly to click into, so it gets a share too.
        let fixed = 34.0 + 66.0 + 72.0 + GAP * 5.0;
        let flexible = (width - fixed - 20.0).max(280.0);
        let at_width = (flexible * 0.14).max(96.0);
        let name_width = (flexible * 0.30).max(110.0);
        let address_width = (flexible - at_width - name_width).max(150.0);

        if cues.is_empty() {
            hint(ui, "no cues yet — add one below");
        } else {
            egui::ScrollArea::vertical()
                .max_height(CUE_ROW * CUES_VISIBLE)
                .id_salt("cuelist")
                .show(ui, |ui| {
                    egui::Grid::new("cues")
                        .num_columns(5)
                        .spacing([GAP, 4.0])
                        .striped(true)
                        .show(ui, |ui| {
                            ui.label(egui::RichText::new("on").size(11.0).weak());
                            ui.label(egui::RichText::new("at").size(11.0).weak());
                            ui.label(egui::RichText::new("name").size(11.0).weak());
                            ui.horizontal(|ui| {
                                ui.label(egui::RichText::new("sends").size(11.0).weak());
                                ui.add_space(address_width - 34.0);
                                ui.label(egui::RichText::new("value").size(11.0).weak())
                                    .on_hover_text(
                                        "The argument that goes with the address. Resolume treats 1 \
                                         as \"do it\" and 0 as \"undo it\"; a fader would take the \
                                         level instead.",
                                    );
                            });
                            ui.label("");
                            ui.end_row();

                            for (index, cue) in cues.iter_mut().enumerate() {
                                changed |= ui.checkbox(&mut cue.enabled, "").changed();

                                // Typed the way the trade writes it, and only
                                // accepted once it is actually a timecode: a
                                // half-typed one must not move a cue to midnight.
                                let mut text = cue.at.to_string();
                                if ui
                                    .add(
                                        egui::TextEdit::singleline(&mut text)
                                            .desired_width(at_width)
                                            .font(egui::TextStyle::Monospace),
                                    )
                                    .on_hover_text("HH:MM:SS:FF — a semicolon before the frames means drop frame")
                                    .changed()
                                {
                                    if let Some(parsed) = parse_timecode(&text) {
                                        cue.at = parsed;
                                        changed = true;
                                    }
                                }

                                changed |= ui
                                    .add(
                                        egui::TextEdit::singleline(&mut cue.name)
                                            .desired_width(name_width),
                                    )
                                    .changed();

                                changed |= action_editor(ui, &mut cue.action, address_width);

                                if ui.small_button("remove").clicked() {
                                    remove = Some(index);
                                }
                                ui.end_row();
                            }
                        });
                });
        }

        ui.add_space(4.0);
        ui.horizontal(|ui| {
            if ui.button("Add cue").clicked() {
                let next = cues.iter().map(|cue| cue.id).max().unwrap_or(0) + 1;
                cues.push(cue::Cue::new(
                    next,
                    format!("cue {next}"),
                    ltc::Timecode::new(10, 0, 0, 0),
                    cue::Action::Osc {
                        address: "/composition/columns/1/connect".into(),
                        args: vec![cue::OscArg::Int(1)],
                    },
                ));
                changed = true;
            }
            if ui.button("Save to file").clicked() {
                match save_cues(&self.cue_path, &cues) {
                    Ok(()) => self.message = Some(format!("written to {}", self.cue_path)),
                    Err(error) => self.message = Some(error),
                }
            }
            if runner.is_armed() {
                hint(ui, "editing re-arms every cue and re-syncs");
            }
        });

        if let Some(index) = remove {
            cues.remove(index);
            changed = true;
        }
        if changed {
            runner.set_cues(cues);
        }
    }

    fn timing_section(&mut self, ui: &mut egui::Ui, runner: &mut Runner) {
        section(ui, "Timing");
        grid(ui, "timing", |ui| {
            label(ui, "Offset");
            ui.horizontal(|ui| {
                let mut offset = runner.offset_frames();
                if ui
                    .add(
                        egui::DragValue::new(&mut offset)
                            .range(-50..=50)
                            .suffix(" frames"),
                    )
                    .changed()
                {
                    runner.set_offset_frames(offset);
                }
                hint(ui, "positive fires early, to cancel the delay of the card, the network and the far end");
            });
            ui.end_row();

            label(ui, "Freewheel");
            ui.horizontal(|ui| {
                let mut freewheel = runner.freewheel_frames();
                if ui
                    .add(
                        egui::DragValue::new(&mut freewheel)
                            .range(0..=120)
                            .suffix(" frames"),
                    )
                    .changed()
                {
                    runner.set_freewheel_frames(freewheel);
                }
                hint(ui, "how long to keep counting after the signal goes; the trade uses eight to forty");
            });
            ui.end_row();
        });
    }
}

fn appearance_section(ui: &mut egui::Ui, presentation: &mut Presentation) {
    section(ui, "Appearance");
    grid(ui, "appearance", |ui| {
        label(ui, "Status display");
        ui.horizontal(|ui| {
            ui.selectable_value(presentation, Presentation::Pablo, "Pablo");
            ui.selectable_value(presentation, Presentation::Plain, "Transport marks");
        });
        ui.end_row();
    });
    ui.add_space(2.0);
    hint(
        ui,
        "Both say exactly the same five things. One is a little guitarist and the other is not.",
    );
}

/// The ask, and who did what.
///
/// Kept honest and kept short. The software is free and stays free; what is
/// being asked for is a contribution from people who use it to earn a living,
/// and there is no nagging, no timer and nothing withheld if nobody pays.
fn support_section(ui: &mut egui::Ui) {
    section(ui, "Support this");
    ui.label(
        egui::RichText::new(
            "Chasefire is free software and always will be — source, licence, all of it. \
             There is no trial, no expiry and nothing switched off if you never pay a penny.",
        )
        .size(12.0),
    );
    ui.add_space(4.0);
    ui.label(
        egui::RichText::new(
            "If it earns you money, a one-off contribution keeps it being worked on. \
             Pay what you think it is worth, once. Never a subscription.",
        )
        .size(12.0),
    );
    ui.add_space(8.0);
    ui.horizontal(|ui| {
        // Deliberately a button and not a link buried in a sentence. Somebody
        // who has decided to pay should not have to hunt for where.
        let donate = egui::Button::new(
            egui::RichText::new("Donate")
                .size(14.0)
                .strong()
                .color(egui::Color32::WHITE),
        )
        .fill(egui::Color32::from_rgb(0, 112, 186))
        .min_size(egui::vec2(190.0, 32.0));

        if ui.add(donate).on_hover_text(DONATE_URL).clicked() {
            ui.ctx().open_url(egui::OpenUrl::new_tab(DONATE_URL));
        }

        ui.add_space(GAP);
        ui.vertical(|ui| {
            ui.hyperlink_to("mrbolster.app", HOME_PAGE);
            hint(ui, "downloads and everything else");
        });
    });

    ui.add_space(GAP);
    section(ui, "About");

    grid(ui, "about", |ui| {
        label(ui, "Version");
        ui.horizontal(|ui| {
            // Straight from the manifest, so it can never disagree with what
            // was actually built. Somebody reporting a fault will be reading
            // this line out loud, and it has to be the truth.
            ui.label(
                egui::RichText::new(env!("CARGO_PKG_VERSION"))
                    .monospace()
                    .strong(),
            );
            hint(ui, "chase timecode, fire cues");
        });
        ui.end_row();

        label(ui, "Built for");
        ui.label("live shows: LTC in, cues out, on the machine you already own");
        ui.end_row();

        label(ui, "Written by");
        ui.label("Leo Bolster");
        ui.end_row();

        label(ui, "Pablo drawn by");
        ui.label("commissioned pixel art");
        ui.end_row();

        label(ui, "Licence");
        ui.horizontal(|ui| {
            ui.label("GPL-3.0-or-later");
            hint(ui, "the source is open and stays open");
        });
        ui.end_row();

        label(ui, "Source");
        ui.hyperlink_to(
            "github.com/mr-bolster/chasefire",
            "https://github.com/mr-bolster/chasefire",
        );
        ui.end_row();

        label(ui, "Settings live in");
        ui.label(
            egui::RichText::new(crate::settings::Settings::location())
                .size(11.0)
                .monospace(),
        )
        .on_hover_text(
            "Put a file called chasefire.json next to the executable and it will be used instead — \
             for a stick that travels with its own setup.",
        );
        ui.end_row();

        label(ui, "Standing on");
        ui.label(egui::RichText::new("egui · cpal · SMPTE 12M-1, read the hard way").size(12.0));
        ui.end_row();
    });
}

// ---------------------------------------------------------------- layout bits

fn section(ui: &mut egui::Ui, title: &str) {
    ui.add_space(GAP);
    ui.label(egui::RichText::new(title).size(15.0).strong());
    ui.separator();
    ui.add_space(2.0);
}

fn grid(ui: &mut egui::Ui, id: &str, contents: impl FnOnce(&mut egui::Ui)) {
    egui::Grid::new(id)
        .num_columns(2)
        .spacing([GAP, 6.0])
        .show(ui, contents);
}

/// Every label in the same column, so every field starts at the same x.
fn label(ui: &mut egui::Ui, text: &str) {
    ui.add_sized(
        [LABEL, 18.0],
        egui::Label::new(egui::RichText::new(text).size(12.0)).halign(egui::Align::LEFT),
    );
}

fn hint(ui: &mut egui::Ui, text: &str) {
    ui.label(egui::RichText::new(text).size(11.0).weak());
}

fn shorten(text: &str, room: usize) -> String {
    if text.chars().count() <= room {
        return text.to_string();
    }
    let tail: String = text
        .chars()
        .skip(text.chars().count().saturating_sub(room - 1))
        .collect();
    format!("…{tail}")
}

/// Edit whatever a cue does. Only OSC exists so far, so this is one row; when
/// MIDI arrives it becomes a choice of kind and then its own fields.
fn action_editor(ui: &mut egui::Ui, action: &mut cue::Action, width: f32) -> bool {
    match action {
        cue::Action::Osc { address, args } => {
            let mut changed = false;
            ui.horizontal(|ui| {
                changed = ui
                    .add(egui::TextEdit::singleline(address).desired_width(width))
                    .changed();
                if let Some(cue::OscArg::Int(value)) = args.first_mut() {
                    changed |= ui
                        .add(egui::DragValue::new(value))
                        .on_hover_text(
                            "The value sent with the address. For Resolume, 1 triggers and 0 \
                             releases; for something like a fader it would be the level.",
                        )
                        .changed();
                }
            });
            changed
        }
        other => {
            ui.label(format!("{other:?}"));
            false
        }
    }
}

/// Accept a timecode only when it is one.
fn parse_timecode(text: &str) -> Option<ltc::Timecode> {
    let drop_frame = text.contains(';');
    let parts: Vec<&str> = text.split([':', ';']).collect();
    if parts.len() != 4 {
        return None;
    }
    let mut numbers = [0u8; 4];
    for (index, part) in parts.iter().enumerate() {
        numbers[index] = part.trim().parse().ok()?;
    }
    let timecode = ltc::Timecode {
        hours: numbers[0],
        minutes: numbers[1],
        seconds: numbers[2],
        frames: numbers[3],
        drop_frame,
    };
    timecode.is_plausible().then_some(timecode)
}

fn save_cues(path: &str, cues: &[cue::Cue]) -> Result<(), String> {
    if path.trim().is_empty() {
        return Err("no file name — type one in the box above".into());
    }
    let text = serde_json::to_string_pretty(cues).map_err(|error| error.to_string())?;
    std::fs::write(path, text).map_err(|error| error.to_string())
}
