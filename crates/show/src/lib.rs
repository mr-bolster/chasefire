// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
//
// Copyright (c) Leo Bolster.

//! The running show: everything wired together, stepped from one place.
//!
//! Capture feeds the chaser, the chaser feeds the cue engine, the cue engine
//! feeds the outputs. Each of those is its own crate with its own tests; this
//! is the loom they hang on. It exists so the window and the command line
//! drive exactly the same machine — the moment there are two copies of this
//! wiring, one of them starts being subtly wrong.

use chase::{Chaser, Signal};
use cue::{Cue, Engine, Firing};
use ltc::Timecode;
use sink::{MidiSink, MtcClock, MtcInput, NetworkMidiSink, OscSink, Outputs};
use std::time::{Duration, Instant};

/// Whether the input is actually delivering anything.
///
/// Three different ways for a sound card to look fine and give you nothing, and
/// all three end the same way on screen — no timecode — which is why they are
/// worth telling apart. Guessing which one it is has cost me an evening.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Health {
    /// Nothing is open.
    Closed,
    /// Open, samples arriving, signal present. Normal.
    Fine,
    /// Open, but the audio callback has stopped running. The device is there
    /// and it is not delivering: usually something took it away.
    NotDelivering,
    /// Samples arriving, but nothing on them. Almost always the wrong channel.
    Silent,
}

impl Health {
    /// What to tell the operator, in words they can act on.
    pub fn describe(self, channel: usize) -> Option<String> {
        match self {
            Health::Closed | Health::Fine => None,
            Health::NotDelivering => {
                Some("the input stopped delivering audio — has another program taken the card?".into())
            }
            Health::Silent => Some(format!(
                "audio is arriving but channel {channel} is silent — wrong channel, or nothing plugged in?"
            )),
        }
    }
}

/// Where the timecode is coming from.
///
/// Only one of these is built so far, but the distinction belongs in the model
/// rather than being assumed: an operator glancing at the window needs to know
/// which of the two they are actually chasing, because the cable that broke is
/// a different cable.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Source {
    /// Audio, off a sound card.
    Ltc,
    /// MIDI Time Code, off a MIDI port. Not built yet.
    Mtc,
}

impl Source {
    pub fn label(self) -> &'static str {
        match self {
            Source::Ltc => "LTC",
            Source::Mtc => "MTC",
        }
    }
}

/// Something worth telling the operator about.
#[derive(Debug, Clone)]
pub enum Event {
    /// A cue went out, or tried to.
    Fired {
        firing: Firing,
        sent: Result<(), String>,
    },
    /// The frame rate was worked out from the signal.
    Locked { fps: f64, nominal: u8 },
    /// Timecode has been gone long enough that we stopped counting.
    SignalLost,
}

/// Where a cue is going, for the sake of the animation.
///
/// A cue can now send to several places at once, so there is a choice to make.
/// MIDI wins: it is the rarer flourish, and a cue that moves a desk is the one
/// worth noticing out of the corner of an eye.
fn flourish_for(steps: &[cue::Step]) -> pablo::Flourish {
    if steps
        .iter()
        .any(|step| step.send.carried_by() == cue::Carrier::Midi)
    {
        pablo::Flourish::Midi
    } else {
        pablo::Flourish::Osc
    }
}

/// How an output was made, so it can be made again next time.
///
/// The sinks themselves cannot be asked this: a socket knows where it is
/// pointing but not that it was called "video", and a MIDI connection has no
/// memory of the name it was opened by. So the recipe is kept beside them —
/// which is also exactly what has to be written to the settings file for a
/// show that is rebuilt every night to stop being rebuilt by hand.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub enum Wiring {
    Osc {
        name: String,
        target: String,
    },
    Midi {
        name: String,
        port: String,
    },
    Network {
        name: String,
        port: u16,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        peer: Option<String>,
    },
}

impl Wiring {
    pub fn name(&self) -> &str {
        match self {
            Wiring::Osc { name, .. } | Wiring::Midi { name, .. } | Wiring::Network { name, .. } => {
                name
            }
        }
    }
}

