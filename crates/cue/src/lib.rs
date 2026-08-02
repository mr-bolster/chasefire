// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
//
// Copyright (c) Leo Bolster.

//! The cue table and the rules that decide when something fires.
//!
//! This crate holds no sockets and no MIDI ports: it turns a stream of incoming
//! timecode positions into a list of things that should happen. That is what
//! makes the rules below testable, and the rules are the whole product — anyone
//! can compare two numbers, but getting the *edge cases* right is the difference
//! between a tool an operator trusts and one they switch off after the first show.
//!
//! The rules, in plain words:
//!
//! * A cue fires when the timecode **crosses** it, not when it exactly equals
//!   it. LTC arrives dirty and frames go missing; demanding an exact match means
//!   a dropped frame silently eats a cue.
//! * A **big jump is a seek, not a crossing.** If someone drags the playhead
//!   from the top of the show to the encore, the cues in between must *not* all
//!   fire at once. That single rule is what stops a rehearsal from turning into
//!   a firework display.
//! * **Rewinding re-arms.** Go back before a cue and it will fire again, because
//!   that is what "let's take it from the top" means.
//! * **Nothing fires in reverse.** Rewinding past cues is not a performance.
//! * **Starting mid-show fires nothing.** Cues already in the past when the
//!   timecode locks are treated as passed, not pending.

use ltc::Timecode;
use serde::{Deserialize, Serialize};

/// One message going out. Data only — see the `sink` crate for the doing.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum Message {
    Osc {
        address: String,
        args: Vec<OscArg>,
    },
    MidiNote {
        channel: u8,
        note: u8,
        velocity: u8,
    },
    MidiProgramChange {
        channel: u8,
        program: u8,
        bank: Option<(u8, u8)>,
    },
    MidiControlChange {
        channel: u8,
        controller: u8,
        value: u8,
    },
    /// MIDI Show Control: a system exclusive message, and the only thing a
    /// grandMA2 or a ChamSys will take from us. Neither speaks OSC.
    ShowControl(ShowControl),
}

/// One MIDI Show Control command.
///
/// The shape is fixed by the standard: `F0 7F <device> 02 <format> <command>
/// <data> F7`. What varies between desks is which device id they are listening
/// on and which format they answer to, so both are settings rather than
/// constants — a console ignoring a message because it was addressed to device
/// 0 when it is listening on 3 is silent and baffling.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ShowControl {
    /// 0x00..=0x6F one desk, 0x70..=0x7E a group, 0x7F everybody.
    pub device: u8,
    /// 0x01 general lighting, 0x02 moving lights, 0x7F all types.
    pub format: u8,
    pub command: ShowCommand,
    /// The cue number **as written**, decimals and all: "21.5", not 21.5. The
    /// standard sends it as text, and 21.5 and 21.50 are different cues.
    pub cue: String,
    /// The cue list, when the desk needs telling which one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub list: Option<String>,
}

/// The commands worth having. The standard defines more; these are the ones
/// consoles actually implement and operators actually use.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ShowCommand {
    Go,
    Stop,
    Resume,
    GoOff,
}

impl ShowCommand {
    pub fn byte(self) -> u8 {
        match self {
            ShowCommand::Go => 0x01,
            ShowCommand::Stop => 0x02,
            ShowCommand::Resume => 0x03,
            ShowCommand::GoOff => 0x0B,
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            ShowCommand::Go => "GO",
            ShowCommand::Stop => "STOP",
            ShowCommand::Resume => "RESUME",
            ShowCommand::GoOff => "GO OFF",
        }
    }

    pub const ALL: [ShowCommand; 4] = [
        ShowCommand::Go,
        ShowCommand::Stop,
        ShowCommand::Resume,
        ShowCommand::GoOff,
    ];
}

impl Default for ShowControl {
    fn default() -> Self {
        Self {
            // Everybody, general lighting: the setting most likely to be heard
            // by a desk nobody has configured yet.
            device: 0x7F,
            format: 0x01,
            command: ShowCommand::Go,
            cue: "1".into(),
            list: None,
        }
    }
}

impl Message {
    /// Which kind of output can carry this. Used to pick a destination when the
    /// cue does not name one, and to refuse politely when it names the wrong one.
    pub fn carried_by(&self) -> Carrier {
        match self {
            Self::Osc { .. } => Carrier::Osc,
            _ => Carrier::Midi,
        }
    }
}

/// The sort of output a message needs.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Carrier {
    Osc,
    Midi,
}

