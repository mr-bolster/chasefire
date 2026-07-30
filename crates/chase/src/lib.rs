//! Turning a stream of decoded frames into a position you can fire cues from.
//!
//! The decoder answers "what does this frame say". This crate answers the
//! harder question: **should we believe it**. They are not the same, and the
//! gap between them is where shows go wrong.
//!
//! LTC has no checksum. A frame with a few corrupted bits and an intact sync
//! word decodes into a perfectly reasonable-looking wrong time — and the
//! reference implementation, libltc, hands it straight to the application
//! without a word. Measured on a real analogue loop (sound card out, cable,
//! mic preamp, converter in), that starts happening once the signal-to-noise
//! ratio drops to about 12 dB: eleven bad frames in a hundred and thirty.
//!
//! One bad frame is not a cosmetic problem, though not in the way you would
//! first guess. A wildly wrong value reads as a seek and heals itself, because
//! the next good frame reads as a seek back. The damage is done by a value
//! that is wrong by only a little: it stays inside the seek threshold, so it
//! looks like ordinary playback, fires the cue it appears to have passed, and
//! then the step backwards re-arms that cue so it fires *again* when the
//! timecode really arrives. One corrupted frame, two triggers, and nothing in
//! the cue list afterwards to explain it. There is a test that builds exactly
//! that situation, in `tests/protects_the_cue_engine.rs`.
//!
//! So this layer applies, in order:
//!
//! 1. **Parity**, but only against sources that have shown they maintain it.
//! 2. **Continuity**: a frame should follow the one before. One that does not
//!    is held back until a second frame confirms the new position. A real seek
//!    arrives with company; a corrupted frame arrives alone.
//! 3. **Freewheel**: when the signal drops out, keep counting for a while
//!    rather than stopping dead. The professional norm is eight to forty
//!    frames — Pro Tools allows up to 120, and its hardware synchronisers cap
//!    at 40 — so eight is the default here and it is adjustable.

use ltc::{DecodedFrame, Timecode};

/// What the chaser thinks of the incoming signal right now.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Signal {
    /// Nothing believable yet.
    Searching,
    /// Frames are arriving and making sense.
    Locked,
    /// Nothing is arriving, but we are still counting on our own.
    Freewheeling { frames: u32 },
    /// Gone long enough that guessing would be dishonest.
    Lost,
}

/// Where a position came from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Source {
    /// Straight off the wire.
    Decoded,
    /// Counted internally because the signal dropped out.
    Freewheeled,
}

/// A position the cue engine can act on.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Tick {
    pub timecode: Timecode,
    pub reverse: bool,
    pub source: Source,
}

/// Everything thrown away and why. Worth showing an operator: a rising reject
/// count is the earliest warning that a cable is going bad, well before
/// anything actually misfires.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Rejections {
    pub accepted: u64,
    pub freewheeled: u64,
    pub failed_parity: u64,
    pub broke_continuity: u64,
    pub seeks: u64,
    pub dropouts: u64,
}

pub struct Chaser {
    nominal_fps: u8,
    freewheel_frames: u32,
    trust_parity: bool,
    signal: Signal,
    last_accepted: Option<Timecode>,
    /// A position we have seen once and do not yet believe.
    pending: Option<Timecode>,
    frames_without_signal: u32,
    rejections: Rejections,
}

impl Chaser {
    pub fn new(nominal_fps: u8) -> Self {
        Self {
            nominal_fps,
            // Eight frames: a third of a second at 25 fps. Long enough to ride
            // out the dropouts a dirty cable produces, short enough that a
            // genuine stop is noticed almost immediately.
            freewheel_frames: 8,
            trust_parity: false,
            signal: Signal::Searching,
            last_accepted: None,
            pending: None,
            frames_without_signal: 0,
            rejections: Rejections::default(),
        }
    }

    pub fn set_freewheel_frames(&mut self, frames: u32) {
        self.freewheel_frames = frames;
    }