pub struct Runner {
    capture: Option<audio::Capture>,
    chaser: Chaser,
    engine: Engine,
    outputs: Outputs,
    /// How each output was made, in the order it was made.
    wiring: Vec<Wiring>,
    /// Set when timecode is arriving from somewhere other than the sound card.
    external_source: Option<Source>,
    /// Sending the timecode we are chasing back out as MIDI Time Code, when
    /// somebody asked for that. It keeps its own clock on its own thread.
    mtc: Option<MtcClock>,
    /// Chasing MTC instead of LTC: a DAW on this machine, or a Mac over the
    /// network. No sound card and no cable involved.
    mtc_in: Option<MtcInput>,
    /// What the single OSC destination is called when nobody has named any.
    /// Cues that name nowhere land here, so a one-machine show never has to
    /// learn that outputs have names at all.
    default_osc: Option<String>,
    /// The rate the operator pinned, if they pinned one. Left alone, the rate
    /// is worked out from the signal at the cost of two extra frames of lock.
    pinned_fps: Option<f64>,
    settled: bool,
    samples_per_frame: f64,
    last_good_sample: u64,
    freewheel_ticks: u64,
    current: Option<Timecode>,
    /// Wall clock of the last accepted frame, for the nod.
    last_frame_at: Option<Instant>,
    /// Sample counter and when it last moved, for spotting a dead stream.
    last_sample_count: u64,
    samples_moved_at: Option<Instant>,
    /// When the level was last above the noise floor.
    signal_seen_at: Option<Instant>,
    error: Option<String>,
}

impl Runner {
    pub fn new(nominal_fps: u8) -> Self {
        Self {
            capture: None,
            chaser: Chaser::new(nominal_fps),
            engine: Engine::new(nominal_fps),
            outputs: Outputs::new(),
            wiring: Vec::new(),
            external_source: None,
            mtc: None,
            mtc_in: None,
            default_osc: None,
            pinned_fps: None,
            settled: false,
            samples_per_frame: 48_000.0 / nominal_fps as f64,
            last_good_sample: 0,
            freewheel_ticks: 0,
            current: None,
            last_frame_at: None,
            last_sample_count: 0,
            samples_moved_at: None,
            signal_seen_at: None,
            error: None,
        }
    }

    pub fn set_cues(&mut self, cues: Vec<Cue>) {
        self.engine.set_cues(cues);
    }

    pub fn cues(&self) -> &[Cue] {
        self.engine.cues()
    }

    pub fn set_offset_frames(&mut self, frames: i32) {
        self.engine.set_offset_frames(frames);
    }

    pub fn offset_frames(&self) -> i32 {
        self.engine.offset_frames()
    }

    /// Pin the frame rate, or pass `None` to work it out from the signal.
    pub fn pin_frame_rate(&mut self, fps: Option<f64>) {
        self.pinned_fps = fps;
        self.settled = fps.is_some();
        if let Some(fps) = fps {
            let nominal = fps.ceil() as u8;
            self.engine.set_nominal_fps(nominal);
            self.chaser.set_nominal_fps(nominal);
        }
    }

    pub fn open_input(
        &mut self,
        device: Option<&str>,
        channel: usize,
    ) -> Result<(), audio::AudioError> {
        let capture = match audio::Capture::open(device, channel, self.pinned_fps) {
            Ok(capture) => capture,
            Err(error) => {
                self.error = Some(error.to_string());
                return Err(error);
            }
        };
        self.samples_per_frame = capture.sample_rate() as f64 / self.pinned_fps.unwrap_or(25.0);
        self.last_good_sample = capture.samples_processed();
        self.last_sample_count = self.last_good_sample;
        let now = Instant::now();
        self.samples_moved_at = Some(now);
        self.signal_seen_at = Some(now);
        self.capture = Some(capture);
        self.error = None;
        Ok(())
    }

    pub fn close_input(&mut self) {
        self.capture = None;
        self.chaser.reset();
        self.current = None;
    }

    /// Point the unnamed OSC output at a machine. This is the one a show with
    /// a single destination uses without ever naming it.
    pub fn connect_osc(&mut self, target: &str) -> Result<(), String> {
        self.connect_osc_as("osc", target)
    }

