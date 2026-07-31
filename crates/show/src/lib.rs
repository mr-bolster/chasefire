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
use sink::{OscSink, Sink};
use std::time::Instant;

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
fn flourish_for(action: &cue::Action) -> pablo::Flourish {
    match action {
        cue::Action::Osc { .. } => pablo::Flourish::Osc,
        _ => pablo::Flourish::Midi,
    }
}

pub struct Runner {
    capture: Option<audio::Capture>,
    chaser: Chaser,
    engine: Engine,
    output: Option<OscSink>,
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
    error: Option<String>,
}

impl Runner {
    pub fn new(nominal_fps: u8) -> Self {
        Self {
            capture: None,
            chaser: Chaser::new(nominal_fps),
            engine: Engine::new(nominal_fps),
            output: None,
            pinned_fps: None,
            settled: false,
            samples_per_frame: 48_000.0 / nominal_fps as f64,
            last_good_sample: 0,
            freewheel_ticks: 0,
            current: None,
            last_frame_at: None,
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
        let capture = audio::Capture::open(device, channel, self.pinned_fps)?;
        self.samples_per_frame = capture.sample_rate() as f64 / self.pinned_fps.unwrap_or(25.0);
        self.last_good_sample = capture.samples_processed();
        self.capture = Some(capture);
        self.error = None;
        Ok(())
    }

    pub fn close_input(&mut self) {
        self.capture = None;
        self.chaser.reset();
        self.current = None;
    }

    pub fn connect_osc(&mut self, target: &str) -> Result<(), String> {
        let sink = OscSink::connect(target).map_err(|error| error.to_string())?;
        self.output = Some(sink);
        Ok(())
    }

    /// Where cues are being sent, for the window to show. Somebody staring at
    /// a corner of a screen for six hours should not have to remember which
    /// machine they pointed this at.
    pub fn output_target(&self) -> Option<String> {
        self.output.as_ref().map(|sink| sink.target().to_string())
    }

    /// What kind of timecode this is chasing, if anything.
    pub fn source(&self) -> Option<Source> {
        // An audio input can only ever be carrying LTC. When a MIDI input is
        // possible this stops being a foregone conclusion.
        self.capture.as_ref().map(|_| Source::Ltc)
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
                let fired = self.engine.update(tick.timecode, tick.reverse);
                self.deliver(fired, &mut events);
            }
        }

        // Nothing arrived: work out from the audio clock how many frames have
        // gone by, and let the chaser count through them if it is willing to.
        let elapsed = samples_seen.saturating_sub(self.last_good_sample);
        let due = (elapsed as f64 / self.samples_per_frame) as u64;
        while self.freewheel_ticks < due {
            self.freewheel_ticks += 1;
            match self.chaser.on_missing_frame() {
                Some(tick) => {
                    self.current = Some(tick.timecode);
                    let fired = self.engine.update(tick.timecode, tick.reverse);
                    self.deliver(fired, &mut events);
                }
                None => {
                    if self.current.is_some() {
                        events.push(Event::SignalLost);
                    }
                    self.engine.signal_lost();
                    self.current = None;
                }
            }
        }

        events
    }

    fn deliver(&mut self, fired: Vec<Firing>, events: &mut Vec<Event>) {
        for firing in fired {
            let sent = match &mut self.output {
                Some(sink) => sink
                    .deliver(&firing.action)
                    .map_err(|error| error.to_string()),
                None => Err("no output connected".to_string()),
            };
            events.push(Event::Fired { firing, sent });
        }
    }

    /// The flourish to draw for a cue that just went out.
    pub fn flourish_of(firing: &Firing) -> pablo::Flourish {
        flourish_for(&firing.action)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use cue::{Action, OscArg};

    fn a_cue() -> Cue {
        Cue::new(
            1,
            "test",
            Timecode::new(10, 0, 1, 0),
            Action::Osc {
                address: "/go".into(),
                args: vec![OscArg::Int(1)],
            },
        )
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
