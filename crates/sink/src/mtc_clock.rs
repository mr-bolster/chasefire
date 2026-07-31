//! Sending MTC at the right moment, which is most of what makes it useful.
//!
//! A receiver locks to MTC by **when the quarter-frames arrive**, not by what
//! is in them. Sending all eight in a burst once every two frames carries the
//! same numbers and clocks nothing: the receiver sees the position jump and
//! stand still, jump and stand still. So this runs on its own thread with its
//! own timer, sending one message every quarter of a frame, and the rest of the
//! program only ever tells it where the show is.
//!
//! Two frames of latency are built into the format and cannot be removed: the
//! position a sequence carries is where the show was when the sequence began.
//! Receivers add it back. This is why an MTC-locked machine is always a frame
//! or two behind an LTC-locked one, and it is worth knowing before somebody
//! spends an evening looking for it.

use crate::midi::MidiSink;
use crate::mtc::{full_frame, quarter_frame, Rate};
use ltc::Timecode;
use std::sync::mpsc::{self, Sender, TryRecvError};
use std::time::{Duration, Instant};

/// What the clock is told from outside.
enum Word {
    /// The show is here, at this rate.
    At(Timecode, Rate),
    /// The timecode has gone. Stop sending rather than free-running: a machine
    /// that keeps receiving MTC believes the show is still going.
    Lost,
}

/// A running MTC generator. Dropping it stops the thread and the clock.
pub struct MtcClock {
    words: Sender<Word>,
    port: String,
}

impl MtcClock {
    /// Open a port and start sending as soon as there is something to send.
    pub fn start(port: &str) -> Result<Self, String> {
        let mut sink = MidiSink::open(port)?;
        let name = sink.port().to_string();
        let (words, inbox) = mpsc::channel();

        std::thread::Builder::new()
            .name("chasefire-mtc".into())
            .spawn(move || {
                let mut position: Option<(Timecode, Rate)> = None;
                let mut piece = 0u8;
                // The position the current sequence of eight is describing.
                let mut sequence_at: Option<Timecode> = None;
                let mut next_at = Instant::now();

                loop {
                    // Everything waiting, so a burst of updates does not put
                    // the clock behind.
                    loop {
                        match inbox.try_recv() {
                            Ok(Word::At(at, rate)) => {
                                let jumped = match position {
                                    None => true,
                                    Some((was, _)) => !follows(was, at, rate),
                                };
                                position = Some((at, rate));
                                if jumped {
                                    // A whole position in one message. Eight
                                    // quarter-frames would take two frames to
                                    // say it, and after a seek the receiver
                                    // needs it now.
                                    let _ = sink.send_raw(&full_frame(at, rate));
                                    piece = 0;
                                    sequence_at = Some(at);
                                    next_at = Instant::now();
                                }
                            }
                            Ok(Word::Lost) => position = None,
                            Err(TryRecvError::Empty) => break,
                            Err(TryRecvError::Disconnected) => return,
                        }
                    }

                    let Some((_, rate)) = position else {
                        // Nothing to say. Idle politely rather than spinning.
                        std::thread::sleep(Duration::from_millis(20));
                        next_at = Instant::now();
                        continue;
                    };

                    let quarter = Duration::from_secs_f64(1.0 / (rate.fps() * 4.0));
                    let now = Instant::now();
                    if now < next_at {
                        std::thread::sleep((next_at - now).min(Duration::from_millis(5)));
                        continue;
                    }

                    // A new sequence starts at whatever the position is now;
                    // the four pieces after it keep describing that same
                    // position, which is what a receiver expects.
                    if piece == 0 {
                        sequence_at = position.map(|(at, _)| at);
                    }
                    if let Some(at) = sequence_at {
                        let _ = sink.send_raw(&quarter_frame(piece, at, rate));
                    }
                    piece = (piece + 1) & 0x07;
                    next_at += quarter;
                    // If we fell a long way behind — the machine was busy, or
                    // the show was paused — start again from now rather than
                    // firing a backlog of quarter-frames all at once.
                    if next_at + quarter * 8 < Instant::now() {
                        next_at = Instant::now();
                    }
                }
            })
            .map_err(|error| error.to_string())?;

        Ok(Self { words, port: name })
    }

    pub fn port(&self) -> &str {
        &self.port
    }

    /// Tell it where the show is. Cheap enough to call on every frame.
    pub fn at(&self, timecode: Timecode, fps: f64) {
        let rate = Rate::nearest(fps, timecode.drop_frame);
        let _ = self.words.send(Word::At(timecode, rate));
    }

    /// The timecode has gone.
    pub fn lost(&self) {
        let _ = self.words.send(Word::Lost);
    }
}

/// Does `now` follow `was` by one frame, give or take? Anything else is a jump
/// and deserves a full frame rather than being crawled towards a nibble at a
/// time.
fn follows(was: Timecode, now: Timecode, rate: Rate) -> bool {
    let fps = rate.fps().round() as i64;
    let frames = |at: Timecode| {
        ((at.hours as i64 * 3600) + (at.minutes as i64 * 60) + at.seconds as i64) * fps
            + at.frames as i64
    };
    let gap = frames(now) - frames(was);
    (0..=2).contains(&gap)
}