    /// Point a **named** OSC output at a machine, so cues can address it.
    pub fn connect_osc_as(&mut self, name: &str, target: &str) -> Result<(), String> {
        let sink = OscSink::connect(target).map_err(|error| error.to_string())?;
        self.outputs.put(name, Box::new(sink));
        self.note_wiring(Wiring::Osc {
            name: name.to_string(),
            target: target.to_string(),
        });
        if self.default_osc.is_none() {
            self.default_osc = Some(name.to_string());
        }
        Ok(())
    }

    /// Everywhere this machine can send MIDI.
    pub fn midi_ports() -> Vec<String> {
        MidiSink::ports().unwrap_or_default()
    }

    /// Open a local MIDI port under a name cues can address.
    pub fn connect_midi_as(&mut self, name: &str, port: &str) -> Result<(), String> {
        let sink = MidiSink::open(port)?;
        self.outputs.put(name, Box::new(sink));
        self.note_wiring(Wiring::Midi {
            name: name.to_string(),
            port: port.to_string(),
        });
        Ok(())
    }

    /// Start an RTP-MIDI session under a name cues can address.
    ///
    /// With a `peer` this end invites; without, it waits to be invited, which
    /// is what a Mac or rtpMIDI does when somebody presses Connect over there.
    pub fn connect_network_midi_as(
        &mut self,
        name: &str,
        port: u16,
        peer: Option<&str>,
    ) -> Result<(), String> {
        let peer = match peer {
            Some(text) if !text.trim().is_empty() => Some(
                text.trim()
                    .parse()
                    .map_err(|_| format!("'{}' is not an address and a port", text.trim()))?,
            ),
            _ => None,
        };
        let sink = NetworkMidiSink::start(name, port, peer)?;
        self.outputs.put(name, Box::new(sink));
        self.note_wiring(Wiring::Network {
            name: name.to_string(),
            port,
            peer: peer.map(|at| at.to_string()),
        });
        Ok(())
    }

    /// Start sending MIDI Time Code out of a port.
    ///
    /// This is the program's other job: a rig with LTC on a cable and a machine
    /// that only speaks MTC has a hole in it, and this fills it.
    pub fn start_mtc(&mut self, port: &str) -> Result<(), String> {
        self.mtc = Some(MtcClock::start(port)?);
        Ok(())
    }

    /// Everything this machine can listen to for MTC.
    pub fn mtc_input_ports() -> Vec<String> {
        MtcInput::ports().unwrap_or_default()
    }

    /// Chase MTC from a MIDI port instead of LTC from a sound card.
    ///
    /// The two are mutually exclusive on purpose: chasing two clocks at once
    /// is not a feature, it is a way to have the show in two places.
    pub fn open_mtc_input(&mut self, port: &str) -> Result<(), String> {
        let input = MtcInput::open(port)?;
        self.close_input();
        self.external_source = Some(Source::Mtc);
        self.settled = false;
        self.mtc_in = Some(input);
        Ok(())
    }

    pub fn close_mtc_input(&mut self) {
        self.mtc_in = None;
        if self.capture.is_none() {
            self.external_source = None;
            self.current = None;
        }
    }

    /// Which MIDI port MTC is arriving on, if it is.
    pub fn mtc_input_port(&self) -> Option<&str> {
        self.mtc_in.as_ref().map(|input| input.port())
    }

    pub fn stop_mtc(&mut self) {
        self.mtc = None;
    }

    /// Which port MTC is going out of, if it is.
    pub fn mtc_port(&self) -> Option<&str> {
        self.mtc.as_ref().map(|clock| clock.port())
    }

    fn note_wiring(&mut self, wiring: Wiring) {
        self.wiring
            .retain(|existing| existing.name() != wiring.name());
        self.wiring.push(wiring);
    }

    pub fn disconnect_output(&mut self, name: &str) -> bool {
        if self.default_osc.as_deref() == Some(name) {
            self.default_osc = None;
        }
        self.wiring.retain(|existing| existing.name() != name);
        self.outputs.remove(name)
    }

    /// How everything is wired, for writing down.
    pub fn wiring(&self) -> &[Wiring] {
        &self.wiring
    }

