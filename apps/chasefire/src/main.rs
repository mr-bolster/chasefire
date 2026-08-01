//! Chasefire, the window.
//!
//! Two modes, and the small one is the one that matters. It holds four things:
//! whether the show is armed, a way into the settings, the timecode, and
//! Pablo. That is the whole of it, because that is all anyone glances at while
//! doing three other jobs. Everything else lives behind Options.

mod cuefile;
mod options;
mod pablo_view;
mod presets;
mod reminder;
mod settings;
mod text;

use eframe::egui;
use pablo::{Mood, Presentation};
use show::{Event, Runner};

/// What the window was told on the way in. Everything here will eventually be
/// a setting behind Options; until then it is the only way to point it at a
/// sound card, and it is useful to keep even afterwards for a fixed install.
#[derive(Default)]
struct Startup {
    device: Option<String>,
    /// `None` when nobody said, so the remembered one is left alone.
    channel: Option<usize>,
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
    /// Show the transport marks instead of the little guitarist. This is the
    /// default now, so the flag is kept only for scripts and habits that
    /// already have it.
    sober: bool,
    /// Ask for the little guitarist, which is no longer what turns up on its
    /// own.
    pablo: bool,
    /// Send MIDI Time Code out of this port from the moment it starts. For a
    /// machine whose whole job is to convert, booting with nobody in front of
    /// it.
    mtc: Option<String>,
    /// Open the settings window straight away. For a machine being set up, and
    /// for anybody who would rather start where the work is.
    options: bool,
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
        channel: value("--channel").and_then(|text| text.parse().ok()),
        osc: value("--osc"),
        cues: value("--cues"),
        fps: value("--fps").and_then(|text| text.parse().ok()),
        screenshot: value("--screenshot"),
        screenshot_after: value("--screenshot-after")
            .and_then(|text| text.parse().ok())
            .unwrap_or(0.3),
        demo_flash: value("--demo-flash"),
        arm: arguments.iter().any(|argument| argument == "--arm"),
        sober: arguments.iter().any(|argument| argument == "--sober"),
        pablo: arguments.iter().any(|argument| argument == "--pablo"),
        mtc: value("--mtc"),
        options: arguments.iter().any(|argument| argument == "--options"),
    }
}

/// A port nobody else is likely to want. Binding it is how this program knows
/// it is the only copy running.
const ONLY_ONE_PORT: u16 = 49213;

/// Claim the right to be the only instance.
///
/// A UDP socket rather than a lock file, and deliberately: a lock file left
/// behind by a crash blocks every future start until somebody deletes it, and
/// the person it happens to will be ten minutes from doors. A socket is
/// released by the operating system the moment the process dies, however it
/// dies. It also behaves the same on Windows without a line of platform code.
///
/// Returned rather than dropped: it has to stay open for the life of the
/// program, or the claim evaporates.
fn claim_single_instance() -> Result<std::net::UdpSocket, bool> {
    match std::net::UdpSocket::bind(("127.0.0.1", ONLY_ONE_PORT)) {
        Ok(socket) => Ok(socket),
        Err(error) if error.kind() == std::io::ErrorKind::AddrInUse => Err(true),
        // Something else went wrong — a locked-down machine, an odd network
        // stack. Not being able to check is no reason to refuse to run.
        Err(_) => Err(false),
    }
}

/// Say so, in a window, and stop.
///
/// Exiting silently would be the same sin as the crash that leaves no note:
/// somebody double-clicks the icon, nothing happens, and they double-click it
/// again. Two copies fighting over one sound card is exactly the failure this
/// is here to prevent, so the explanation has to arrive.
fn complain_already_running() -> eframe::Result {
    // Reads the saved language before doing anything else: somebody who set
    // this to Spanish should not be told off in English.
    let words = text::Text::of(settings::Settings::load().0.language);
    eprintln!("{}", words.already_running_title);
    eframe::run_native(
        "Chasefire",
        eframe::NativeOptions {
            viewport: egui::ViewportBuilder::default()
                .with_inner_size([360.0, 132.0])
                .with_resizable(false)
                .with_title("Chasefire"),
            ..Default::default()
        },
        Box::new(move |_| Ok(Box::new(AlreadyRunning { words }))),
    )
}