/// An OSC argument.
///
/// All four types exist because the trade needs all four and getting them wrong
/// is not a cosmetic matter: QLab wants **no arguments at all** on
/// `/cue/5/start`, grandMA3 wants a whole command line as a **string**, and
/// Resolume wants an **int** to trigger but a **float** for opacity. A single
/// on/off integer is not a cue system, it is one media server's habit.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum OscArg {
    Int(i32),
    Float(f32),
    Str(String),
    Bool(bool),
}

/// One message and where it goes.
///
/// A cue is a **list** of these, sent in order, because a cue is a moment in a
/// show and not a wire. The Behringer Wing needs two messages to recall a scene
/// — the index, then the word GO — and a moment that lights the stage and rolls
/// the video is two messages to two different machines. Modelling a cue as one
/// message made both of those impossible to express.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Step {
    /// Which configured output this goes to. `None` means the default one for
    /// its carrier, which is what a show with a single destination wants and
    /// should never have to think about.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub to: Option<String>,
    pub send: Message,
}

impl Step {
    /// To wherever that kind of message normally goes.
    pub fn anywhere(send: Message) -> Self {
        Self { to: None, send }
    }

    /// To one named output in particular.
    pub fn to(name: impl Into<String>, send: Message) -> Self {
        Self {
            to: Some(name.into()),
            send,
        }
    }
}

/// One programmed cue: a moment in the show, and everything it sends.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Cue {
    pub id: u32,
    pub name: String,
    pub at: Timecode,
    pub enabled: bool,
    pub steps: Vec<Step>,
}

impl Cue {
    /// A cue that sends one thing, which is most of them.
    pub fn new(id: u32, name: impl Into<String>, at: Timecode, send: Message) -> Self {
        Self::of(id, name, at, vec![Step::anywhere(send)])
    }

    /// A cue that sends several, in order.
    pub fn of(id: u32, name: impl Into<String>, at: Timecode, steps: Vec<Step>) -> Self {
        Self {
            id,
            name: name.into(),
            at,
            enabled: true,
            steps,
        }
    }

    /// What sort of outputs this cue needs, in the order it needs them.
    pub fn carriers(&self) -> impl Iterator<Item = Carrier> + '_ {
        self.steps.iter().map(|step| step.send.carried_by())
    }
}

/// Read a cue, including one written by a version that only knew how to send a
/// single message.
///
/// Somebody's show file is not a schema to be tidied up. A cue list written on
/// the laptop this morning has to open this afternoon, so the old `action`
/// field is still understood and quietly becomes a one-step cue. Files are
/// *written* in the new shape, so the conversion happens once and only once.
impl<'de> Deserialize<'de> for Cue {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        fn yes() -> bool {
            true
        }

        #[derive(Deserialize)]
        struct OnDisk {
            id: u32,
            name: String,
            at: Timecode,
            #[serde(default = "yes")]
            enabled: bool,
            #[serde(default)]
            steps: Vec<Step>,
            /// What version 0.1 wrote.
            #[serde(default)]
            action: Option<Message>,
        }

        let disk = OnDisk::deserialize(deserializer)?;
        let steps = match (disk.steps.is_empty(), disk.action) {
            (true, Some(action)) => vec![Step::anywhere(action)],
            _ => disk.steps,
        };
        if steps.is_empty() {
            return Err(serde::de::Error::custom(format!(
                "cue {} ('{}') does not send anything",
                disk.id, disk.name
            )));
        }
        Ok(Self {
            id: disk.id,
            name: disk.name,
            at: disk.at,
            enabled: disk.enabled,
            steps,
        })
    }
}

/// A cue that just went off, handed to whoever is doing the sending.
#[derive(Debug, Clone, PartialEq)]
pub struct Firing {
    pub cue_id: u32,
    pub name: String,
    /// Everything this cue sends, in order.
    pub steps: Vec<Step>,
    /// The cue's programmed time, for the log.
    pub at: Timecode,
    /// Where the timecode actually was when it fired, for the log. On a clean
    /// signal these match; when they do not, the log says why.
    pub fired_at: Timecode,
}

/// Why nothing is currently being sent, when that is the case.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Idle {
    Disarmed,
    RunningBackwards,
}

pub struct Engine {
    nominal_fps: u8,
    offset_frames: i32,
    seek_threshold_frames: i64,
    armed: bool,
    cues: Vec<Cue>,
    /// Parallel to `cues`: whether each one is still waiting to go off.
    armed_states: Vec<bool>,
    last_position: Option<i64>,
}