    pub fn freewheel_frames(&self) -> u32 {
        self.freewheel_frames
    }

    /// Enable parity rejection. Only call this once the decoder has reported
    /// that the source actually maintains the bit — plenty of gear does not,
    /// and rejecting on parity against such a source throws away every frame.
    pub fn set_trust_parity(&mut self, trust: bool) {
        self.trust_parity = trust;
    }

    pub fn set_nominal_fps(&mut self, nominal_fps: u8) {
        self.nominal_fps = nominal_fps;
        self.reset();
    }

    pub fn signal(&self) -> Signal {
        self.signal
    }

    pub fn rejections(&self) -> Rejections {
        self.rejections
    }

    pub fn reset(&mut self) {
        self.signal = Signal::Searching;
        self.last_accepted = None;
        self.pending = None;
        self.frames_without_signal = 0;
    }

    /// Feed a frame the decoder produced. Returns a position only if it is
    /// believable — which is the entire point of this crate.
    pub fn on_frame(&mut self, frame: &DecodedFrame) -> Option<Tick> {
        if self.trust_parity && !frame.parity_ok {
            self.rejections.failed_parity += 1;
            return None;
        }

        self.frames_without_signal = 0;

        // Running backwards has no continuity to check in the forward sense,
        // and nothing fires in reverse anyway. Pass it through and note where
        // we are, so the first forward frame afterwards is not read as a jump.
        if frame.reverse {
            self.last_accepted = Some(frame.timecode);
            self.pending = None;
            self.signal = Signal::Locked;
            self.rejections.accepted += 1;
            return Some(Tick {
                timecode: frame.timecode,
                reverse: true,
                source: Source::Decoded,
            });
        }

        let Some(last) = self.last_accepted else {
            // Nothing to be continuous with yet.
            return Some(self.accept(frame.timecode));
        };

        let mut expected = last;
        expected.advance_one_frame(self.nominal_fps);

        if frame.timecode == expected {
            self.pending = None;
            return Some(self.accept(frame.timecode));
        }

        // Not where we expected. Either the operator jumped, or this frame is
        // damaged. Both look identical for exactly one frame, so wait.
        match self.pending.take() {
            Some(candidate) => {
                let mut confirmation = candidate;
                confirmation.advance_one_frame(self.nominal_fps);
                if frame.timecode == confirmation {
                    // Two in a row agreeing: a real move. Take it.
                    self.rejections.seeks += 1;
                    self.last_accepted = Some(candidate);
                    return Some(self.accept(frame.timecode));
                }
                // Two disagreeing oddities in a row. Believe neither, but keep
                // the newer one as the candidate — a jump landing mid-flight
                // still converges on the next frame.
                self.rejections.broke_continuity += 1;
                self.pending = Some(frame.timecode);
                None
            }
            None => {
                self.rejections.broke_continuity += 1;
                self.pending = Some(frame.timecode);
                None
            }
        }
    }

    /// Call once per frame period when no frame arrived.
    ///
    /// Returns a counted-on position while inside the freewheel window, then
    /// nothing once the signal has been gone too long to keep pretending.
    pub fn on_missing_frame(&mut self) -> Option<Tick> {
        match self.signal {
            Signal::Searching | Signal::Lost => None,
            Signal::Locked | Signal::Freewheeling { .. } => {
                self.frames_without_signal += 1;
                if self.frames_without_signal > self.freewheel_frames {
                    if self.signal != Signal::Lost {
                        self.rejections.dropouts += 1;
                    }
                    self.signal = Signal::Lost;
                    self.last_accepted = None;
                    self.pending = None;
                    return None;
                }

                let mut timecode = self.last_accepted?;
                timecode.advance_one_frame(self.nominal_fps);
                self.last_accepted = Some(timecode);
                self.signal = Signal::Freewheeling {
                    frames: self.frames_without_signal,
                };
                self.rejections.freewheeled += 1;
                Some(Tick {
                    timecode,
                    reverse: false,
                    source: Source::Freewheeled,
                })
            }
        }
    }