struct AlreadyRunning {
    words: &'static text::Text,
}

impl eframe::App for AlreadyRunning {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        ui.add_space(18.0);
        ui.vertical_centered(|ui| {
            ui.label(
                egui::RichText::new(self.words.already_running_title)
                    .size(16.0)
                    .strong(),
            );
            ui.add_space(6.0);
            ui.label(egui::RichText::new(self.words.already_running_body).size(12.0));
            ui.add_space(12.0);
            if ui.button(self.words.close).clicked() {
                ui.ctx().send_viewport_cmd(egui::ViewportCommand::Close);
            }
        });
    }
}

fn main() -> eframe::Result {
    install_crash_log();

    // Held for the life of the program: dropping it would give the claim away.
    let _only_one = match claim_single_instance() {
        Ok(socket) => Some(socket),
        Err(true) => return complain_already_running(),
        Err(false) => None,
    };

    let startup = parse_startup();
    let shoot_to = startup
        .screenshot
        .clone()
        .map(|path| (path, startup.screenshot_after));

    let mut viewport = egui::ViewportBuilder::default()
        .with_inner_size([404.0, 182.0])
        .with_min_inner_size([396.0, 178.0])
        .with_always_on_top()
        .with_title("Chasefire");
    if let Some(icon) = window_icon() {
        viewport = viewport.with_icon(icon);
    }

    eframe::run_native(
        "Chasefire",
        eframe::NativeOptions {
            viewport,
            ..Default::default()
        },
        Box::new(move |_context| Ok(Box::new(Window::new(startup, shoot_to)))),
    )
}