    /// Wire it all up again from what was written down.
    ///
    /// Returns what could not be reconnected, by name and reason — never an
    /// error for the whole thing. A MIDI port that is not in the building
    /// tonight must not stop the other three outputs coming back.
    pub fn restore_wiring(&mut self, wiring: &[Wiring]) -> Vec<String> {
        let mut trouble = Vec::new();
        for one in wiring {
            let outcome = match one {
                Wiring::Osc { name, target } => self.connect_osc_as(name, target),
                Wiring::Midi { name, port } => self.connect_midi_as(name, port),
                Wiring::Network { name, port, peer } => {
                    self.connect_network_midi_as(name, *port, peer.as_deref())
                }
            };
            if let Err(why) = outcome {
                trouble.push(format!("{}: {why}", one.name()));
            }
        }
        trouble
    }

    /// The names of everywhere cues can be sent.
    pub fn output_names(&self) -> Vec<String> {
        self.outputs.names().map(|name| name.to_string()).collect()
    }

    pub fn output_described(&self, name: &str) -> Option<String> {
        self.outputs.describe(name)
    }

    /// Where cues are being sent, for the window to show. Somebody staring at
    /// a corner of a screen for six hours should not have to remember which
    /// machine they pointed this at.
    pub fn output_target(&self) -> Option<String> {
        let first = self
            .default_osc
            .clone()
            .or_else(|| self.outputs.names().next().map(|name| name.to_string()))?;
        let described = self.outputs.describe(&first)?;
        // "OSC to 10.0.0.5:7000" is what a single output is called; with more
        // than one, say how many so the corner is honest about it.
        let count = self.outputs.names().count();
        Some(if count > 1 {
            format!("{described} +{}", count - 1)
        } else {
            described
        })
    }

    /// How long the chaser keeps counting after the signal goes.
    pub fn set_freewheel_frames(&mut self, frames: u32) {
        self.chaser.set_freewheel_frames(frames);
    }

    pub fn freewheel_frames(&self) -> u32 {
        self.chaser.freewheel_frames()
    }

    /// The rate the operator pinned, if any. `None` means it is measured.
    pub fn pinned_frame_rate(&self) -> Option<f64> {
        self.pinned_fps
    }

    /// True when a sound card is open.
    pub fn is_listening(&self) -> bool {
        self.capture.is_some()
    }

    /// Load a cue list from a JSON file, replacing whatever is loaded.
    pub fn load_cues(&mut self, path: &std::path::Path) -> Result<usize, String> {
        let text = std::fs::read_to_string(path).map_err(|error| error.to_string())?;
        let cues: Vec<Cue> = serde_json::from_str(&text).map_err(|error| error.to_string())?;
        let count = cues.len();
        self.engine.set_cues(cues);
        Ok(count)
    }

    /// The next cue due, and how many seconds away it is.
    ///
    /// The single most useful thing a show tool can put on screen after the
    /// timecode itself: it answers "have I got time" without anybody doing
    /// arithmetic in their head at four in the morning.
    pub fn countdown(&self) -> Option<(String, f64)> {
        let now = self.current?;
        let rate = self.frame_rate()?;
        let cue = self.engine.next_cue_after(now)?;
        let nominal = self.engine.nominal_fps() as u32;
        let frames = cue.at.as_frame_count(nominal) as i64 - now.as_frame_count(nominal) as i64;
        // The offset fires cues early, so the wait is shorter by exactly that.
        let frames = frames - self.engine.offset_frames() as i64;
        Some((cue.name.clone(), frames.max(0) as f64 / rate))
    }

    /// Whether the input is delivering, and if not, which way it is failing.
    pub fn health(&self) -> Health {
        let Some(capture) = &self.capture else {
            return Health::Closed;
        };
        // A stream whose sample counter has not moved for a second is not a
        // stream. Anything above about a hundred milliseconds is already far
        // outside normal, so a second is generous and still quick to notice.
        if let Some(moved) = self.samples_moved_at {
            if moved.elapsed().as_secs_f32() > 1.0 {
                return Health::NotDelivering;
            }
        }
        // Samples arriving with nothing on them. Below this the decoder could
        // not work anyway, so calling it silence is not a judgement call.
        let quiet = capture
            .level_dbfs()
            .map(|level| level < -70.0)
            .unwrap_or(true);
        if quiet {
            if let Some(seen) = self.signal_seen_at {
                if seen.elapsed().as_secs_f32() > 2.0 {
                    return Health::Silent;
                }
            }
        }
        Health::Fine
    }