    fn accept(&mut self, timecode: Timecode) -> Tick {
        self.last_accepted = Some(timecode);
        self.signal = Signal::Locked;
        self.rejections.accepted += 1;
        Tick {
            timecode,
            reverse: false,
            source: Source::Decoded,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const FPS: u8 = 25;

    fn frame(timecode: Timecode) -> DecodedFrame {
        DecodedFrame {
            timecode,
            reverse: false,
            user_bits: 0,
            estimated_fps: FPS as f32,
            parity_ok: true,
            end_sample: 0,
        }
    }

    /// Feed a run of clean consecutive frames.
    fn run(chaser: &mut Chaser, from: Timecode, count: u32) -> Vec<Tick> {
        let mut timecode = from;
        let mut ticks = Vec::new();
        for _ in 0..count {
            if let Some(tick) = chaser.on_frame(&frame(timecode)) {
                ticks.push(tick);
            }
            timecode.advance_one_frame(FPS);
        }
        ticks
    }

    #[test]
    fn clean_timecode_passes_straight_through() {
        let mut chaser = Chaser::new(FPS);
        let ticks = run(&mut chaser, Timecode::new(10, 0, 0, 0), 50);
        assert_eq!(ticks.len(), 50);
        assert!(ticks.iter().all(|tick| tick.source == Source::Decoded));
        assert_eq!(chaser.signal(), Signal::Locked);
        assert_eq!(chaser.rejections().broke_continuity, 0);
    }

    #[test]
    fn a_lone_corrupt_frame_is_swallowed() {
        // The whole reason this crate exists. One frame decodes to nonsense in
        // the middle of a clean run; it must not reach the cue engine.
        let mut chaser = Chaser::new(FPS);
        run(&mut chaser, Timecode::new(10, 0, 0, 0), 10);

        let garbage = chaser.on_frame(&frame(Timecode::new(3, 47, 12, 9)));
        assert!(garbage.is_none(), "garbage frame reached the engine");

        // And the good frame that follows carries on as if nothing happened.
        let resumed = chaser.on_frame(&frame(Timecode::new(10, 0, 0, 10)));
        assert_eq!(resumed.unwrap().timecode, Timecode::new(10, 0, 0, 10));
        assert_eq!(chaser.rejections().broke_continuity, 1);
        assert_eq!(chaser.rejections().seeks, 0);
    }

    #[test]
    fn a_real_seek_is_believed_once_a_second_frame_agrees() {
        let mut chaser = Chaser::new(FPS);
        run(&mut chaser, Timecode::new(10, 0, 0, 0), 10);

        // The operator drags the playhead. First frame at the new position is
        // held back...
        assert!(chaser
            .on_frame(&frame(Timecode::new(10, 5, 0, 0)))
            .is_none());
        // ...and the next one confirms it.
        let confirmed = chaser.on_frame(&frame(Timecode::new(10, 5, 0, 1)));
        assert_eq!(confirmed.unwrap().timecode, Timecode::new(10, 5, 0, 1));
        assert_eq!(chaser.rejections().seeks, 1);

        // From there it runs on normally.
        let after = run(&mut chaser, Timecode::new(10, 5, 0, 2), 10);
        assert_eq!(after.len(), 10);
    }

    #[test]
    fn a_burst_of_garbage_never_gets_believed() {
        // Random wrong values, none following any other: nothing should escape.
        let mut chaser = Chaser::new(FPS);
        run(&mut chaser, Timecode::new(10, 0, 0, 0), 10);

        let rubbish = [
            Timecode::new(1, 2, 3, 4),
            Timecode::new(23, 59, 59, 24),
            Timecode::new(7, 7, 7, 7),
            Timecode::new(0, 0, 0, 1),
            Timecode::new(15, 30, 45, 12),
        ];
        for timecode in rubbish {
            assert!(
                chaser.on_frame(&frame(timecode)).is_none(),
                "{timecode} escaped"
            );
        }
        assert_eq!(chaser.rejections().accepted, 10, "only the clean run");
    }

    #[test]
    fn freewheels_through_a_dropout_and_then_gives_up() {
        let mut chaser = Chaser::new(FPS);
        run(&mut chaser, Timecode::new(10, 0, 1, 0), 10);
        let last = Timecode::new(10, 0, 1, 9);

        // Inside the window it keeps counting, so cues still land.
        let mut counted = Vec::new();
        for _ in 0..8 {
            counted.push(chaser.on_missing_frame().expect("stopped counting early"));
        }
        assert_eq!(counted.len(), 8);
        assert!(counted
            .iter()
            .all(|tick| tick.source == Source::Freewheeled));
        assert_eq!(counted.last().unwrap().timecode, {
            let mut expected = last;
            for _ in 0..8 {
                expected.advance_one_frame(FPS);
            }
            expected
        });

        // Past it, honesty: stop guessing.
        assert!(chaser.on_missing_frame().is_none());
        assert_eq!(chaser.signal(), Signal::Lost);
        assert_eq!(chaser.rejections().dropouts, 1);
    }

    #[test]
    fn a_dropout_shorter_than_the_window_is_invisible_downstream() {
        let mut chaser = Chaser::new(FPS);
        run(&mut chaser, Timecode::new(10, 0, 1, 0), 10);

        for _ in 0..3 {
            assert!(chaser.on_missing_frame().is_some());
        }
        // Signal returns exactly where the freewheel had counted to.
        let resumed = chaser.on_frame(&frame(Timecode::new(10, 0, 1, 13)));
        assert!(resumed.is_some(), "clean resume was treated as a jump");
        assert_eq!(chaser.signal(), Signal::Locked);
    }

    #[test]
    fn after_giving_up_it_relocks_without_pretending_to_be_continuous() {
        let mut chaser = Chaser::new(FPS);
        run(&mut chaser, Timecode::new(10, 0, 0, 0), 10);
        for _ in 0..20 {
            chaser.on_missing_frame();
        }
        assert_eq!(chaser.signal(), Signal::Lost);

        // Timecode comes back somewhere else entirely, as it would after a
        // break. First frame is accepted as a fresh lock, not held as a jump.
        let relocked = chaser.on_frame(&frame(Timecode::new(11, 0, 0, 0)));
        assert!(relocked.is_some());
        assert_eq!(chaser.signal(), Signal::Locked);
    }

    #[test]
    fn parity_is_only_enforced_when_asked() {
        let mut bad = frame(Timecode::new(10, 0, 0, 0));
        bad.parity_ok = false;

        // Off by default: plenty of sources never set the bit, and rejecting
        // on it blindly would throw away every frame they send.
        let mut lenient = Chaser::new(FPS);
        assert!(lenient.on_frame(&bad).is_some());

        let mut strict = Chaser::new(FPS);
        strict.set_trust_parity(true);
        assert!(strict.on_frame(&bad).is_none());
        assert_eq!(strict.rejections().failed_parity, 1);
    }

    #[test]
    fn drop_frame_minute_boundaries_are_not_mistaken_for_a_glitch() {
        // At 29.97 the count jumps from 59;29 to 00;02. A continuity check that
        // did not know that would reject a perfectly good frame every minute.
        let mut chaser = Chaser::new(30);
        let start = Timecode::new(10, 0, 59, 25).with_drop_frame(true);

        let mut timecode = start;
        let mut accepted = 0;
        for _ in 0..10 {
            if chaser.on_frame(&frame(timecode)).is_some() {
                accepted += 1;
            }
            timecode.advance_one_frame(30);
        }
        assert_eq!(accepted, 10, "the minute boundary was treated as a jump");
        assert_eq!(chaser.rejections().broke_continuity, 0);
    }
}
