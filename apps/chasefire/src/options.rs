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
/// The button that adds another message to a cue.
const ADD_STEP: f32 = 24.0;
/// The little button showing an argument's OSC type tag.
const ARG_TYPE: f32 = 46.0;
/// The editor for an argument's value.
const ARG_VALUE: f32 = 58.0;
/// One of the little add/remove buttons on an argument.
const ARG_BUTTON: f32 = 20.0;
/// The destination picker, when there is more than one place to send to.
const DEST: f32 = 76.0;
/// What the window spends before the cue table gets a look in: the panel's own
/// margin on both sides, and the indent the section headings put on their
/// contents.
const CHROME: f32 = 100.0;
/// How tall the settings window opens. Chosen to sit comfortably on the
/// shortest screen anybody is likely to run a show from.
const WINDOW_TALL: f32 = 1000.0;
/// The picker that says what sort of message a line carries.
const KIND: f32 = 74.0;
/// The smallest each elastic column may become. Below these a field stops
/// being clickable, which is worse than a list that has to be scrolled.
const AT_FLOOR: f32 = 96.0;
const NAME_FLOOR: f32 = 100.0;
const ADDRESS_FLOOR: f32 = 140.0;
/// The little button that takes one message off a cue.
const DROP_STEP: f32 = 24.0;
/// The tick that picks a cue out for duplicating or deleting.
const PICK: f32 = 22.0;
/// One row of the cue list, near enough, for working out how many fit.
const CUE_ROW: f32 = 23.0;
/// The colour a button turns when the next click destroys something.
const WARNING: egui::Color32 = egui::Color32::from_rgb(150, 62, 40);
/// A cue that will fire, and one that will not. Read from an angle, in a hurry,
/// by somebody who is not going to lean in to count tick marks.
const LIVE: egui::Color32 = egui::Color32::from_rgb(38, 122, 62);
const MUTED: egui::Color32 = egui::Color32::from_rgb(132, 46, 42);
/// How many lines of the cue list are visible without scrolling when there is
/// room. Lines, not cues: a cue that sends two messages takes two of them.
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
    /// What the next output added will be called. Cues address outputs by name.
    osc_name: String,
    /// What a new RTP-MIDI session will be called, listen on, and invite.
    rtp_name: String,
    rtp_port: u16,
    rtp_peer: String,
    /// The MIDI ports this machine has, listed once because asking is slow.
    midi_ports: Vec<String>,
    ports_listed: bool,
    midi_port: Option<String>,
    midi_name: String,
    /// Which transport's settings are on show.
    output_tab: Carrier,
    /// Which cues the list is showing.
    filter: Filter,
    /// Which cues are ticked, by id rather than by position: the list gets
    /// edited and reordered underneath, and a selection that follows row
    /// numbers ends up pointing at the wrong cues.
    selected: std::collections::HashSet<u32>,
    cue_path: String,
    /// What the next click on that button would destroy, if anything. Set on
    /// the first click and cleared by the second, so a save that overwrites and
    /// a new list that discards are both two deliberate clicks and never one.
    about_to: Option<Danger>,
    message: Option<String>,
}

/// Which sort of output a panel or a filter is about. Not `cue::Carrier`,
/// because MIDI down a cable and MIDI over a network are the same messages and
/// very different things to set up — and to an operator hunting for the reason
/// nothing is moving, they are two different cables to check.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Carrier {
    Osc,
    Midi,
    Network,
}

impl Carrier {
    const ALL: [Carrier; 3] = [Carrier::Osc, Carrier::Midi, Carrier::Network];

    fn label(self, words: &'static crate::text::Text) -> &'static str {
        match self {
            Carrier::Osc => words.tab_osc,
            Carrier::Midi => words.tab_midi,
            Carrier::Network => words.tab_network,
        }
    }
}

/// What the cue list is showing.
///
/// **One list, filtered** — not one list per transport. A cue at 10:00:00:00
/// that starts the video and changes a snapshot on the desk is *one moment in
/// the show*; kept in two tabs it would be the same timecode written down twice,
/// and the day one of them moved they would quietly disagree. The tabs choose
/// what to look at. They do not choose what a cue is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Filter {
    Everything,
    Only(Carrier),
}

impl Filter {
    fn shows(self, cue: &cue::Cue) -> bool {
        match self {
            Filter::Everything => true,
            Filter::Only(Carrier::Osc) => {
                cue.carriers().any(|carrier| carrier == cue::Carrier::Osc)
            }
            // Until MIDI is built there is nothing to tell the two apart by, so
            // both MIDI tabs show every MIDI cue rather than pretending to know.
            Filter::Only(_) => cue.carriers().any(|carrier| carrier == cue::Carrier::Midi),
        }
    }
}

/// Something a click is about to throw away.
#[derive(Debug, Clone, PartialEq)]
enum Danger {
    /// Save over a file that is already there.
    Overwrite(String),
    /// Start an empty list, losing the cues currently loaded.
    Discard(usize),
    /// Throw away the cues that are ticked.
    DeleteCues(usize),
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
            osc_name: String::new(),
            rtp_name: String::new(),
            // The port everything in this trade defaults to.
            rtp_port: 5004,
            rtp_peer: String::new(),
            midi_ports: Vec::new(),
            ports_listed: false,
            midi_port: None,
            midi_name: String::new(),
            output_tab: Carrier::Osc,
            filter: Filter::Everything,
            selected: std::collections::HashSet::new(),
            cue_path: cues.unwrap_or_else(|| {
                crate::cuefile::untitled(&crate::cuefile::default_directory(), "cues")
                    .to_string_lossy()
                    .into_owned()
            }),
            about_to: None,
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
            // Wide enough that a cue with a long OSC address and a couple of
            // arguments reads without dragging anything, and as tall as a
            // 1080-line screen will take without the bottom going under a
            // taskbar. The cue list is at the end and scrolls into view; it
            // shows its full twenty-five lines when it gets there.
            .with_inner_size([Widths::narrowest(true) + CHROME + 120.0, WINDOW_TALL])
            // Narrow enough to be dragged out of the way, wide enough that the
            // cue table still draws every field at a size somebody can click.
            // Below this the list scrolls sideways rather than shrinking things
            // past being usable.
            .with_min_inner_size([Widths::narrowest(true) + CHROME, 420.0]);

