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
/// The tick box at the head of a cue row.
const TICK: f32 = 22.0;
/// The number that goes out with the address.
const VALUE: f32 = 66.0;
/// The remove button at the end of a cue row.
const REMOVE: f32 = 52.0;
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
const SOURCE_URL: &str = "https://github.com/mr-bolster/chasefire";

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
        language: &mut crate::text::Language,
    ) {
        let words = crate::text::Text::of(*language);
        if !self.open {
            return;
        }

        let viewport = egui::ViewportId::from_hash_of("chasefire-options");
        let builder = egui::ViewportBuilder::default()
            .with_title(words.options_title)
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
                        self.contents(ui, runner, presentation, language, width);
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
        language: &mut crate::text::Language,
        width: f32,
    ) {
        let words = crate::text::Text::of(*language);
        self.input_section(ui, runner, words);
        self.outputs_section(ui, runner, words);
        self.cues_section(ui, runner, words, width);
        self.timing_section(ui, runner, words);
        appearance_section(ui, presentation, language, words);
        support_section(ui, words);

        if let Some(message) = &self.message {
            ui.add_space(GAP);
            ui.separator();
            ui.add_space(4.0);
            ui.label(egui::RichText::new(message).size(12.0));
        }
    }

    fn input_section(
        &mut self,
        ui: &mut egui::Ui,
        runner: &mut Runner,
        words: &'static crate::text::Text,
    ) {
        section(ui, words.section_input);
        grid(ui, "input", |ui| {
            label(ui, words.device);
            let current = self
                .chosen_device
                .clone()
                .unwrap_or_else(|| words.system_default.into());
            egui::ComboBox::from_id_salt("device")
                .selected_text(shorten(&current, 44))
                .width(FIELD)
                .show_ui(ui, |ui| {
                    ui.selectable_value(&mut self.chosen_device, None, words.system_default);
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

            label(ui, words.channel);
            ui.horizontal(|ui| {
                ui.add(egui::DragValue::new(&mut self.channel).range(1..=64));
                hint(ui, words.channel_hint);
            });
            ui.end_row();

            label(ui, words.frame_rate);
            ui.horizontal(|ui| {
                let current = match self.pinned_fps {
                    None => words.work_it_out.to_string(),
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
                        ui.selectable_value(&mut self.pinned_fps, None, words.work_it_out);
                        for (name, rate) in RATES {
                            ui.selectable_value(&mut self.pinned_fps, Some(rate), name);
                        }
                    });
                // Not a detail: it is two frames of lock time, and on a stage
                // that is the difference between catching the first cue and
                // missing it.
                hint(ui, words.frame_rate_hint);
            });
            ui.end_row();

            label(ui, "");
            ui.horizontal(|ui| {
                if ui.button(words.apply_and_listen).clicked() {
                    runner.pin_frame_rate(self.pinned_fps);
                    match runner.open_input(self.chosen_device.as_deref(), self.channel) {
                        Ok(()) => self.message = Some(words.listening.into()),
                        Err(error) => self.message = Some(words.audio_error(&error)),
                    }
                }
                if runner.is_listening() {
                    if ui.button(words.stop).clicked() {
                        runner.close_input();
                        self.message = Some(words.input_closed.into());
                    }
                    hint(ui, words.restarts_input);
                }
            });
            ui.end_row();
        });
    }

    fn outputs_section(
        &mut self,
        ui: &mut egui::Ui,
        runner: &mut Runner,
        words: &'static crate::text::Text,
    ) {
        section(ui, words.section_outputs);
        grid(ui, "outputs", |ui| {
            label(ui, "OSC");
            ui.horizontal(|ui| {
                ui.add(
                    egui::TextEdit::singleline(&mut self.osc_target).desired_width(FIELD - 80.0),
                );
                if ui.button(words.connect).clicked() {
                    match runner.connect_osc(&self.osc_target) {
                        Ok(()) => {
                            self.message = Some(words.sending_to.replace("{}", &self.osc_target))
                        }
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
                    let mut empty = String::from(words.no_port);
                    ui.add(egui::TextEdit::singleline(&mut empty).desired_width(FIELD - 80.0));
                });
                hint(ui, words.not_built_yet);
            });
            ui.end_row();

            label(ui, "MIDI over network");
            ui.horizontal(|ui| {
                ui.add_enabled_ui(false, |ui| {
                    let mut empty = String::from(words.no_session);
                    ui.add(egui::TextEdit::singleline(&mut empty).desired_width(FIELD - 80.0));
                });
                hint(ui, words.not_built_yet);
            });
            ui.end_row();
        });
        ui.add_space(2.0);
        hint(ui, words.rtp_note);
    }

    fn cues_section(
        &mut self,
        ui: &mut egui::Ui,
        runner: &mut Runner,
        words: &'static crate::text::Text,
        width: f32,
    ) {
        section(ui, words.section_cues);
        grid(ui, "cuefile", |ui| {
            label(ui, words.file);
            ui.horizontal(|ui| {
                ui.add(egui::TextEdit::singleline(&mut self.cue_path).desired_width(FIELD));
                if ui.button(words.load).clicked() {
                    match runner.load_cues(std::path::Path::new(&self.cue_path)) {
                        Ok(count) => {
                            self.message = Some(words.cues_loaded.replace("{}", &count.to_string()))
                        }
                        Err(error) => self.message = Some(error),
                    }
                }
            });
            ui.end_row();
        });

        ui.add_space(4.0);
        self.cue_table(ui, runner, words, width);
    }

    /// The cue list, editable in place.
    ///
    /// Editing writes straight through to the engine, which re-arms everything
    /// and forgets where it was. That is correct, and it means editing during a
    /// show is a real interruption rather than a quiet one — hence the warning.
    fn cue_table(
        &mut self,
        ui: &mut egui::Ui,
        runner: &mut Runner,
        words: &'static crate::text::Text,
        width: f32,
    ) {
        let mut cues = runner.cues().to_vec();
        let mut changed = false;
        let mut remove = None;

        // Every text field grows with the window, each with a floor below which
        // it stops being usable. The timecode needs the least — it is always
        // eleven characters — but cramming it into exactly eleven characters
        // makes it fiddly to click into, so it gets a share too.
        // What the row spends on things that never change size: the tick, the
        // value, the remove button, and the five gaps between the six of them.
        let fixed = TICK + VALUE + REMOVE + GAP * 5.0;
        let flexible = (width - fixed - 20.0).max(280.0);
        let at_width = (flexible * 0.14).max(96.0);
        let name_width = (flexible * 0.30).max(110.0);
        let address_width = (flexible - at_width - name_width).max(150.0);

        if cues.is_empty() {
            hint(ui, words.no_cues_yet);
        } else {
            egui::ScrollArea::vertical()
                .max_height(CUE_ROW * CUES_VISIBLE)
                .id_salt("cuelist")
                .show(ui, |ui| {
                    // Laid out by hand rather than with a Grid, and not for
                    // taste. A Grid gives a non-final column only as much room
                    // as that column had last frame, and a TextEdit will not
                    // take more room than it is offered — so the two feed each
                    // other and the fields freeze at whatever width they had on
                    // the first frame. Widening the window did nothing at all.
                    // Rows of our own break the loop: each one is told its
                    // widths outright.
                    row(ui, |ui| {
                        heading(ui, words.column_on, TICK);
                        heading(ui, words.column_at, at_width);
                        heading(ui, words.column_name, name_width);
                        heading(ui, words.column_sends, address_width);
                        heading(ui, words.column_value, VALUE).on_hover_text(words.value_tooltip);
                    });

                    for (index, cue) in cues.iter_mut().enumerate() {
                        // Alternating bands, painted behind the row. Reserved
                        // before the row is drawn and filled in after, once its
                        // real height is known — twenty near-identical lines of
                        // timecode are hard to follow otherwise.
                        let band = ui.painter().add(egui::Shape::Noop);
                        let response = row(ui, |ui| {
                            changed |= ui
                                .add_sized(
                                    [TICK, CUE_ROW],
                                    egui::Checkbox::without_text(&mut cue.enabled),
                                )
                                .changed();

                            // Typed the way the trade writes it, and only
                            // accepted once it is actually a timecode: a
                            // half-typed one must not move a cue to midnight.
                            let mut text = cue.at.to_string();
                            let edited = ui
                                .add(
                                    egui::TextEdit::singleline(&mut text)
                                        .desired_width(at_width)
                                        .font(egui::TextStyle::Monospace),
                                )
                                .on_hover_text(words.at_tooltip)
                                .changed();
                            if edited {
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

                            changed |= action_editor(ui, &mut cue.action, words, address_width);

                            if ui.small_button(words.remove).clicked() {
                                remove = Some(index);
                            }
                        });

                        if index % 2 == 1 {
                            let stripe = ui.visuals().faint_bg_color;
                            ui.painter().set(
                                band,
                                egui::Shape::rect_filled(response.response.rect, 2.0, stripe),
                            );
                        }
                    }
                });
        }

        ui.add_space(4.0);
        ui.horizontal(|ui| {
            if ui.button(words.add_cue).clicked() {
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
            if ui.button(words.save_to_file).clicked() {
                match save_cues(&self.cue_path, &cues, words) {
                    Ok(()) => self.message = Some(words.written_to.replace("{}", &self.cue_path)),
                    Err(error) => self.message = Some(error),
                }
            }
            if runner.is_armed() {
                hint(ui, words.editing_rearms);
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

    fn timing_section(
        &mut self,
        ui: &mut egui::Ui,
        runner: &mut Runner,
        words: &'static crate::text::Text,
    ) {
        section(ui, words.section_timing);
        grid(ui, "timing", |ui| {
            label(ui, words.offset);
            ui.horizontal(|ui| {
                let mut offset = runner.offset_frames();
                if ui
                    .add(
                        egui::DragValue::new(&mut offset)
                            .range(-50..=50)
                            .suffix(words.frames_suffix),
                    )
                    .changed()
                {
                    runner.set_offset_frames(offset);
                }
                hint(ui, words.offset_hint);
            });
            ui.end_row();

            label(ui, words.freewheel);
            ui.horizontal(|ui| {
                let mut freewheel = runner.freewheel_frames();
                if ui
                    .add(
                        egui::DragValue::new(&mut freewheel)
                            .range(0..=120)
                            .suffix(words.frames_suffix),
                    )
                    .changed()
                {
                    runner.set_freewheel_frames(freewheel);
                }
                hint(ui, words.freewheel_hint);
            });
            ui.end_row();
        });
    }
}

fn appearance_section(
    ui: &mut egui::Ui,
    presentation: &mut Presentation,
    language: &mut crate::text::Language,
    words: &'static crate::text::Text,
) {
    section(ui, words.section_appearance);
    grid(ui, "appearance", |ui| {
        label(ui, words.status_display);
        ui.horizontal(|ui| {
            ui.selectable_value(presentation, Presentation::Pablo, "Pablo");
            ui.selectable_value(presentation, Presentation::Plain, words.transport_marks);
        });
        ui.end_row();
    });
    grid(ui, "language", |ui| {
        label(ui, words.language_label);
        ui.horizontal(|ui| {
            for entry in crate::text::Text::all() {
                ui.selectable_value(language, entry.language, entry.language.name());
            }
        });
        ui.end_row();
    });
    ui.add_space(2.0);
    hint(ui, words.appearance_hint);
}

/// The ask, and who did what.
///
/// Kept honest and kept short. The software is free and stays free; what is
/// being asked for is a contribution from people who use it to earn a living,
/// and there is no nagging, no timer and nothing withheld if nobody pays.
fn support_section(ui: &mut egui::Ui, words: &'static crate::text::Text) {
    section(ui, words.section_support);
    ui.label(egui::RichText::new(words.support_free).size(12.0));
    ui.add_space(4.0);
    ui.label(egui::RichText::new(words.support_ask).size(12.0));
    ui.add_space(8.0);
    ui.horizontal(|ui| {
        // Deliberately a button and not a link buried in a sentence. Somebody
        // who has decided to pay should not have to hunt for where.
        let donate = egui::Button::new(
            egui::RichText::new(words.donate)
                .size(14.0)
                .strong()
                .color(egui::Color32::WHITE),
        )
        .fill(egui::Color32::from_rgb(0, 112, 186))
        .min_size(egui::vec2(190.0, 32.0));

        if ui.add(donate).on_hover_text(DONATE_URL).clicked() {
            ui.ctx().open_url(egui::OpenUrl::new_tab(DONATE_URL));
        }
    });

    ui.add_space(GAP);
    section(ui, words.section_about);

    grid(ui, "about", |ui| {
        label(ui, words.version);
        ui.horizontal(|ui| {
            // Straight from the manifest, so it can never disagree with what
            // was actually built. Somebody reporting a fault will be reading
            // this line out loud, and it has to be the truth.
            ui.label(
                egui::RichText::new(env!("CARGO_PKG_VERSION"))
                    .monospace()
                    .strong(),
            );
            hint(ui, words.version_hint);
        });
        ui.end_row();

        label(ui, words.built_for);
        ui.label(words.built_for_value);
        ui.end_row();

        label(ui, words.written_by);
        ui.label("Leo Bolster");
        ui.end_row();

        label(ui, words.art_by);
        ui.label(words.art_by_value);
        ui.end_row();

        label(ui, words.licence);
        ui.horizontal(|ui| {
            ui.label("GPL-3.0-or-later");
            hint(ui, words.licence_hint);
        });
        ui.end_row();

        label(ui, words.source);
        ui.hyperlink_to(SOURCE_URL.trim_start_matches("https://"), SOURCE_URL);
        ui.end_row();

        label(ui, words.settings_live_in);
        ui.label(
            egui::RichText::new(crate::settings::Settings::location())
                .size(11.0)
                .monospace(),
        )
        .on_hover_text(words.portable_hint);
        ui.end_row();

        label(ui, words.standing_on);
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

/// One line of the cue list. Nothing between the widgets but the gaps we put
/// there ourselves: egui adds its own spacing on top of anything we allocate,
/// which is how rows end up a few pixels wider than the window every time.
fn row<R>(ui: &mut egui::Ui, contents: impl FnOnce(&mut egui::Ui) -> R) -> egui::InnerResponse<R> {
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = GAP;
        contents(ui)
    })
}

/// A column heading, sitting over the field it names.
fn heading(ui: &mut egui::Ui, text: &str, width: f32) -> egui::Response {
    ui.add_sized(
        [width, 14.0],
        egui::Label::new(egui::RichText::new(text).size(11.0).weak()).halign(egui::Align::LEFT),
    )
}

/// Edit whatever a cue does. Only OSC exists so far, so this is one row; when
/// MIDI arrives it becomes a choice of kind and then its own fields.
fn action_editor(
    ui: &mut egui::Ui,
    action: &mut cue::Action,
    words: &'static crate::text::Text,
    width: f32,
) -> bool {
    match action {
        cue::Action::Osc { address, args } => {
            // Drawn straight into the row it was called from, not in a
            // horizontal of its own: a nested one brings its own spacing and
            // the value stops lining up with the heading above it.
            let mut changed = ui
                .add(egui::TextEdit::singleline(address).desired_width(width))
                .changed();
            if let Some(cue::OscArg::Int(value)) = args.first_mut() {
                changed |= ui
                    .add_sized([VALUE, CUE_ROW - 5.0], egui::DragValue::new(value))
                    .on_hover_text(words.value_tooltip)
                    .changed();
            }
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

fn save_cues(
    path: &str,
    cues: &[cue::Cue],
    words: &'static crate::text::Text,
) -> Result<(), String> {
    if path.trim().is_empty() {
        return Err(words.no_file_name.into());
    }
    let text = serde_json::to_string_pretty(cues).map_err(|error| error.to_string())?;
    std::fs::write(path, text).map_err(|error| error.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Measure a string the way egui will actually draw it, using egui's own
    /// font stack. Character counts are not good enough here: "Frames por
    /// segundo" and "Status display" are the same length and different widths.
    fn width_on_screen(text: &str, size: f32) -> f32 {
        let context = egui::Context::default();
        // One frame so the fonts exist. Nothing is drawn.
        let _ = context.run_ui(egui::RawInput::default(), |_| {});
        context
            .fonts_mut(|fonts| {
                fonts.layout_no_wrap(
                    text.to_string(),
                    egui::FontId::proportional(size),
                    egui::Color32::WHITE,
                )
            })
            .rect
            .width()
    }

    /// Lay out one cue row in a window `window_wide` wide and give back the
    /// width the address field actually got. No window is opened: egui will
    /// lay out against whatever screen rect it is handed.
    fn address_field_width(window_wide: f32) -> f32 {
        let context = egui::Context::default();
        let input = egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(window_wide, 860.0),
            )),
            ..Default::default()
        };

        let mut measured = 0.0;
        // Twice, because a layout that only settles on the second frame is
        // still a layout that works — and the bug this guards against was one
        // that never settled at all.
        for _ in 0..2 {
            let _ = context.run_ui(input.clone(), |context| {
                egui::CentralPanel::default()
                    .frame(
                        egui::Frame::central_panel(context.style().as_ref()).inner_margin(MARGIN),
                    )
                    .show(context, |ui| {
                        let width = ui.max_rect().width();
                        let fixed = TICK + VALUE + REMOVE + GAP * 5.0;
                        let flexible = (width - fixed - 20.0).max(280.0);
                        let at_width = (flexible * 0.14).max(96.0);
                        let name_width = (flexible * 0.30).max(110.0);
                        let address_width = (flexible - at_width - name_width).max(150.0);

                        let mut address = String::from("/composition/columns/1/connect");
                        let mut name = String::from("cue 1");
                        let mut at = String::from("10:00:00:00");
                        let mut on = true;
                        row(ui, |ui| {
                            ui.add_sized([TICK, CUE_ROW], egui::Checkbox::without_text(&mut on));
                            ui.add(egui::TextEdit::singleline(&mut at).desired_width(at_width));
                            ui.add(egui::TextEdit::singleline(&mut name).desired_width(name_width));
                            measured = ui
                                .add(
                                    egui::TextEdit::singleline(&mut address)
                                        .desired_width(address_width),
                                )
                                .rect
                                .width();
                            let _ = ui.small_button("x");
                        });
                    });
            });
        }
        measured
    }

    #[test]
    fn the_cue_fields_grow_with_the_window() {
        // The one that was wrong, and wrong in a way that looked like arithmetic
        // and was not. The widths were always worked out correctly; the fields
        // never got them, because a Grid offers a non-final column only as much
        // room as it had last frame and a TextEdit will not take more room than
        // it is offered. The two agreed with each other for ever and dragging
        // the window did nothing.
        let narrow = address_field_width(720.0);
        let wide = address_field_width(1280.0);

        assert!(narrow > 150.0, "the address field started off unusable");
        assert!(
            wide > narrow + 200.0,
            "widening the window by 560px gave the address field {:.0}px more",
            wide - narrow
        );
    }

    #[test]
    fn every_label_fits_its_column_in_every_language() {
        // The labels sit in a fixed-width column so the fields line up. A
        // translation that is wider than the column does not push anything
        // along — it gets cut off, and somebody reads half a word in a dark
        // room. Spanish runs about a fifth longer than English, so this is the
        // check that has to exist before a third language is ever added.
        for words in crate::text::Text::all() {
            for label in [
                words.device,
                words.channel,
                words.frame_rate,
                words.file,
                words.offset,
                words.freewheel,
                words.status_display,
                words.language_label,
                words.version,
                words.built_for,
                words.written_by,
                words.art_by,
                words.licence,
                words.source,
                words.settings_live_in,
                words.standing_on,
            ] {
                let width = width_on_screen(label, 12.0);
                assert!(
                    width <= LABEL,
                    "'{label}' needs {width:.0}px and the column is {LABEL:.0}px"
                );
            }
        }
    }
}