    /// What kind of timecode this is chasing, if anything.
    pub fn source(&self) -> Option<Source> {
        // An audio input can only ever be carrying LTC. Anything fed in from
        // elsewhere says for itself what it is.
        self.external_source
            .or_else(|| self.capture.as_ref().map(|_| Source::Ltc))
    }

    /// The frame rate in force: pinned by the operator, or measured.
    pub fn frame_rate(&self) -> Option<f64> {
        if let Some(pinned) = self.pinned_fps {
            return Some(pinned);
        }
        self.settled.then(|| self.engine.nominal_fps() as f64)
    }

    /// Which input channel is being read.
    pub fn channel(&self) -> Option<usize> {
        self.capture.as_ref().map(|capture| capture.channel())
    }

    pub fn set_armed(&mut self, armed: bool) {
        self.engine.set_armed(armed);
    }

    pub fn is_armed(&self) -> bool {
        self.engine.is_armed()
    }

    pub fn timecode(&self) -> Option<Timecode> {
        self.current
    }

    pub fn pending_cues(&self) -> usize {
        self.engine.pending_count()
    }

    pub fn next_cue(&self) -> Option<&Cue> {
        self.current.and_then(|now| self.engine.next_cue_after(now))
    }

    pub fn device_name(&self) -> Option<&str> {
        self.capture.as_ref().map(|capture| capture.device_name())
    }

    pub fn error(&self) -> Option<&str> {
        self.error.as_deref()
    }

    pub fn rejections(&self) -> chase::Rejections {
        self.chaser.rejections()
    }

    /// How Pablo should be feeling, and therefore what is actually going on.
    pub fn situation(&self) -> pablo::Situation {
        pablo::Situation {
            armed: self.engine.is_armed(),
            locked: matches!(self.chaser.signal(), Signal::Locked),
            freewheeling: matches!(self.chaser.signal(), Signal::Freewheeling { .. }),
            level_dbfs: self
                .capture
                .as_ref()
                .and_then(|capture| capture.level_dbfs()),
        }
    }

    /// Seconds since the last accepted frame, for driving a nod in time with
    /// the timecode rather than with a free-running clock.
    pub fn since_last_frame(&self) -> Option<f32> {
        self.last_frame_at.map(|at| at.elapsed().as_secs_f32())
    }