        let mut still_open = true;
        context.show_viewport_immediate(viewport, builder, |context, _class| {
            egui::CentralPanel::default()
                .frame(egui::Frame::central_panel(context.style().as_ref()).inner_margin(MARGIN))
                .show(context, |ui| {
                    // Never shrink to fit: a scroll area that sizes itself
                    // to its content, whose content sizes itself to the scroll
                    // area, settles wherever it happened to start. The table
                    // came out two thirds of the window wide and stayed there.
                    egui::ScrollArea::vertical()
                        .auto_shrink([false, false])
                        .show(ui, |ui| {
                            self.contents(ui, runner, presentation, language);
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
    ) {
        let words = crate::text::Text::of(*language);
        self.input_section(ui, runner, words);
        self.outputs_section(ui, runner, words);
        self.cues_section(ui, runner, words);
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

        // Tabs here, where they belong: MIDI down a cable and MIDI over a
        // network are the same messages and completely different things to set
        // up. What is *not* tabbed is the cue list — see `Filter`.
        ui.horizontal(|ui| {
            for carrier in Carrier::ALL {
                ui.selectable_value(&mut self.output_tab, carrier, carrier.label(words));
            }
        });
        ui.add_space(6.0);

        match self.output_tab {
            Carrier::Osc => self.osc_outputs(ui, runner, words),
            // Listed rather than hidden. Somebody deciding whether this tool
            // fits their rig needs to know what it will and will not do, and
            // finding that out by not finding a setting is a poor way to learn.
            Carrier::Midi => self.midi_outputs(ui, runner, words),
            Carrier::Network => self.network_outputs(ui, runner, words),
        }
    }

    /// The local MIDI ports.
    ///
    /// This is the half of the trade that does not speak OSC at all: a
    /// grandMA2 takes MIDI Show Control or nothing, and SuperRack recalls
    /// snapshots on Program Change.
    fn midi_outputs(
        &mut self,
        ui: &mut egui::Ui,
        runner: &mut Runner,
        words: &'static crate::text::Text,
    ) {
        if !self.ports_listed {
            self.ports_listed = true;
            self.midi_ports = Runner::midi_ports();
        }

        grid(ui, "midi-outputs", |ui| {
            label(ui, words.add_output);
            ui.horizontal(|ui| {
                ui.add(
                    egui::TextEdit::singleline(&mut self.midi_name)
                        .desired_width(80.0)
                        .hint_text(words.output_name),
                );
                let chosen = self
                    .midi_port
                    .clone()
                    .unwrap_or_else(|| words.no_midi_ports.to_string());
                egui::ComboBox::from_id_salt("midi-port")
                    .selected_text(shorten(&chosen, 30))
                    .width(FIELD - 80.0)
                    .show_ui(ui, |ui| {
                        for port in &self.midi_ports {
                            ui.selectable_value(&mut self.midi_port, Some(port.clone()), port);
                        }
                    });
                if ui.button(words.rescan).clicked() {
                    self.midi_ports = Runner::midi_ports();
                    self.message = Some(
                        words
                            .ports_found
                            .replace("{}", &self.midi_ports.len().to_string()),
                    );
                }
                let ready = self.midi_port.is_some();
                if ui
                    .add_enabled(ready, egui::Button::new(words.connect))
                    .clicked()
                {
                    let name = if self.midi_name.trim().is_empty() {
                        "midi".to_string()
                    } else {
                        self.midi_name.trim().to_string()
                    };
                    let port = self.midi_port.clone().unwrap_or_default();
                    match runner.connect_midi_as(&name, &port) {
                        Ok(()) => {
                            self.message = Some(words.sending_to.replace("{}", &port));
                            self.midi_name.clear();
                        }
                        Err(error) => self.message = Some(error),
                    }
                }
            });
            ui.end_row();
        });
        ui.add_space(6.0);

        // The program's other job. A rig with LTC on a cable and a machine that
        // only speaks MTC has a hole in it; this fills it, and somebody who
        // needs only that never has to programme a cue at all.
        grid(ui, "mtc", |ui| {
            label(ui, words.mtc);
            ui.horizontal(|ui| {
                let running = runner.mtc_port().map(|port| port.to_string());
                match &running {
                    Some(port) => {
                        ui.label(egui::RichText::new(shorten(port, 30)).size(11.0));
                        if ui.button(words.stop).clicked() {
                            runner.stop_mtc();
                            self.message = Some(words.mtc_stopped.into());
                        }
                    }
                    None => {
                        let ready = self.midi_port.is_some();
                        if ui
                            .add_enabled(ready, egui::Button::new(words.mtc_send))
                            .clicked()
                        {
                            let port = self.midi_port.clone().unwrap_or_default();
                            match runner.start_mtc(&port) {
                                Ok(()) => {
                                    self.message = Some(words.mtc_sending.replace("{}", &port))
                                }
                                Err(error) => self.message = Some(error),
                            }
                        }
                    }
                }
            });
            ui.end_row();
        });
        ui.add_space(2.0);
        hint(ui, words.mtc_note);
        ui.add_space(2.0);
        if self.midi_ports.is_empty() {
            hint(ui, words.no_midi_ports_hint);
        } else {
            hint(ui, words.midi_note);
        }
    }

    /// RTP-MIDI sessions.
    ///
    /// Nothing to install and no virtual cable: a Mac has this in Audio MIDI
    /// Setup, an iPad has it, Companion speaks it, and on Windows it is what
    /// rtpMIDI provides. Which end invites is not ours to choose, so both are
    /// offered — leave the address empty and it waits to be invited.
    fn network_outputs(
        &mut self,
        ui: &mut egui::Ui,
        runner: &mut Runner,
        words: &'static crate::text::Text,
    ) {
        grid(ui, "rtp-outputs", |ui| {
            label(ui, words.add_output);
            ui.horizontal(|ui| {
                ui.add(
                    egui::TextEdit::singleline(&mut self.rtp_name)
                        .desired_width(80.0)
                        .hint_text(words.output_name),
                );
                ui.add(
                    egui::DragValue::new(&mut self.rtp_port)
                        .range(1024..=65534)
                        .prefix(words.rtp_port_prefix),
                )
                .on_hover_text(words.rtp_port_tooltip);
                ui.add(
                    egui::TextEdit::singleline(&mut self.rtp_peer)
                        .desired_width(FIELD - 90.0)
                        .hint_text(words.rtp_peer_hint),
                );
                if ui.button(words.connect).clicked() {
                    let name = if self.rtp_name.trim().is_empty() {
                        "red".to_string()
                    } else {
                        self.rtp_name.trim().to_string()
                    };
                    match runner.connect_network_midi_as(
                        &name,
                        self.rtp_port,
                        Some(self.rtp_peer.as_str()),
                    ) {
                        Ok(()) => {
                            self.message = Some(if self.rtp_peer.trim().is_empty() {
                                words.rtp_waiting.replace("{}", &self.rtp_port.to_string())
                            } else {
                                words.sending_to.replace("{}", self.rtp_peer.trim())
                            });
                            self.rtp_name.clear();
                        }
                        Err(error) => self.message = Some(error),
                    }
                }
            });
            ui.end_row();
        });
        ui.add_space(2.0);
        hint(ui, words.rtp_note);
        ui.add_space(2.0);
        hint(ui, words.rtp_no_journal);
    }

    /// The OSC destinations, by name.
    ///
    /// A show is not one machine. The video server, the desk and the lighting
    /// console are three addresses, and a cue names which one it means — so
    /// this is a list rather than a box.
    fn osc_outputs(
        &mut self,
        ui: &mut egui::Ui,
        runner: &mut Runner,
        words: &'static crate::text::Text,
    ) {
        grid(ui, "osc-outputs", |ui| {
            label(ui, words.add_output);
            ui.horizontal(|ui| {
                ui.add(
                    egui::TextEdit::singleline(&mut self.osc_name)
                        .desired_width(80.0)
                        .hint_text(words.output_name),
                );
                ui.add(
                    egui::TextEdit::singleline(&mut self.osc_target).desired_width(FIELD - 80.0),
                );
                if ui.button(words.connect).clicked() {
                    let name = if self.osc_name.trim().is_empty() {
                        "osc".to_string()
                    } else {
                        self.osc_name.trim().to_string()
                    };
                    match runner.connect_osc_as(&name, &self.osc_target) {
                        Ok(()) => {
                            self.message = Some(words.sending_to.replace("{}", &self.osc_target));
                            self.osc_name.clear();
                        }
                        Err(error) => self.message = Some(error),
                    }
                }
            });
            ui.end_row();

            for name in runner.output_names() {
                label(ui, "");
                ui.horizontal(|ui| {
                    ui.label(
                        egui::RichText::new(format!(
                            "{name} — {}",
                            runner.output_described(&name).unwrap_or_default()
                        ))
                        .size(11.0),
                    );
                    if ui.small_button(words.remove).clicked() {
                        runner.disconnect_output(&name);
                    }
                });
                ui.end_row();
            }
        });
        ui.add_space(2.0);
        if runner.output_names().is_empty() {
            hint(ui, words.no_outputs);
        } else {
            hint(ui, words.outputs_hint);
        }
    }

    fn cues_section(
        &mut self,
        ui: &mut egui::Ui,
        runner: &mut Runner,
        words: &'static crate::text::Text,
    ) {
        section(ui, words.section_cues);
        grid(ui, "cuefile", |ui| {
            label(ui, words.file);
            ui.horizontal(|ui| {
                // Typing a different name in here and pressing save is "save
                // as". There is no separate button for it because there is no
                // separate thing happening.
                if ui
                    .add(egui::TextEdit::singleline(&mut self.cue_path).desired_width(FIELD))
                    .changed()
                {
                    // A path that has been edited is no longer the path that
                    // was agreed to be overwritten.
                    self.about_to = None;
                }
                if ui.button(words.load).clicked() {
                    self.about_to = None;
                    match runner.load_cues(std::path::Path::new(&self.cue_path)) {
                        Ok(count) => {
                            self.message = Some(words.cues_loaded.replace("{}", &count.to_string()))
                        }
                        Err(error) => self.message = Some(error),
                    }
                }

                let loaded = runner.cues().len();
                let asking = self.about_to == Some(Danger::Discard(loaded));
                let label = if asking {
                    words.discard_and_start.replace("{}", &loaded.to_string())
                } else {
                    words.new_list.to_string()
                };
                let mut button = egui::Button::new(&label);
                if asking {
                    button = button.fill(WARNING);
                }
                if ui.add(button).clicked() {
                    if loaded > 0 && !asking {
                        // Somebody has cues open. Say what would be lost, and
                        // make them click again.
                        self.about_to = Some(Danger::Discard(loaded));
                    } else {
                        self.about_to = None;
                        runner.set_cues(Vec::new());
                        self.cue_path =
                            crate::cuefile::untitled(&crate::cuefile::default_directory(), "cues")
                                .to_string_lossy()
                                .into_owned();
                        self.message = Some(words.new_list_ready.to_string());
                    }
                }
            });
            ui.end_row();
        });
        ui.add_space(2.0);
        hint(ui, words.save_as_hint);

        ui.add_space(6.0);
        // One list, filtered — never one list per transport. A cue that starts
        // the video and moves the desk is one moment in the show; split across
        // tabs it would be the same timecode written twice, and the day one of
        // them moved they would quietly disagree.
        ui.horizontal(|ui| {
            let all = runner.cues().len();
            ui.selectable_value(
                &mut self.filter,
                Filter::Everything,
                format!("{} {all}", words.filter_all),
            );
            for carrier in Carrier::ALL {
                let filter = Filter::Only(carrier);
                let count = runner.cues().iter().filter(|cue| filter.shows(cue)).count();
                ui.selectable_value(
                    &mut self.filter,
                    filter,
                    format!("{} {count}", carrier.label(words)),
                );
            }
        });
        ui.add_space(4.0);
        self.cue_table(ui, runner, words);
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
    ) {
        let mut cues = runner.cues().to_vec();
        let mut changed = false;
        let mut remove_step = None;
        // Measured **here**, where the rows are actually drawn, not up at the
        // panel: the section indents its contents, and handing the table the
        // panel's width made every row that much too wide for the window. The
        // last button of each line was sitting off the edge.
        let width = ui.max_rect().width();
        let outputs = runner.output_names();
        // The destination column only exists once there is a choice to make. A
        // show with one machine should never have to learn that outputs have
        // names at all.
        let show_destination = outputs.len() > 1;

        let filter = self.filter;
        let Widths {
            at: at_width,
            name: name_width,
            address: address_width,
            args: args_width,
        } = Widths::for_a_window(width, show_destination);

        if cues.is_empty() {
            hint(ui, words.no_cues_yet);
        } else {
            egui::ScrollArea::both()
                .max_height(CUE_ROW * CUES_VISIBLE)
                .min_scrolled_height(CUE_ROW * CUES_VISIBLE)
                // Always twenty lines tall, whether there are two cues or
                // two hundred. A box that grows as cues are added moves every
                // button under it while somebody is working, and a short list
                // gives nowhere to drop the next one. What is below stays
                // reachable: the window opens tall enough to show that there
                // is more under it.
                .auto_shrink([false, false])
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
                        heading(ui, words.column_pick, PICK);
                        heading(ui, words.column_on, TICK);
                        heading(ui, words.column_at, at_width);
                        heading(ui, words.column_name, name_width);
                        if show_destination {
                            heading(ui, words.column_to, DEST);
                        }
                        heading(ui, words.column_sends, address_width);
                        heading(ui, words.column_args, args_width)
                            .on_hover_text(words.args_tooltip);
                    });

                    let mut lines = 0usize;
                    for (index, cue) in cues.iter_mut().enumerate() {
                        // The row number is the real one whatever is on show:
                        // duplicating and deleting work on the list, not on
                        // what happens to be visible.
                        if !filter.shows(cue) {
                            continue;
                        }
                        // Alternating bands, painted behind the whole cue —
                        // every one of its messages, not just the first line.
                        // Reserved before the block is drawn and filled in
                        // after, once its real height is known.
                        let band = ui.painter().add(egui::Shape::Noop);
                        let block = ui.vertical(|ui| {
                            ui.spacing_mut().item_spacing.y = 2.0;
                            let steps = cue.steps.len();
                            for step_index in 0..steps {
                                let first = step_index == 0;
                                row(ui, |ui| {
                                    // The later messages of a cue line up under
                                    // the first with nothing repeated: they all
                                    // happen at one moment, and the moment is
                                    // written once. The columns are still
                                    // allocated, so everything stays in step.
                                    cell(ui, PICK, |ui| {
                                        if !first {
                                            return;
                                        }
                                        let mut picked = self.selected.contains(&cue.id);
                                        if ui
                                            .checkbox(&mut picked, "")
                                            .on_hover_text(words.pick_tooltip)
                                            .changed()
                                        {
                                            if picked {
                                                self.selected.insert(cue.id);
                                            } else {
                                                self.selected.remove(&cue.id);
                                            }
                                        }
                                    });
                                    gap(ui);

                                    cell(ui, TICK, |ui| {
                                        if !first {
                                            return;
                                        }
                                        // Green for armed, red for not. A cue
                                        // list gets read in a hurry and from an
                                        // angle, and whether a line will fire
                                        // is the one thing that has to be
                                        // legible without leaning in.
                                        let colour = if cue.enabled { LIVE } else { MUTED };
                                        let widgets = &mut ui.visuals_mut().widgets;
                                        for widget in [
                                            &mut widgets.inactive,
                                            &mut widgets.hovered,
                                            &mut widgets.active,
                                        ] {
                                            widget.bg_fill = colour;
                                            widget.weak_bg_fill = colour;
                                        }
                                        changed |= ui
                                            .checkbox(&mut cue.enabled, "")
                                            .on_hover_text(if cue.enabled {
                                                words.enabled_tooltip
                                            } else {
                                                words.disabled_tooltip
                                            })
                                            .changed();
                                    });
                                    gap(ui);

                                    cell(ui, at_width, |ui| {
                                        if !first {
                                            return;
                                        }
                                        // Typed the way the trade writes it,
                                        // and only accepted once it is actually
                                        // a timecode: a half-typed one must not
                                        // move a cue to midnight.
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
                                    });
                                    gap(ui);

                                    cell(ui, name_width, |ui| {
                                        if first {
                                            changed |= ui
                                                .add(
                                                    egui::TextEdit::singleline(&mut cue.name)
                                                        .desired_width(name_width),
                                                )
                                                .changed();
                                        }
                                    });
                                    gap(ui);

                                    changed |= step_editor(
                                        ui,
                                        &mut cue.steps[step_index],
                                        &outputs,
                                        show_destination,
                                        words,
                                        address_width,
                                        args_width,
                                    );

                                    // The two buttons at the end of every line,
                                    // in the same two columns whichever line it
                                    // is: one more message, and take one away.
                                    cell(ui, ADD_STEP, |ui| {
                                        if !first {
                                            return;
                                        }
                                        if ui
                                            .add_sized(ui.available_size(), egui::Button::new("+"))
                                            .on_hover_text(words.add_message)
                                            .clicked()
                                        {
                                            cue.steps.push(cue::Step::anywhere(
                                                cue::Message::Osc {
                                                    address: String::new(),
                                                    args: Vec::new(),
                                                },
                                            ));
                                            changed = true;
                                        }
                                    });
                                    gap(ui);

                                    // Always "take this message away", never
                                    // "take the cue away": a whole cue goes by
                                    // ticking it and using the button under the
                                    // list, so there are never two ways to do
                                    // the same thing sitting side by side.
                                    cell(ui, DROP_STEP, |ui| {
                                        if steps < 2 {
                                            return;
                                        }
                                        if ui
                                            .add_sized(ui.available_size(), egui::Button::new("−"))
                                            .on_hover_text(words.remove_message)
                                            .clicked()
                                        {
                                            remove_step = Some((index, step_index));
                                        }
                                    });
                                });
                            }
                        });

                        if index % 2 == 1 {
                            let stripe = ui.visuals().faint_bg_color;
                            ui.painter().set(
                                band,
                                egui::Shape::rect_filled(block.response.rect, 2.0, stripe),
                            );
                        }
                        lines += cue.steps.len();
                    }

                    // Rule the rest of the box. Empty space is a box that has
                    // run out; ruled lines are a list with room in it, and the
                    // difference matters when somebody is looking for where the
                    // next cue goes.
                    let width = ui.max_rect().width();
                    let stripe = ui.visuals().faint_bg_color;
                    let rule = ui.visuals().widgets.noninteractive.bg_stroke.color;
                    ui.add_space(1.0);
                    for spare in lines..(CUES_VISIBLE as usize) {
                        let (rect, _) = ui.allocate_exact_size(
                            egui::vec2(width, CUE_ROW - 4.0),
                            egui::Sense::hover(),
                        );
                        if spare % 2 == 1 {
                            ui.painter().rect_filled(rect, 2.0, stripe);
                        }
                        // A hairline along the bottom, so the rows read as rows
                        // rather than as bands of shading.
                        ui.painter().hline(
                            rect.x_range(),
                            rect.bottom(),
                            egui::Stroke::new(1.0, rule),
                        );
                    }
                });
        }

        // The legend is on the screen rather than only in a tooltip. A hover
        // that has to be discovered is no help to somebody who does not yet
        // know there is anything to discover.
        ui.add_space(3.0);
        hint(ui, words.arg_types);

        ui.add_space(4.0);
        ui.horizontal(|ui| {
            if ui.button(words.add_cue).clicked() {
                let next = cues.iter().map(|cue| cue.id).max().unwrap_or(0) + 1;
                cues.push(cue::Cue::new(
                    next,
                    format!("cue {next}"),
                    ltc::Timecode::new(10, 0, 0, 0),
                    cue::Message::Osc {
                        address: "/composition/columns/1/connect".into(),
                        args: vec![cue::OscArg::Int(1)],
                    },
                ));
                changed = true;
            }

            // A working cue for whatever machine this is pointed at. Not
            // magic and not hidden: it writes one ordinary cue that can then
            // be edited like any other. What it saves is the half hour of
            // reading a manual to find out that QLab wants no arguments.
            egui::ComboBox::from_id_salt("preset")
                .selected_text(words.from_a_preset)
                .width(150.0)
                .show_ui(ui, |ui| {
                    for preset in crate::presets::ALL {
                        let mut label = egui::RichText::new(preset.name);
                        if preset.port_unconfirmed {
                            label = label.italics();
                        }
                        if ui
                            .selectable_label(false, label)
                            .on_hover_text(if preset.port_unconfirmed {
                                format!("{} — {}", preset.note, words.port_unconfirmed)
                            } else {
                                preset.note.to_string()
                            })
                            .clicked()
                        {
                            let next = cues.iter().map(|cue| cue.id).max().unwrap_or(0) + 1;
                            cues.push(preset.cue(next));
                            if let Some(target) = preset.suggested_target() {
                                self.osc_target = target;
                                self.osc_name = preset.name.to_lowercase();
                            }
                            self.message = Some(words.preset_added.replace("{}", preset.name));
                            changed = true;
                        }
                    }
                });

            // What the ticks are for. Both buttons say how many they would
            // take, so nobody has to count ticks before pressing one.
            let picked: Vec<usize> = cues
                .iter()
                .enumerate()
                .filter(|(_, cue)| self.selected.contains(&cue.id))
                .map(|(index, _)| index)
                .collect();
            let any = !picked.is_empty();

            let duplicate = egui::Button::new(if any {
                words.duplicate.replace("{}", &picked.len().to_string())
            } else {
                words.duplicate_none.to_string()
            });
            if ui
                .add_enabled(any, duplicate)
                .on_hover_text(words.duplicate_tooltip)
                .clicked()
            {
                duplicate_picked(&mut cues, &picked, words.copy_suffix);
                changed = true;
            }

            let asking_delete = matches!(self.about_to, Some(Danger::DeleteCues(_)));
            let mut delete = egui::Button::new(if asking_delete {
                words.delete_sure.replace("{}", &picked.len().to_string())
            } else if any {
                words.delete_picked.replace("{}", &picked.len().to_string())
            } else {
                words.delete_none.to_string()
            });
            if asking_delete {
                delete = delete.fill(WARNING);
            }
            if ui.add_enabled(any, delete).clicked() {
                if asking_delete {
                    self.about_to = None;
                    cues.retain(|cue| !self.selected.contains(&cue.id));
                    self.selected.clear();
                    changed = true;
                } else {
                    self.about_to = Some(Danger::DeleteCues(picked.len()));
                }
            }

            let asking = self.about_to == Some(Danger::Overwrite(self.cue_path.clone()));
            let label = if asking {
                words
                    .overwrite
                    .replace("{}", &crate::cuefile::name_of(&self.cue_path))
            } else {
                words.save_to_file.to_string()
            };
            let mut button = egui::Button::new(&label);
            if asking {
                button = button.fill(WARNING);
            }
            if ui.add(button).clicked() {
                if crate::cuefile::exists(&self.cue_path) && !asking {
                    // Overwriting somebody else's show file is the one mistake
                    // here that cannot be undone, so it costs a second click.
                    self.about_to = Some(Danger::Overwrite(self.cue_path.clone()));
                } else {
                    self.about_to = None;
                    match crate::cuefile::save(&self.cue_path, &cues, words.no_file_name) {
                        Ok(()) => {
                            self.message = Some(
                                words
                                    .written_to
                                    .replace("{}", &crate::cuefile::name_of(&self.cue_path)),
                            )
                        }
                        Err(error) => self.message = Some(error),
                    }
                }
            }
            if runner.is_armed() {
                hint(ui, words.editing_rearms);
            }
        });

        if let Some((cue_index, step_index)) = remove_step {
            // The last message of a cue is not removable: a cue that sends
            // nothing is a line in the list that looks armed and does nothing
            // when its moment comes. Remove the cue instead.
            if cues[cue_index].steps.len() > 1 {
                cues[cue_index].steps.remove(step_index);
                changed = true;
            }
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

/// How wide each elastic column of the cue table is, for a given window.
///
/// Worked out in one place and by arithmetic alone, so it can be checked
/// without opening a window — and so that "the row is too wide" is a test
/// failure rather than something spotted in a screenshot.
pub struct Widths {
    pub at: f32,
    pub name: f32,
    pub address: f32,
    pub args: f32,
}

impl Widths {
    /// What the row spends on things that never change size: the tick, the two
    /// buttons at the end, the destination picker when there is one, and the
    /// gaps between all of it.
    fn fixed(show_destination: bool) -> f32 {
        let destination = if show_destination { DEST + GAP } else { 0.0 };
        PICK + TICK + KIND + ADD_STEP + DROP_STEP + destination + GAP * 7.0
    }

    /// The narrowest the table can be drawn with every field still usable.
    /// Below this it scrolls sideways rather than shrinking things past the
    /// point of being clickable — a timecode box too small to click into is
    /// worse than a row that has to be scrolled.
    pub fn narrowest(show_destination: bool) -> f32 {
        Self::fixed(show_destination) + Self::floors()
    }

    fn floors() -> f32 {
        AT_FLOOR + NAME_FLOOR + ADDRESS_FLOOR + Self::args_floor()
    }

    fn args_floor() -> f32 {
        ARG_TYPE + ARG_VALUE + ARG_BUTTON * 2.0 + 6.0
    }

    pub fn for_a_window(width: f32, show_destination: bool) -> Self {
        // Floors first, then share out whatever is left over. Taking shares
        // first and clamping afterwards looks equivalent and is not: one column
        // can sit above its floor while another is pinned to it, and the total
        // then comes out wider than the row it has to fit in. That is exactly
        // how the last button ended up off the edge of the window.
        let flexible = width - Self::fixed(show_destination);
        let spare = (flexible - Self::floors()).max(0.0);
        Self {
            at: AT_FLOOR + spare * 0.13,
            name: NAME_FLOOR + spare * 0.22,
            args: Self::args_floor() + spare * 0.30,
            address: ADDRESS_FLOOR + spare * 0.35,
        }
    }

    /// Everything a row of the table occupies, gaps included — the same sum the
    /// drawing code makes, for the test that says a row never sticks out.
    #[cfg(test)]
    pub fn whole_row(&self, show_destination: bool) -> f32 {
        Self::fixed(show_destination) + self.at + self.name + self.address + self.args
    }
}

/// Copy the ticked cues, each one landing right after the cue it came from.
///
/// Walked backwards on purpose. Done forwards, every insertion shifts the
/// positions still to be visited and the wrong cues get copied — with two
/// ticks it copies the first one twice.
fn duplicate_picked(cues: &mut Vec<cue::Cue>, picked: &[usize], suffix: &str) {
    let mut next = cues.iter().map(|cue| cue.id).max().unwrap_or(0);
    for index in picked.iter().rev() {
        next += 1;
        let mut copy = cues[*index].clone();
        copy.id = next;
        copy.name = format!("{} {}", copy.name, suffix);
        // The copy arrives switched off. It lands at the same timecode as the
        // cue it came from, and two cues firing at the same instant is almost
        // never what somebody meant — better a red tick to turn on than a
        // surprise on the night.
        copy.enabled = false;
        cues.insert(index + 1, copy);
    }
}

/// One line of the cue list.
///
/// Spacing is zero and every gap is put there by hand, because egui adds its
/// own on top of anything allocated — which is how rows end up wider than the
/// window, and how the second line of a two-message cue ends up a few pixels
/// out of step with the first.
fn row<R>(ui: &mut egui::Ui, contents: impl FnOnce(&mut egui::Ui) -> R) -> egui::InnerResponse<R> {
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 0.0;
        contents(ui)
    })
}

/// A cell of a cue row: **exactly** this wide, whatever it decides to put in
/// it. Without this a step with one argument advances the cursor less than a
/// step with two, and the buttons at the end of the line stop lining up.
fn cell<R>(ui: &mut egui::Ui, width: f32, contents: impl FnOnce(&mut egui::Ui) -> R) -> R {
    let (rect, _) = ui.allocate_exact_size(
        egui::vec2(width, CUE_ROW - 4.0),
        egui::Sense::focusable_noninteractive(),
    );
    let mut inside = ui.new_child(
        egui::UiBuilder::new()
            .max_rect(rect)
            .layout(egui::Layout::left_to_right(egui::Align::Center)),
    );
    inside.spacing_mut().item_spacing.x = 2.0;
    contents(&mut inside)
}

fn gap(ui: &mut egui::Ui) {
    ui.add_space(GAP);
}

/// A column heading, sitting over the field it names.
fn heading(ui: &mut egui::Ui, text: &str, width: f32) -> egui::Response {
    let response = ui.add_sized(
        [width, 14.0],
        egui::Label::new(egui::RichText::new(text).size(11.0).weak()).halign(egui::Align::LEFT),
    );
    gap(ui);
    response
}

/// Edit one message of a cue: where it goes, what it says, what it carries.
fn step_editor(
    ui: &mut egui::Ui,
    step: &mut cue::Step,
    outputs: &[String],
    show_destination: bool,
    words: &'static crate::text::Text,
    address_width: f32,
    args_width: f32,
) -> bool {
    let mut changed = false;

    if show_destination {
        cell(ui, DEST, |ui| {
            let chosen = step
                .to
                .clone()
                .unwrap_or_else(|| words.default_output.to_string());
            egui::ComboBox::from_id_salt(ui.next_auto_id())
                .selected_text(egui::RichText::new(shorten(&chosen, 9)).size(11.0))
                .width(DEST)
                .show_ui(ui, |ui| {
                    changed |= ui
                        .selectable_value(&mut step.to, None, words.default_output)
                        .changed();
                    for name in outputs {
                        changed |= ui
                            .selectable_value(&mut step.to, Some(name.clone()), name)
                            .changed();
                    }
                });
        });
        gap(ui);
    }

    // What sort of message this is. A cue list mixes them freely — the moment
    // that starts the video also changes a snapshot on the desk — so the kind
    // belongs on the line, not in a mode somewhere else.
    cell(ui, KIND, |ui| {
        let mut kind = Kind::of(&step.send);
        egui::ComboBox::from_id_salt(ui.next_auto_id())
            .selected_text(egui::RichText::new(kind.label(words)).size(11.0))
            .width(KIND)
            .show_ui(ui, |ui| {
                for option in Kind::ALL {
                    ui.selectable_value(&mut kind, option, option.label(words));
                }
            });
        if kind != Kind::of(&step.send) {
            step.send = kind.blank();
            changed = true;
        }
    });
    gap(ui);

    let room = address_width + args_width + GAP;
    match &mut step.send {
        cue::Message::Osc { address, args } => {
            cell(ui, address_width, |ui| {
                changed |= ui
                    .add(egui::TextEdit::singleline(address).desired_width(address_width))
                    .changed();
            });
            gap(ui);
            cell(ui, args_width, |ui| {
                changed |= args_editor(ui, args, words);
            });
            gap(ui);
        }
        cue::Message::MidiNote {
            channel,
            note,
            velocity,
        } => {
            cell(ui, room, |ui| {
                changed |= number(ui, words.midi_channel, channel, 1..=16);
                changed |= number(ui, words.midi_note_number, note, 0..=127);
                changed |= number(ui, words.midi_velocity, velocity, 0..=127);
            });
            gap(ui);
        }
        cue::Message::MidiProgramChange {
            channel,
            program,
            bank,
        } => {
            cell(ui, room, |ui| {
                changed |= number(ui, words.midi_channel, channel, 1..=16);
                changed |= number(ui, words.midi_program, program, 0..=127);
                // The bank is what reaches SuperRack snapshots past 128.
                let mut banked = bank.is_some();
                if ui
                    .checkbox(&mut banked, egui::RichText::new(words.midi_bank).size(11.0))
                    .on_hover_text(words.midi_bank_tooltip)
                    .changed()
                {
                    *bank = banked.then_some((0, 0));
                    changed = true;
                }
                if let Some((msb, lsb)) = bank {
                    changed |= number(ui, "MSB", msb, 0..=127);
                    changed |= number(ui, "LSB", lsb, 0..=127);
                }
            });
            gap(ui);
        }
        cue::Message::MidiControlChange {
            channel,
            controller,
            value,
        } => {
            cell(ui, room, |ui| {
                changed |= number(ui, words.midi_channel, channel, 1..=16);
                changed |= number(ui, words.midi_controller, controller, 0..=127);
                changed |= number(ui, words.midi_value, value, 0..=127);
            });
            gap(ui);
        }
        cue::Message::ShowControl(msc) => {
            cell(ui, room, |ui| {
                egui::ComboBox::from_id_salt(ui.next_auto_id())
                    .selected_text(egui::RichText::new(msc.command.name()).size(11.0))
                    .width(74.0)
                    .show_ui(ui, |ui| {
                        for option in cue::ShowCommand::ALL {
                            changed |= ui
                                .selectable_value(&mut msc.command, option, option.name())
                                .changed();
                        }
                    });
                ui.label(egui::RichText::new(words.msc_cue).size(11.0));
                changed |= ui
                    .add(egui::TextEdit::singleline(&mut msc.cue).desired_width(52.0))
                    .on_hover_text(words.msc_cue_tooltip)
                    .changed();
                changed |= number(ui, words.msc_device, &mut msc.device, 0..=127);
            });
            gap(ui);
        }
    }

    changed
}

/// Which sort of message a step carries, as a thing you can pick.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Kind {
    Osc,
    Note,
    Program,
    Control,
    ShowControl,
}

impl Kind {
    const ALL: [Kind; 5] = [
        Kind::Osc,
        Kind::Note,
        Kind::Program,
        Kind::Control,
        Kind::ShowControl,
    ];