/// The resting mark, decoded for the title bar and the task switcher.
fn window_icon() -> Option<std::sync::Arc<egui::IconData>> {
    let decoder = png::Decoder::new(pablo::sprites::LOGO_BYTES);
    let mut reader = decoder.read_info().ok()?;
    let mut buffer = vec![0; reader.output_buffer_size()];
    let info = reader.next_frame(&mut buffer).ok()?;
    // Only RGBA is worth handling: the logo is ours and we know what it is.
    if info.color_type != png::ColorType::Rgba {
        return None;
    }
    Some(std::sync::Arc::new(egui::IconData {
        rgba: buffer[..info.buffer_size()].to_vec(),
        width: info.width,
        height: info.height,
    }))
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
    /// What the window level actually is, as opposed to what the pin says it
    /// should be. The two differ while the settings window is open.
    level_now: egui::WindowLevel,
    /// Where to write a picture of the window, and when.
    screenshot: Option<(String, f32)>,
    /// The last couple of things that happened. Two lines is deliberate: it
    /// is enough to see a cue fire and still read what came before it, and not
    /// enough to turn the corner of someone's screen into a log window.
    log: std::collections::VecDeque<String>,
    /// A wash of colour across the whole window when something fires.
    flash: Option<Flash>,
    /// The last input health reported, so it is said once and not every frame.
    last_health: show::Health,
    settings: settings::Settings,
    /// When the settings last differed from what is on disk. Saving is
    /// deferred a moment so that dragging a value does not write the file
    /// sixty times a second.
    settings_dirty_since: Option<std::time::Instant>,
    reminder: reminder::Reminder,
    options: options::Options,
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
        // Remembered settings first, then anything said on the command line,
        // which wins — somebody typing a device name means it. And what they
        // typed becomes what is remembered, so it does not have to be typed
        // again tomorrow.
        let (mut settings, settings_note) = settings::Settings::load();
        if startup.device.is_some() {
            settings.device = startup.device.clone();
        }
        if let Some(channel) = startup.channel {
            settings.channel = channel;
        }
        if startup.osc.is_some() {
            settings.osc_target = startup.osc.clone();
        }
        if startup.cues.is_some() {
            settings.cue_file = startup.cues.clone();
        }
        if startup.fps.is_some() {
            settings.frame_rate = startup.fps;
        }
        if startup.sober {
            settings.pablo = false;
        }
        if startup.pablo {
            settings.pablo = true;
        }

        // From here on the settings are the single source of truth: whatever
        // was typed has been folded into them, and what was not typed comes
        // from last time.
        let device = settings.device.clone();
        let channel = settings.channel.max(1);
        let osc = settings.osc_target.clone();
        let cue_file = settings.cue_file.clone();
        let mut runner = Runner::new(25);
        // Nothing is armed because a window opened. Arming is a decision, and
        // the only way to make it by accident should be to say so out loud.
        runner.set_armed(startup.arm);
        runner.pin_frame_rate(settings.frame_rate);
        runner.set_offset_frames(settings.offset_frames);
        runner.set_freewheel_frames(settings.freewheel_frames);

        let startup_words = text::Text::of(settings.language);
        let mut notes = Vec::new();
        if let Some(note) = settings_note {
            notes.push(note);
        }
        if startup.arm {
            notes.push(startup_words.armed_from_command_line.to_string());
        }

        if let Some(path) = &cue_file {
            match std::fs::read_to_string(path)
                .map_err(|error| error.to_string())
                .and_then(|text| serde_json::from_str(&text).map_err(|error| error.to_string()))
            {
                Ok(cues) => {
                    let cues: Vec<cue::Cue> = cues;
                    notes.push(
                        startup_words
                            .cues_loaded
                            .replace("{}", &cues.len().to_string()),
                    );
                    runner.set_cues(cues);
                }
                Err(error) => notes.push(format!("cues: {error}")),
            }
        }

        // Everything that was wired last time, wired again. Each failure is
        // named and the rest still come up: a MIDI port that is not in the
        // building tonight must not take the video server down with it.
        for trouble in runner.restore_wiring(&settings.outputs) {
            notes.push(trouble);
        }

        // The command line wins over what was remembered, because somebody who
        // typed an address meant that address.
        if let Some(target) = &osc {
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
        match runner.open_input(device.as_deref(), channel) {
            Ok(()) => {}
            // Not fatal, and deliberately so. No sound card is a thing to say
            // in the window, not a reason for the window to vanish.
            Err(error) => notes.push(startup_words.audio_error(&error)),
        }

        // The clock too: it was switched on deliberately last time, and a
        // machine that boots into converting should boot into converting.
        if let Some(port) = startup.mtc.clone().or_else(|| settings.mtc_port.clone()) {
            match runner.start_mtc(&port) {
                Ok(()) => notes.push(startup_words.mtc_sending.replace("{}", &port)),
                Err(error) => notes.push(error),
            }
        }

        // Written once on the way in, not only when something later changes.
        // Otherwise a first run that was given everything on the command line
        // would remember none of it: nothing had changed, so nothing was saved,
        // and tomorrow it would all have to be typed again.
        let _ = settings.save();

        Self {
            runner,
            pablo: pablo_view::PabloView::new(),
            presentation: if settings.pablo {
                Presentation::Pablo
            } else {
                Presentation::Plain
            },
            always_on_top: settings.always_on_top,
            // What the window was *built* as, not what the settings want.
            // The viewport is always created pinned, so if the settings say
            // otherwise the first frame has to say so — and before this,
            // nothing ever did: the pin came back on every launch no matter
            // what the button had been left saying.
            level_now: egui::WindowLevel::AlwaysOnTop,
            // A few frames of grace so the layout has settled before the shot.
            screenshot,
            log: notes.into_iter().rev().take(2).collect(),
            last_health: show::Health::Closed,
            reminder: reminder::Reminder::new(settings.reminders_dismissed),
            settings_dirty_since: None,
            settings,
            options: {
                let mut options =
                    options::Options::new(device.clone(), channel, osc.clone(), cue_file.clone());
                options.open = startup.options;
                options
            },
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

    /// What the settings would be if written right now.
    fn current_settings(&self) -> settings::Settings {
        settings::Settings {
            device: self
                .runner
                .device_name()
                .map(str::to_string)
                .or_else(|| self.settings.device.clone()),
            channel: self.runner.channel().unwrap_or(self.settings.channel),
            frame_rate: self.runner.pinned_frame_rate(),
            osc_target: self
                .runner
                .output_target()
                .or_else(|| self.settings.osc_target.clone()),
            outputs: self.runner.wiring().to_vec(),
            mtc_port: self.runner.mtc_port().map(str::to_string),
            cue_file: self.settings.cue_file.clone(),
            offset_frames: self.runner.offset_frames(),
            freewheel_frames: self.runner.freewheel_frames(),
            pablo: self.presentation == Presentation::Pablo,
            always_on_top: self.always_on_top,
            language: self.settings.language,
            reminders_dismissed: self.reminder.dismissed(),
        }
    }

    /// Notice a change and write it out shortly after.
    ///
    /// Saving only on the way out was not good enough: a process that is killed
    /// — or a machine that loses power, which happens in venues — never gets
    /// to run its exit code, and the setting somebody carefully chose an hour
    /// ago is gone. Watching for changes means the file is right within a
    /// second of any of them, however the program ends afterwards.
    fn remember_if_changed(&mut self) {
        let current = self.current_settings();
        if current != self.settings {
            self.settings = current;
            if self.settings_dirty_since.is_none() {
                self.settings_dirty_since = Some(std::time::Instant::now());
            }
        }
        // A moment's delay so that dragging a slider writes once, not once per
        // frame.
        if let Some(since) = self.settings_dirty_since {
            if since.elapsed().as_secs_f32() > 0.75 {
                self.settings_dirty_since = None;
                self.remember();
            }
        }
    }

    /// Write the settings down. Failing to is worth a line in the log and
    /// nothing more: not being able to save a preference is not a reason to
    /// interrupt anybody.
    fn remember(&mut self) {
        self.settings = self.current_settings();
        if let Err(error) = self.settings.save() {
            let message = self.text().settings_not_saved.replace("{}", &error);
            self.note(message);
        }
    }

    /// The words, in whichever language is set. Static, so holding one does
    /// not borrow the window and tie up everything else.
    fn text(&self) -> &'static text::Text {
        text::Text::of(self.settings.language)
    }

    fn timecode_text(&self) -> String {
        match self.runner.timecode() {
            Some(timecode) => timecode.to_string(),
            None => "--:--:--:--".to_string(),
        }
    }
}

impl eframe::App for Window {
    /// Write everything down on the way out.
    ///
    /// Saving only when the reminder was closed left a hole: change the input
    /// device in Options, close the program, and the change was gone. Settings
    /// that quietly fail to stick are worse than no settings at all, because
    /// somebody will set them once and then trust them.
    fn on_exit(&mut self, _gl: Option<&eframe::glow::Context>) {
        self.remember();
    }

    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let context = ui.ctx().clone();
        let context = &context;
        let words = self.text();
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
                Event::SignalLost => words.signal_lost.to_string(),
            };
            self.note(line);
        }

        // Say why there is no timecode, when there is a reason worth saying.
        // Repeated only when it changes, so a card that is out for ten minutes
        // does not fill the log with the same line six hundred times.
        let health = self.runner.health();
        if health != self.last_health {
            self.last_health = health;
            if let Some(message) = words.health(health, self.runner.channel().unwrap_or(1)) {
                self.note(message);
            }
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
                            (None, _) => words.no_input.to_string(),
                        };
                        ui.label(
                            egui::RichText::new(source)
                                .size(12.0)
                                .strong()
                                .color(egui::Color32::from_rgb(185, 190, 200)),
                        );
                        // Sending the clock back out is a thing other machines
                        // are relying on, and until now the window said nothing
                        // about it: somebody could have it running, or not
                        // running, and no way to tell from here.
                        if let Some(port) = self.runner.mtc_port() {
                            ui.add_space(8.0);
                            ui.label(
                                egui::RichText::new(words.mtc_badge)
                                    .size(11.0)
                                    .monospace()
                                    .strong()
                                    .color(egui::Color32::from_rgb(120, 200, 140)),
                            )
                            .on_hover_text(words.mtc_sending.replace("{}", port));
                        }
                        // The short word, not the sentence: it balances against
                        // the source on the left instead of running off the
                        // edge. The full sentence is a hover away, and Pablo is
                        // saying the same thing in pictures anyway.
                        let (red, green, blue) = mood.colour();
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            ui.label(
                                egui::RichText::new(words.badge(mood))
                                    .size(12.0)
                                    .strong()
                                    .color(egui::Color32::from_rgb(red, green, blue)),
                            )
                            .on_hover_text(words.mood(mood));
                        });
                    });
                });

                // The countdown gets a line of its own. After the timecode it
                // is the number people actually want, and squeezing it in
                // beside something else was what made this look thrown together.
                // An input that will not open is more urgent than a countdown,
                // and it is long enough that the two-line log truncates it into
                // uselessness. It goes where the eye already is.
                let trouble = self
                    .runner
                    .error()
                    .map(str::to_string)
                    .or_else(|| words.health(health, self.runner.channel().unwrap_or(1)));

                ui.allocate_ui(egui::vec2(width, 20.0), |ui| {
                    if let Some(message) = &trouble {
                        ui.label(
                            egui::RichText::new(trim_to(message, 58))
                                .size(11.5)
                                .color(egui::Color32::from_rgb(235, 130, 100)),
                        )
                        .on_hover_text(message);
                        return;
                    }
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
                                    egui::RichText::new(format!(
                                        "{}  {}",
                                        words.next_cue,
                                        trim_to(&name, 24)
                                    ))
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
                            ui.label(egui::RichText::new(words.no_cue_ahead).size(13.0).weak());
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
                (words.armed, egui::Color32::from_rgb(70, 200, 110))
            } else {
                (words.disarmed, egui::Color32::from_rgb(210, 150, 40))
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
                .add(egui::Button::new(words.options).min_size(egui::vec2(options_width, ROW)))
                .clicked()
            {
                self.options.open = true;
            }

            ui.add_space(GAP);
            // The pin takes the arm colour too, so the two lit things in the
            // row agree instead of arguing: green while the show is live,
            // amber while it is not, plain when the pin is off.
            let pin_button = if self.always_on_top {
                egui::Button::new(
                    egui::RichText::new(words.pin)
                        .strong()
                        .color(egui::Color32::BLACK),
                )
                .fill(colour)
            } else {
                egui::Button::new(egui::RichText::new(words.pin).weak())
            };
            if ui
                .add(pin_button.min_size(egui::vec2(pin_width, ROW)))
                .on_hover_text(words.pin_tooltip)
                .clicked()
            {
                self.always_on_top = !self.always_on_top;
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
            // Halves, with the divider down the middle where it belongs.
            let left_zone = (content - GAP) * 0.5;
            let right_zone = content - GAP - left_zone;

            // Left: what this is wired to. The labels are a fixed-width column
            // so the values line up under each other instead of stepping in and
            // out with the length of the word before them.
            ui.allocate_ui(egui::vec2(left_zone, 30.0), |ui| {
                ui.vertical(|ui| {
                    // Nothing between the two lines: they are one block of
                    // information, and the gap left the second one sitting
                    // lower than its opposite number across the divider.
                    ui.spacing_mut().item_spacing.y = 0.0;
                    // Plain labels, not rows of widgets. A horizontal layout
                    // brings its own height with it, which left this column
                    // taller than the log across the divider and the second
                    // line sitting lower than its opposite number. Monospace
                    // does the aligning that the widget row was there for.
                    // How much fits on one of these lines after the label.
                    // Monospace, so characters are a fair unit.
                    const LINE: usize = 27;
                    let line = |label: &str, value: String| {
                        egui::RichText::new(format!("{label:<4}{value}"))
                            .monospace()
                            .size(10.0)
                            .weak()
                    };
                    ui.label(line(
                        words.in_label,
                        match self.runner.device_name() {
                            Some(name) => {
                                // The channel is never dropped; the card name
                                // gives way instead. Worked out from the words
                                // in use rather than fixed, because "ch" is two
                                // characters and "canal" is five, and the line
                                // wrapped the first time it was read in Spanish.
                                let tail = format!(
                                    "{} {}",
                                    words.channel_short,
                                    self.runner.channel().unwrap_or(1)
                                );
                                format!(
                                    "{}  {tail}",
                                    shorten(name, LINE.saturating_sub(tail.chars().count() + 2))
                                )
                            }
                            None => "—".to_string(),
                        },
                    ));
                    let out = ui.label(line(
                        words.out_label,
                        match self.runner.output_target() {
                            // Cut from the end, not the start: with four
                            // outputs the first names are the ones somebody
                            // is looking for, and `shorten` keeps the tail
                            // because that is right for a device name and
                            // wrong for a list.
                            Some(target) => trim_tail(&target, LINE),
                            None => "—".to_string(),
                        },
                    ));
                    // Which machine each name actually is, a hover away. The
                    // corner has room for names and nothing else; this is
                    // where the addresses live.
                    let described = self.runner.outputs_described();
                    if !described.is_empty() {
                        out.on_hover_text(described.join("\n"));
                    }
                });
            });

            // A hairline between the two halves, placed from the same geometry
            // as everything else rather than from wherever the cursor happens
            // to have ended up.
            let divider = left + left_zone + GAP * 0.5;
            let top = ui.cursor().top();
            ui.painter().vline(
                divider,
                top..=(top + 26.0),
                egui::Stroke::new(1.0, ui.visuals().weak_text_color().gamma_multiply(0.4)),
            );

            ui.add_space(GAP);
            ui.allocate_ui(egui::vec2(right_zone, 30.0), |ui| {
                // Centred in its zone rather than shoved against the divider,
                // which made the right-hand zone look like an overflow of the
                // left one instead of a column of its own.
                ui.vertical_centered(|ui| {
                    ui.spacing_mut().item_spacing.y = 0.0;
                    // Clipped to its zone. A log line long enough to reach
                    // across the divider makes the split look accidental.
                    for line in &self.log {
                        ui.label(small(&trim_to(line, 30)));
                    }
                });
            });
        });

        self.options.show(
            context,
            &mut self.runner,
            &mut self.presentation,
            &mut self.settings.language,
        );

        // While the settings window is open the corner one steps out of the
        // way. Pinned above everything it sits exactly on top of the input
        // section, and somebody who has opened the settings is at the machine
        // configuring it, not watching a corner. The pin itself is untouched:
        // it comes back the moment the settings close.
        let wanted = if self.options.open || !self.always_on_top {
            egui::WindowLevel::Normal
        } else {
            egui::WindowLevel::AlwaysOnTop
        };
        if wanted != self.level_now {
            self.level_now = wanted;
            context.send_viewport_cmd(egui::ViewportCommand::WindowLevel(wanted));
        }

        // The reminder waits for the window to settle, and stands down for good
        // the moment a show is running. `busy` is the whole safety rule.
        let busy = self.runner.is_armed() || self.runner.timecode().is_some();
        self.reminder
            .update(context.input(|input| input.stable_dt), busy);
        if self.reminder.show(context, options::DONATE_URL, words) {
            self.remember();
        }
        self.remember_if_changed();

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
/// Trim a device name to `room` characters, keeping the tail — the end of an
/// ALSA or WASAPI name is the part that tells one card from another.
/// Cut a list down, keeping the beginning. The opposite of `shorten`, and for
/// the opposite reason: a list is read from the front.
fn trim_tail(text: &str, room: usize) -> String {
    if text.chars().count() <= room {
        return text.to_string();
    }
    let head: String = text.chars().take(room.saturating_sub(1)).collect();
    format!("{head}…")
}

fn shorten(name: &str, room: usize) -> String {
    let room = room.max(8);
    if name.chars().count() <= room {
        return name.to_string();
    }
    let tail: String = name
        .chars()
        .skip(name.chars().count().saturating_sub(room - 1))
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