impl Engine {
    pub fn new(nominal_fps: u8) -> Self {
        Self {
            nominal_fps,
            offset_frames: 0,
            // Two thirds of a second. Longer than any believable run of dropped
            // frames, shorter than any deliberate jump an operator would make.
            seek_threshold_frames: (nominal_fps as i64 * 2) / 3,
            armed: false,
            cues: Vec::new(),
            armed_states: Vec::new(),
            last_position: None,
        }
    }

    /// Positive values fire cues **earlier**, which is how you compensate for
    /// the latency of the sound card, the network and the receiving device.
    pub fn set_offset_frames(&mut self, offset_frames: i32) {
        self.offset_frames = offset_frames;
    }

    pub fn offset_frames(&self) -> i32 {
        self.offset_frames
    }

    /// The master switch. Disarmed means absolutely nothing leaves the machine —
    /// the state you want during soundcheck, and the state you want to reach in
    /// one keystroke when something goes wrong.
    pub fn set_armed(&mut self, armed: bool) {
        self.armed = armed;
    }

    pub fn is_armed(&self) -> bool {
        self.armed
    }

    /// Change the counting rate — after auto-detection, or when the operator
    /// picks a different one. Position is forgotten, so the next update
    /// re-syncs instead of firing everything between the two interpretations.
    pub fn set_nominal_fps(&mut self, nominal_fps: u8) {
        self.nominal_fps = nominal_fps;
        self.seek_threshold_frames = (nominal_fps as i64 * 2) / 3;
        self.last_position = None;
    }

    pub fn nominal_fps(&self) -> u8 {
        self.nominal_fps
    }

    pub fn set_cues(&mut self, cues: Vec<Cue>) {
        self.armed_states = vec![true; cues.len()];
        self.cues = cues;
        // Any notion of "where we were" belonged to the old cue list.
        self.last_position = None;
    }

    pub fn cues(&self) -> &[Cue] {
        &self.cues
    }

    /// Timecode has gone away. The next position that arrives re-syncs without
    /// firing everything between here and there.
    pub fn signal_lost(&mut self) {
        self.last_position = None;
    }

    /// Feed the current timecode. Returns whatever that made fire.
    pub fn update(&mut self, timecode: Timecode, reverse: bool) -> Vec<Firing> {
        let position = self.position_of(timecode) + self.offset_frames as i64;

        if reverse {
            // Rewinding re-arms whatever is now ahead of us, but sends nothing.
            self.rearm_after(position);
            self.last_position = Some(position);
            return Vec::new();
        }

        let previous = self.last_position.replace(position);

        let Some(previous) = previous else {
            // First lock: everything already behind us counts as passed.
            self.settle_at(position);
            return Vec::new();
        };

        let delta = position - previous;

        if delta.abs() > self.seek_threshold_frames {
            // A jump, not playback. Re-arm ahead, mark the past as gone, fire nothing.
            self.settle_at(position);
            return Vec::new();
        }

        if delta < 0 {
            // Small step backwards: jitter, or a slow crawl in rehearsal.
            self.rearm_after(position);
            return Vec::new();
        }

        let mut firings = Vec::new();
        for index in 0..self.cues.len() {
            let cue_position = self.position_of(self.cues[index].at);
            let crossed = cue_position > previous && cue_position <= position;
            if !crossed
                || !self.armed_states[index]
                || !self.cues[index].enabled
                || !self.cues[index].at.is_valid_at(self.nominal_fps)
            {
                continue;
            }
            // Disarm even when the master switch is off, so that flipping to
            // armed mid-show does not dump every cue we walked past.
            self.armed_states[index] = false;
            if self.armed {
                let cue = &self.cues[index];
                firings.push(Firing {
                    cue_id: cue.id,
                    name: cue.name.clone(),
                    steps: cue.steps.clone(),
                    at: cue.at,
                    fired_at: timecode,
                });
            }
        }
        firings
    }

    /// Why nothing would fire right now, if that is the case.
    pub fn idle_reason(&self) -> Option<Idle> {
        if !self.armed {
            return Some(Idle::Disarmed);
        }
        None
    }

    /// How many cues are still waiting to go off.
    pub fn pending_count(&self) -> usize {
        self.armed_states
            .iter()
            .zip(&self.cues)
            .filter(|(armed, cue)| **armed && cue.enabled && cue.at.is_valid_at(self.nominal_fps))
            .count()
    }

    /// The next cue due after `timecode`, for a countdown display.
    pub fn next_cue_after(&self, timecode: Timecode) -> Option<&Cue> {
        let now = self.position_of(timecode);
        self.cues
            .iter()
            .filter(|cue| {
                cue.enabled
                    && cue.at.is_valid_at(self.nominal_fps)
                    && self.position_of(cue.at) > now
            })
            .min_by_key(|cue| self.position_of(cue.at))
    }

