// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
//
// Copyright (c) Leo Bolster.

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
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Sender, TryRecvError};
use std::sync::Arc;
use std::time::{Duration, Instant};

/// What the clock is told from outside.
enum Word {
    /// The show is here, at this rate.
    At(Timecode, Rate, bool),
    /// The timecode has gone. Stop sending rather than free-running: a machine
    /// that keeps receiving MTC believes the show is still going.
    Lost,
}

/// A running MTC generator. Dropping it stops the thread and the clock.
pub struct MtcClock {
    words: Sender<Word>,
    port: String,
    /// False once the port has refused to take anything. Interfaces get
    /// unplugged mid-show, and a clock that has quietly stopped while the
    /// window still says it is running is worse than one that never started.
    alive: Arc<AtomicBool>,
}

impl MtcClock {
    /// Open a port and start sending as soon as there is something to send.
    pub fn start(port: &str) -> Result<Self, String> {
        let mut sink = MidiSink::open(port)?;
        let name = sink.port().to_string();
        let (words, inbox) = mpsc::channel();
        let alive = Arc::new(AtomicBool::new(true));
        let mine = Arc::clone(&alive);

        std::thread::Builder::new()
            .name("chasefire-mtc".into())
            .spawn(move || {
                let mut position: Option<(Timecode, Rate, bool)> = None;
                let mut piece = 0u8;
                // The position the current sequence of eight is describing.
                let mut sequence_at: Option<Timecode> = None;
                let mut next_at = Instant::now();

                loop {
                    // Everything waiting, so a burst of updates does not put
                    // the clock behind.
                    loop {
                        match inbox.try_recv() {
                            Ok(Word::At(at, rate, reverse)) => {
                                let jumped = match position {
                                    None => true,
                                    Some((was, old_rate, old_reverse)) => {
                                        rate != old_rate
                                            || reverse != old_reverse
                                            || !follows(was, at, rate, reverse)
                                    }
                                };
                                position = Some((at, rate, reverse));
                                if jumped {
                                    // A whole position in one message. Eight
                                    // quarter-frames would take two frames to
                                    // say it, and after a seek the receiver
                                    // needs it now.
                                    note(&mine, sink.send_raw(&full_frame(at, rate)));
                                    piece = if reverse { 7 } else { 0 };
                                    sequence_at = Some(at);
                                    next_at = Instant::now();
                                }
                            }
                            Ok(Word::Lost) => position = None,
                            Err(TryRecvError::Empty) => break,
                            Err(TryRecvError::Disconnected) => return,
                        }
                    }

                    let Some((_, rate, reverse)) = position else {
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
                    // the seven pieces after it keep describing that same
                    // position, which is what a receiver expects.
                    if (!reverse && piece == 0) || (reverse && piece == 7) {
                        sequence_at = position.map(|(at, _, _)| at);
                    }
                    if let Some(at) = sequence_at {
                        note(&mine, sink.send_raw(&quarter_frame(piece, at, rate)));
                    }
                    piece = next_piece(piece, reverse);
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

        Ok(Self {
            words,
            port: name,
            alive,
        })
    }

    pub fn port(&self) -> &str {
        &self.port
    }

    /// Is the port still taking what it is given?
    pub fn is_alive(&self) -> bool {
        self.alive.load(Ordering::Relaxed)
    }

    /// Tell it where the show is. Cheap enough to call on every frame.
    pub fn at(&self, timecode: Timecode, fps: f64, reverse: bool) {
        let Some(rate) = Rate::nearest(fps, timecode.drop_frame) else {
            // MTC has no native 50/60 fps labels. Sending the original frame
            // number at a half-rate code aliases 40..59 onto earlier frames.
            let _ = self.words.send(Word::Lost);
            self.alive.store(false, Ordering::Relaxed);
            return;
        };
        let _ = self.words.send(Word::At(timecode, rate, reverse));
    }

    /// The timecode has gone.
    pub fn lost(&self) {
        let _ = self.words.send(Word::Lost);
    }
}

/// Remember whether the port is still there. Once it has failed it stays
/// failed: a port that came back would need reopening anyway, and a status
/// that flickers is one nobody believes.
fn note(alive: &AtomicBool, outcome: std::io::Result<()>) {
    if outcome.is_err() {
        alive.store(false, Ordering::Relaxed);
    }
}

/// Does `now` follow `was` by one frame, give or take? Anything else is a jump
/// and deserves a full frame rather than being crawled towards a nibble at a
/// time.
fn follows(was: Timecode, now: Timecode, rate: Rate, reverse: bool) -> bool {
    let fps = rate.fps().ceil() as u32;
    let gap = now.as_frame_count(fps) as i64 - was.as_frame_count(fps) as i64;
    if reverse {
        (-2..=0).contains(&gap)
    } else {
        (0..=2).contains(&gap)
    }
}

fn next_piece(piece: u8, reverse: bool) -> u8 {
    if reverse {
        (piece + 7) & 0x07
    } else {
        (piece + 1) & 0x07
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_drop_frame_minute_boundary_is_continuous() {
        let was = Timecode::new(10, 0, 59, 29).with_drop_frame(true);
        let now = Timecode::new(10, 1, 0, 2).with_drop_frame(true);
        assert!(follows(was, now, Rate::Fps2997Drop, false));
    }

    #[test]
    fn reverse_continuity_runs_towards_smaller_positions() {
        assert!(follows(
            Timecode::new(10, 0, 5, 10),
            Timecode::new(10, 0, 5, 9),
            Rate::Fps25,
            true
        ));
        assert!(!follows(
            Timecode::new(10, 0, 5, 10),
            Timecode::new(10, 0, 5, 9),
            Rate::Fps25,
            false
        ));
    }

    #[test]
    fn quarter_frame_piece_order_follows_the_direction() {
        let mut forward = 0;
        let mut forward_pieces = Vec::new();
        let mut reverse = 7;
        let mut reverse_pieces = Vec::new();
        for _ in 0..8 {
            forward_pieces.push(forward);
            forward = next_piece(forward, false);
            reverse_pieces.push(reverse);
            reverse = next_piece(reverse, true);
        }
        assert_eq!(forward_pieces, [0, 1, 2, 3, 4, 5, 6, 7]);
        assert_eq!(reverse_pieces, [7, 6, 5, 4, 3, 2, 1, 0]);
    }
}