    /// Do a slice of work. Call it as often as you like; it never blocks.
    pub fn poll(&mut self) -> Vec<Event> {
        let mut events = Vec::new();

        // MTC first, and on its own: chasing two clocks at once is not a
        // feature. A position every two frames is all this source gives; the
        // engine only needs to be told where the show is, and it is the same
        // engine either way.
        if let Some(input) = &self.mtc_in {
            let mut settled_rate = None;
            for position in input.drain() {
                if !self.settled {
                    let nominal = position.rate.fps().ceil() as u8;
                    self.engine.set_nominal_fps(nominal);
                    self.chaser.set_nominal_fps(nominal);
                    self.settled = true;
                    settled_rate = Some((position.rate.fps(), nominal));
                }
                self.current = Some(position.at);
                self.last_frame_at = Some(Instant::now());
                self.tell_the_mtc_clock(position.at);
                let fired = self.engine.update(position.at, false);
                self.deliver(fired, &mut events);
            }
            if let Some((fps, nominal)) = settled_rate {
                events.push(Event::Locked { fps, nominal });
            }

            // Nothing for a while means the far end stopped or went away.
            // Silence is how MTC says "stopped" — there is no separate word
            // for it, so the timeout is the whole of the detection.
            if let Some(last) = self.last_frame_at {
                if last.elapsed() > Duration::from_millis(500) && self.current.is_some() {
                    events.push(Event::SignalLost);
                    if let Some(clock) = &self.mtc {
                        clock.lost();
                    }
                    self.engine.signal_lost();
                    self.current = None;
                }
            }
            return events;
        }

        // Drain the audio thread's queue first and let go of the capture, so
        // the rest of this can touch the engine. The frames are small and
        // there are never many: at 25 fps this collects one or two.
        let (incoming, samples_seen, sample_rate) = {
            let Some(capture) = &self.capture else {
                return events;
            };
            let mut incoming = Vec::new();
            while let Some(frame) = capture.next_frame() {
                incoming.push(frame);
            }
            (incoming, capture.samples_processed(), capture.sample_rate())
        };

        // Watch the stream itself, not just what it decodes. A card can be
        // open and healthy-looking while delivering nothing at all.
        let now = Instant::now();
        if samples_seen != self.last_sample_count {
            self.last_sample_count = samples_seen;
            self.samples_moved_at = Some(now);
        }
        if let Some(capture) = &self.capture {
            if capture
                .level_dbfs()
                .map(|level| level > -70.0)
                .unwrap_or(false)
            {
                self.signal_seen_at = Some(now);
            }
        }

        let drained = incoming.len();
        for frame in incoming {
            if !self.settled {
                if let Some(rate) = ltc::snap_to_known_frame_rate(frame.estimated_fps as f64) {
                    let nominal = rate.ceil() as u8;
                    self.engine.set_nominal_fps(nominal);
                    self.chaser.set_nominal_fps(nominal);
                    self.samples_per_frame = sample_rate as f64 / rate;
                    self.settled = true;
                    events.push(Event::Locked { fps: rate, nominal });
                }
            }

            self.last_good_sample = samples_seen;
            self.freewheel_ticks = 0;

            if let Some(tick) = self.chaser.on_frame(&frame) {
                self.current = Some(tick.timecode);
                self.last_frame_at = Some(Instant::now());
                self.tell_the_mtc_clock(tick.timecode);
                let fired = self.engine.update(tick.timecode, tick.reverse);
                self.deliver(fired, &mut events);
            }
        }

        // Nothing arrived: work out from the audio clock how many frames have
        // gone by, and let the chaser count through them if it is willing to.
        let elapsed = samples_seen.saturating_sub(self.last_good_sample);
        let due = (elapsed as f64 / self.samples_per_frame) as u64;

        if std::env::var_os("CHASEFIRE_DEBUG").is_some() {
            eprintln!(
                "poll: drained={} elapsed={} spf={:.0} due={} ticks={} signal={:?} current={:?}",
                drained,
                elapsed,
                self.samples_per_frame,
                due,
                self.freewheel_ticks,
                self.chaser.signal(),
                self.current
            );
        }
        while self.freewheel_ticks < due {
            self.freewheel_ticks += 1;
            match self.chaser.on_missing_frame() {
                Some(tick) => {
                    self.current = Some(tick.timecode);
                    self.tell_the_mtc_clock(tick.timecode);
                    let fired = self.engine.update(tick.timecode, tick.reverse);
                    self.deliver(fired, &mut events);
                }
                None => {
                    if self.current.is_some() {
                        events.push(Event::SignalLost);
                    }
                    // Stop the clock rather than let it free-run: a machine
                    // still receiving MTC believes the show is still going.
                    if let Some(clock) = &self.mtc {
                        clock.lost();
                    }
                    self.engine.signal_lost();
                    self.current = None;
                }
            }
        }

        events
    }

    /// Take a timecode position that did not come from the sound card.
    ///
    /// This is the door MTC will come in through — the brief was always "LTC or
    /// MTC", and a second source should not mean a second copy of the firing
    /// rules. Until then it is also how the delivery path is tested without a
    /// sound card in the machine, which is the only honest way to prove that a
    /// cue really did reach two different destinations.
    pub fn accept_timecode(&mut self, at: Timecode, source: Source) -> Vec<Event> {
        self.external_source = Some(source);
        self.current = Some(at);
        self.tell_the_mtc_clock(at);
        self.last_frame_at = Some(Instant::now());
        let mut events = Vec::new();
        let fired = self.engine.update(at, false);
        self.deliver(fired, &mut events);
        events
    }

    fn tell_the_mtc_clock(&self, at: Timecode) {
        if let Some(clock) = &self.mtc {
            clock.at(at, self.frame_rate().unwrap_or(25.0));
        }
    }