    fn position_of(&self, timecode: Timecode) -> i64 {
        timecode.as_frame_count(self.nominal_fps as u32) as i64
    }

    /// Arm everything ahead of `position`, disarm everything behind it.
    fn settle_at(&mut self, position: i64) {
        for index in 0..self.cues.len() {
            self.armed_states[index] = self.cues[index].at.is_valid_at(self.nominal_fps)
                && self.position_of(self.cues[index].at) > position;
        }
    }

    /// Arm everything ahead of `position`, leaving the past alone.
    fn rearm_after(&mut self, position: i64) {
        for index in 0..self.cues.len() {
            if self.cues[index].at.is_valid_at(self.nominal_fps)
                && self.position_of(self.cues[index].at) > position
            {
                self.armed_states[index] = true;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const FPS: u8 = 25;

    fn osc_cue(id: u32, seconds: u8, frames: u8) -> Cue {
        Cue::new(
            id,
            format!("cue {id}"),
            Timecode::new(10, 0, seconds, frames),
            Message::Osc {
                address: format!("/cue/{id}"),
                args: vec![OscArg::Int(id as i32)],
            },
        )
    }

    /// Walk the timecode forward one frame at a time, collecting everything fired.
    fn play(engine: &mut Engine, from: Timecode, frames: u32) -> Vec<Firing> {
        let mut timecode = from;
        let mut fired = Vec::new();
        for _ in 0..frames {
            fired.extend(engine.update(timecode, false));
            timecode.advance_one_frame(FPS);
        }
        fired
    }

    fn armed_engine(cues: Vec<Cue>) -> Engine {
        let mut engine = Engine::new(FPS);
        engine.set_cues(cues);
        engine.set_armed(true);
        engine
    }

    #[test]
    fn fires_each_cue_once_as_the_show_runs() {
        let mut engine = armed_engine(vec![osc_cue(1, 2, 0), osc_cue(2, 4, 10)]);
        let fired = play(&mut engine, Timecode::new(10, 0, 0, 0), 200);

        let ids: Vec<u32> = fired.iter().map(|f| f.cue_id).collect();
        assert_eq!(ids, vec![1, 2], "each cue exactly once, in order");
    }

    #[test]
    fn a_big_jump_is_a_seek_and_fires_nothing() {
        // The one that matters: drag the playhead across the whole show and the
        // cues in between must stay put instead of all going off at once.
        let mut engine = armed_engine(vec![
            osc_cue(1, 2, 0),
            osc_cue(2, 4, 0),
            osc_cue(3, 6, 0),
            osc_cue(4, 8, 0),
        ]);

        engine.update(Timecode::new(10, 0, 0, 0), false);
        let fired = engine.update(Timecode::new(10, 0, 9, 0), false);

        assert!(fired.is_empty(), "a seek fired {} cues", fired.len());
        assert_eq!(engine.pending_count(), 0, "everything jumped over is spent");
    }

    #[test]
    fn dropped_frames_do_not_eat_a_cue() {
        // Three frames go missing right where the cue lives. It must still fire.
        let mut engine = armed_engine(vec![osc_cue(1, 1, 10)]);
        engine.update(Timecode::new(10, 0, 1, 8), false);
        let fired = engine.update(Timecode::new(10, 0, 1, 12), false);

        assert_eq!(fired.len(), 1, "cue lost to a dirty signal");
        assert_eq!(fired[0].fired_at, Timecode::new(10, 0, 1, 12));
    }

    #[test]
    fn rewinding_rearms_so_the_cue_fires_again() {
        let mut engine = armed_engine(vec![osc_cue(1, 2, 0)]);
        assert_eq!(play(&mut engine, Timecode::new(10, 0, 0, 0), 75).len(), 1);

        // Back to the top, and round again.
        engine.update(Timecode::new(10, 0, 0, 0), false);
        let second_pass = play(&mut engine, Timecode::new(10, 0, 0, 0), 75);
        assert_eq!(second_pass.len(), 1, "second pass did not fire");
    }

    #[test]
    fn starting_mid_show_does_not_fire_the_past() {
        let mut engine = armed_engine(vec![osc_cue(1, 1, 0), osc_cue(2, 2, 0), osc_cue(3, 9, 0)]);
        let fired = play(&mut engine, Timecode::new(10, 0, 5, 0), 25);

        assert!(fired.is_empty(), "fired {} cues from the past", fired.len());
        assert_eq!(
            engine.pending_count(),
            1,
            "only the later cue should remain"
        );
    }

    #[test]
    fn nothing_fires_while_running_backwards() {
        let mut engine = armed_engine(vec![osc_cue(1, 2, 0)]);
        let mut timecode = Timecode::new(10, 0, 4, 0);
        let mut fired = Vec::new();
        for _ in 0..100 {
            fired.extend(engine.update(timecode, true));
            // Crawl backwards a frame at a time.
            let position = timecode.as_frame_count(FPS as u32).saturating_sub(1);
            timecode = Timecode::new(
                10,
                0,
                ((position / FPS as u32) % 60) as u8,
                (position % FPS as u32) as u8,
            );
        }
        assert!(fired.is_empty(), "rewinding fired {} cues", fired.len());
    }

    #[test]
    fn disarmed_sends_nothing_but_still_keeps_its_place() {
        let mut engine = armed_engine(vec![osc_cue(1, 2, 0), osc_cue(2, 6, 0)]);
        engine.set_armed(false);

        let fired = play(&mut engine, Timecode::new(10, 0, 0, 0), 100);
        assert!(fired.is_empty());
        assert_eq!(engine.idle_reason(), Some(Idle::Disarmed));

        // Arming mid-show must not dump the cues we walked past while disarmed.
        engine.set_armed(true);
        let after = play(&mut engine, Timecode::new(10, 0, 4, 0), 25);
        assert!(after.is_empty(), "arming replayed {} old cues", after.len());
    }

    #[test]
    fn a_disabled_cue_never_fires() {
        let mut cue = osc_cue(1, 2, 0);
        cue.enabled = false;
        let mut engine = armed_engine(vec![cue]);
        assert!(play(&mut engine, Timecode::new(10, 0, 0, 0), 100).is_empty());
    }

    #[test]
    fn a_positive_offset_fires_early() {
        let mut engine = armed_engine(vec![osc_cue(1, 2, 0)]);
        engine.set_offset_frames(5);

        // With five frames of offset the cue must land five frames before its
        // programmed time — that is the whole point of latency compensation.
        // Start well before the crossing: the first frame after lock never
        // fires, by design, so a cue sitting exactly on it counts as passed.
        let fired = play(&mut engine, Timecode::new(10, 0, 1, 15), 15);
        assert_eq!(fired.len(), 1, "offset did not bring the cue forward");
        assert_eq!(
            fired[0].fired_at,
            Timecode::new(10, 0, 1, 20),
            "cue landed at the wrong moment for a five frame offset"
        );
        assert_eq!(
            fired[0].at,
            Timecode::new(10, 0, 2, 0),
            "programmed time changed"
        );
    }

    #[test]
    fn an_offset_crosses_a_drop_frame_minute_in_real_frames() {
        let cue = Cue::new(
            1,
            "drop boundary",
            Timecode::new(0, 1, 0, 2).with_drop_frame(true),
            Message::Osc {
                address: "/go".into(),
                args: Vec::new(),
            },
        );
        let mut engine = Engine::new(30);
        engine.set_cues(vec![cue]);
        engine.set_armed(true);
        engine.set_offset_frames(1);

        engine.update(Timecode::new(0, 0, 59, 28).with_drop_frame(true), false);
        let fired = engine.update(Timecode::new(0, 0, 59, 29).with_drop_frame(true), false);
        assert_eq!(fired.len(), 1);
        assert_eq!(
            fired[0].fired_at,
            Timecode::new(0, 0, 59, 29).with_drop_frame(true)
        );
    }

    #[test]
    fn a_cue_with_a_frame_that_does_not_exist_never_fires() {
        let mut engine = armed_engine(vec![osc_cue(1, 0, 50)]);
        let fired = play(&mut engine, Timecode::new(10, 0, 0, 0), 100);
        assert!(fired.is_empty());
        assert_eq!(engine.pending_count(), 0);
    }

    #[test]
    fn losing_the_signal_resyncs_without_firing() {
        let mut engine = armed_engine(vec![osc_cue(1, 3, 0), osc_cue(2, 4, 0)]);
        engine.update(Timecode::new(10, 0, 0, 0), false);

        engine.signal_lost();
        let fired = engine.update(Timecode::new(10, 0, 5, 0), false);

        assert!(fired.is_empty(), "resync fired {} cues", fired.len());
    }

    #[test]
    fn knows_which_cue_is_next() {
        let engine = armed_engine(vec![osc_cue(1, 2, 0), osc_cue(2, 8, 0)]);
        let next = engine.next_cue_after(Timecode::new(10, 0, 4, 0)).unwrap();
        assert_eq!(next.id, 2);
        assert!(engine.next_cue_after(Timecode::new(10, 0, 59, 0)).is_none());
    }
}