    fn of(message: &cue::Message) -> Self {
        match message {
            cue::Message::Osc { .. } => Kind::Osc,
            cue::Message::MidiNote { .. } => Kind::Note,
            cue::Message::MidiProgramChange { .. } => Kind::Program,
            cue::Message::MidiControlChange { .. } => Kind::Control,
            cue::Message::ShowControl(_) => Kind::ShowControl,
        }
    }

    fn label(self, words: &'static crate::text::Text) -> &'static str {
        match self {
            Kind::Osc => "OSC",
            Kind::Note => words.kind_note,
            Kind::Program => words.kind_program,
            Kind::Control => words.kind_control,
            Kind::ShowControl => "MSC",
        }
    }

    /// A message of this kind with settings that do something harmless.
    fn blank(self) -> cue::Message {
        match self {
            Kind::Osc => cue::Message::Osc {
                address: String::new(),
                args: Vec::new(),
            },
            Kind::Note => cue::Message::MidiNote {
                channel: 1,
                note: 60,
                velocity: 127,
            },
            Kind::Program => cue::Message::MidiProgramChange {
                channel: 1,
                program: 0,
                bank: None,
            },
            Kind::Control => cue::Message::MidiControlChange {
                channel: 1,
                controller: 1,
                value: 0,
            },
            Kind::ShowControl => cue::Message::ShowControl(cue::ShowControl::default()),
        }
    }
}

/// A small labelled number, of the sort MIDI is made of.
fn number(
    ui: &mut egui::Ui,
    label: &str,
    value: &mut u8,
    range: std::ops::RangeInclusive<u8>,
) -> bool {
    ui.label(egui::RichText::new(label).size(11.0).weak());
    ui.add(egui::DragValue::new(value).range(range)).changed()
}

/// The arguments that ride along with an OSC address.
///
/// Not a single on/off box, because a single on/off box is one media server's
/// habit rather than a cue system: QLab wants **no arguments at all** on
/// `/cue/5/start`, grandMA3 wants a whole command line as a **string**, and
/// Resolume wants an **int** to trigger but a **float** for opacity. The type
/// button shows what will actually go on the wire — `i`, `f`, `s`, `T`, `F` —
/// which is the same letter the receiving end will see in the type tag.
fn args_editor(
    ui: &mut egui::Ui,
    args: &mut Vec<cue::OscArg>,
    words: &'static crate::text::Text,
) -> bool {
    let mut changed = false;
    let mut remove = None;
    let tall = ui.available_height();

    if args.is_empty() {
        ui.add_sized(
            [ARG_TYPE + ARG_VALUE, tall],
            egui::Label::new(egui::RichText::new(words.no_args).size(11.0).weak()),
        )
        .on_hover_text(words.no_args_tooltip);
        // The slot where a remove button would be, left empty on purpose: the
        // add button then sits in the same column on every line whether or not
        // that message carries anything.
        ui.add_space(ARG_BUTTON + 2.0);
    }

    for (index, arg) in args.iter_mut().enumerate() {
        // One button per argument, cycling through the types. Faster than a
        // dropdown for something changed this often, and the button says which
        // type it is in words rather than making anybody learn the letters.
        if ui
            .add_sized(
                [ARG_TYPE, tall],
                egui::Button::new(egui::RichText::new(words.arg_type(arg)).size(11.0)),
            )
            .on_hover_text(words.arg_types)
            .clicked()
        {
            *arg = next_type(arg);
            changed = true;
        }

        match arg {
            cue::OscArg::Int(value) => {
                changed |= ui
                    .add_sized([ARG_VALUE, tall], egui::DragValue::new(value))
                    .changed();
            }
            cue::OscArg::Float(value) => {
                changed |= ui
                    .add_sized([ARG_VALUE, tall], egui::DragValue::new(value).speed(0.01))
                    .changed();
            }
            cue::OscArg::Str(value) => {
                changed |= ui
                    .add_sized(
                        [ARG_VALUE, tall],
                        egui::TextEdit::singleline(value).desired_width(ARG_VALUE),
                    )
                    .changed();
            }
            // True and false ride in the type tag alone; there is nothing else
            // to say about them.
            cue::OscArg::Bool(_) => {
                ui.add_space(ARG_VALUE);
            }
        }

        if ui
            .add_sized([ARG_BUTTON, tall], egui::Button::new("−"))
            .on_hover_text(words.remove_arg)
            .clicked()
        {
            remove = Some(index);
        }
    }

    if ui
        .add_sized([ARG_BUTTON, tall], egui::Button::new("+"))
        .on_hover_text(words.add_arg)
        .clicked()
    {
        args.push(cue::OscArg::Int(1));
        changed = true;
    }

    if let Some(index) = remove {
        args.remove(index);
        changed = true;
    }
    changed
}

/// The next type round the loop, carrying the value across where that means
/// something. Changing an int to a float should keep the number.
fn next_type(arg: &cue::OscArg) -> cue::OscArg {
    match arg {
        cue::OscArg::Int(value) => cue::OscArg::Float(*value as f32),
        cue::OscArg::Float(value) => cue::OscArg::Str(format!("{value}")),
        cue::OscArg::Str(_) => cue::OscArg::Bool(true),
        cue::OscArg::Bool(true) => cue::OscArg::Bool(false),
        cue::OscArg::Bool(false) => cue::OscArg::Int(1),
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
                        let Widths {
                            at: at_width,
                            name: name_width,
                            address: address_width,
                            ..
                        } = Widths::for_a_window(width, false);

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
    fn a_row_never_sticks_out_of_the_window() {
        // The bug this exists to stop: the table was handed the width of the
        // whole panel, but the section indents its contents, so every row was
        // that much too wide and the last button of each line sat off the edge.
        // It took a screenshot to notice. It should have taken a test.
        for destinations in [false, true] {
            let narrowest = Widths::narrowest(destinations);
            for window in [narrowest, narrowest + 100.0, 1280.0, 1920.0, 3840.0] {
                let widths = Widths::for_a_window(window, destinations);
                let row = widths.whole_row(destinations);
                assert!(
                    row <= window + 0.5,
                    "at {window:.0}px wide the row needs {row:.0}px (destinations: {destinations})"
                );
            }
        }
    }

    #[test]
    fn every_field_stays_usable_however_narrow_it_gets() {
        // The floors matter more than the shares. A timecode field too small to
        // click into is worse than a row that scrolls.
        // Asked for less than it can honestly do: the floors hold and the list
        // scrolls sideways instead.
        let widths = Widths::for_a_window(400.0, true);
        assert!(
            widths.at >= AT_FLOOR,
            "the timecode field became unclickable"
        );
        assert!(widths.name >= NAME_FLOOR);
        assert!(widths.address >= ADDRESS_FLOOR);
        assert!(
            widths.args >= ARG_TYPE + ARG_VALUE + ARG_BUTTON * 2.0,
            "an argument no longer fits in its own column"
        );
    }

    #[test]
    fn the_cue_fields_grow_with_the_window() {
        // The one that was wrong, and wrong in a way that looked like arithmetic
        // and was not. The widths were always worked out correctly; the fields
        // never got them, because a Grid offers a non-final column only as much
        // room as it had last frame and a TextEdit will not take more room than
        // it is offered. The two agreed with each other for ever and dragging
        // the window did nothing.
        // Anchored to the narrowest the table can honestly be drawn, so adding
        // a column later moves the test with the layout instead of breaking it.
        let narrowest = Widths::narrowest(false);
        let narrow = address_field_width(narrowest + CHROME);
        let wide = address_field_width(narrowest + CHROME + 560.0);

        assert!(
            narrow > ADDRESS_FLOOR,
            "the address field started off at its floor"
        );
        // A quarter of the extra room, at least. The exact share is a layout
        // decision and may move; "dragging the window wider visibly helps" is
        // the thing that must never stop being true.
        let extra = 560.0;
        assert!(
            wide - narrow >= extra * 0.25,
            "widening the window by {extra:.0}px gave the address field {:.0}px more",
            wide - narrow
        );
    }

    fn three_cues() -> Vec<cue::Cue> {
        (1..=3)
            .map(|number| {
                cue::Cue::new(
                    number,
                    format!("cue {number}"),
                    ltc::Timecode::new(10, 0, number as u8, 0),
                    cue::Message::Osc {
                        address: "/go".into(),
                        args: vec![],
                    },
                )
            })
            .collect()
    }

    #[test]
    fn duplicating_several_copies_the_ones_that_were_ticked() {
        let mut cues = three_cues();
        // The first and the last. Done forwards, the second insertion would
        // land on the copy of the first instead of on cue 3.
        duplicate_picked(&mut cues, &[0, 2], "(copia)");

        let names: Vec<&str> = cues.iter().map(|cue| cue.name.as_str()).collect();
        assert_eq!(
            names,
            ["cue 1", "cue 1 (copia)", "cue 2", "cue 3", "cue 3 (copia)"],
            "each copy should sit right after the cue it came from"
        );

        let ids: std::collections::HashSet<u32> = cues.iter().map(|cue| cue.id).collect();
        assert_eq!(ids.len(), cues.len(), "two cues ended up with the same id");
    }

    #[test]
    fn a_copy_arrives_switched_off() {
        // It lands at the same timecode as the cue it came from. Two cues
        // firing at the same instant is almost never what anybody meant, and
        // the red tick says so at a glance.
        let mut cues = three_cues();
        duplicate_picked(&mut cues, &[1], "(copia)");
        assert_eq!(cues[2].at, cues[1].at);
        assert!(!cues[2].enabled, "the copy came back armed");
        assert!(cues[1].enabled, "the original was disturbed");
    }

    #[test]
    fn deleting_goes_by_id_rather_than_by_row() {
        // Selection is held by id on purpose: the list is edited and reordered
        // underneath it, and a selection that follows row numbers deletes the
        // wrong cues.
        let mut cues = three_cues();
        let picked: std::collections::HashSet<u32> = [1, 3].into_iter().collect();
        cues.retain(|cue| !picked.contains(&cue.id));
        assert_eq!(cues.len(), 1);
        assert_eq!(cues[0].id, 2);
    }

    #[test]
    fn the_filter_changes_what_is_shown_and_nothing_else() {
        // The trap in filtering a list you also edit: hiding rows must not
        // change what the row numbers mean. Deleting while a filter is on has
        // to remove the cue somebody ticked, not the one now in its place.
        let mut cues = three_cues();
        cues.push(cue::Cue::new(
            9,
            "por MIDI",
            ltc::Timecode::new(10, 1, 0, 0),
            cue::Message::MidiProgramChange {
                channel: 1,
                program: 12,
                bank: None,
            },
        ));

        let osc: Vec<u32> = cues
            .iter()
            .filter(|cue| Filter::Only(Carrier::Osc).shows(cue))
            .map(|cue| cue.id)
            .collect();
        assert_eq!(osc, [1, 2, 3]);

        let midi: Vec<u32> = cues
            .iter()
            .filter(|cue| Filter::Only(Carrier::Midi).shows(cue))
            .map(|cue| cue.id)
            .collect();
        assert_eq!(midi, [9]);

        assert_eq!(
            cues.iter()
                .filter(|cue| Filter::Everything.shows(cue))
                .count(),
            4,
            "the whole list is still one list"
        );
    }

    #[test]
    fn a_cue_that_sends_to_both_shows_up_under_both() {
        // The reason there is one list and not one per transport: this cue is a
        // single moment in the show and belongs in both views, not split in two.
        let both = cue::Cue::of(
            1,
            "video y mesa",
            ltc::Timecode::new(10, 0, 0, 0),
            vec![
                cue::Step::anywhere(cue::Message::Osc {
                    address: "/go".into(),
                    args: vec![],
                }),
                cue::Step::anywhere(cue::Message::MidiProgramChange {
                    channel: 1,
                    program: 3,
                    bank: None,
                }),
            ],
        );
        assert!(Filter::Only(Carrier::Osc).shows(&both));
        assert!(Filter::Only(Carrier::Midi).shows(&both));
        assert!(Filter::Everything.shows(&both));
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