    /// Send everything a fired cue asked for.
    ///
    /// Every step is attempted even when an earlier one fails. A dead media
    /// server must not stop the same cue reaching the desk — half a cue is bad,
    /// and half a cue that could have been three quarters is worse.
    fn deliver(&mut self, fired: Vec<Firing>, events: &mut Vec<Event>) {
        for firing in fired {
            let mut failures = Vec::new();
            for step in &firing.steps {
                let step = match (&step.to, &self.default_osc) {
                    // A step that names nowhere goes to the default, so that a
                    // second output being added later cannot silently steal it.
                    (None, Some(default)) if step.send.carried_by() == cue::Carrier::Osc => {
                        cue::Step::to(default.clone(), step.send.clone())
                    }
                    _ => step.clone(),
                };
                if let Err(error) = self.outputs.deliver(&step) {
                    failures.push(error.to_string());
                }
            }
            let sent = if failures.is_empty() {
                Ok(())
            } else {
                Err(failures.join("; "))
            };
            events.push(Event::Fired { firing, sent });
        }
    }

    /// The flourish to draw for a cue that just went out.
    pub fn flourish_of(firing: &Firing) -> pablo::Flourish {
        flourish_for(&firing.steps)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use cue::{Message, OscArg};

    fn a_cue() -> Cue {
        Cue::new(
            1,
            "test",
            Timecode::new(10, 0, 1, 0),
            Message::Osc {
                address: "/go".into(),
                args: vec![OscArg::Int(1)],
            },
        )
    }

    #[test]
    fn with_nothing_open_it_reports_closed_rather_than_broken() {
        // A tool that has not been pointed at anything is not faulty, and must
        // not shout as though it were.
        let runner = Runner::new(25);
        assert_eq!(runner.health(), Health::Closed);
        assert_eq!(runner.health().describe(1), None);
    }

    #[test]
    fn each_way_of_failing_says_something_different_and_useful() {
        // The whole point: three failures that look identical on screen — no
        // timecode — have to be told apart in words somebody can act on.
        let delivering = Health::NotDelivering.describe(1).unwrap();
        let silent = Health::Silent.describe(7).unwrap();

        assert_ne!(delivering, silent);
        assert!(
            silent.contains('7'),
            "the silent-channel message has to name the channel: {silent}"
        );
        assert!(
            delivering.to_lowercase().contains("another program"),
            "the stalled-stream message should point at the likely cause: {delivering}"
        );
        // And neither should read like a stack trace.
        for message in [&delivering, &silent] {
            assert!(
                !message.contains("ALSA"),
                "backend jargon leaked: {message}"
            );
            assert!(!message.contains("Err"), "backend jargon leaked: {message}");
        }
    }

    #[test]
    fn with_no_input_it_sits_there_quietly() {
        // Before anyone opens a sound card, polling must be harmless and
        // Pablo must be asleep rather than pretending.
        let mut runner = Runner::new(25);
        runner.set_cues(vec![a_cue()]);
        assert!(runner.poll().is_empty());
        assert_eq!(runner.timecode(), None);
        assert_eq!(pablo::Mood::read(runner.situation()), pablo::Mood::Asleep);
    }

    #[test]
    fn arming_is_off_until_someone_says_otherwise() {
        // Nothing should ever go out because a window happened to open.
        let runner = Runner::new(25);
        assert!(!runner.is_armed());
        assert_eq!(pablo::Mood::read(runner.situation()), pablo::Mood::Asleep);
    }

    #[test]
    fn pinning_a_rate_settles_it_immediately() {
        let mut runner = Runner::new(25);
        runner.pin_frame_rate(Some(30.0));
        assert!(runner.settled, "a pinned rate needs no working out");
        runner.pin_frame_rate(None);
        assert!(!runner.settled, "left alone, it has to measure again");
    }

    #[test]
    fn the_cue_list_survives_being_set() {
        let mut runner = Runner::new(25);
        runner.set_cues(vec![a_cue()]);
        assert_eq!(runner.cues().len(), 1);
        assert_eq!(runner.pending_cues(), 1);
    }
}
