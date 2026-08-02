// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
//
// Copyright (c) Leo Bolster.

//! Reading MIDI Time Code, which is a stranger business than writing it.
//!
//! MTC does not send a position. It sends **one nibble at a time**, eight of
//! them, spread over two frames — and by the time the eighth arrives the show
//! has moved on by two. So a reader has to collect a whole sequence, and then
//! add back the two frames that went by while it was collecting.
//!
//! Two things follow from that, and both matter on a stage:
//!
//! * **A position only exists every two frames.** Anything wanting a per-frame
//!   answer has to count in between, which is exactly what the chaser already
//!   does when LTC drops out.
//! * **Joining mid-show is not instant.** Until a full sequence has gone by
//!   there is nothing to report. The full-frame message exists for that: it
//!   carries a whole position in one go, and is what a sensible sender emits
//!   after a locate.
//!
//! Running backwards is real and has to be handled: rewind a DAW and the
//! quarter-frames arrive in *descending* piece order, seven down to zero.

use crate::mtc::Rate;
use ltc::Timecode;

/// What a reader has worked out so far.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Heard {
    /// A whole position, the rate it came with, and whether it is rewinding.
    At(Timecode, Rate, bool),
    /// Something arrived and was understood, but there is not a position yet.
    Building,
    /// Not MTC, or not anything we can use.
    Ignored,
}

/// Collects quarter-frames into positions.
#[derive(Debug, Default)]
pub struct Reader {
    /// The eight nibbles, and whether each has been seen this time round.
    nibbles: [u8; 8],
    seen: [bool; 8],
    /// Which piece is expected next, so a jump in the sequence can be spotted.
    expecting: Option<u8>,
    /// True when the pieces are arriving backwards, which means the show is.
    reversing: bool,
}

impl Reader {
    pub fn new() -> Self {
        Self::default()
    }

    /// Feed one MIDI message. Anything that is not timecode is ignored rather
    /// than being an error: a MIDI port carries other traffic and none of it is
    /// our business.
    pub fn take(&mut self, message: &[u8]) -> Heard {
        match message.first() {
            Some(0xF1) if message.len() >= 2 => self.quarter_frame(message[1]),
            Some(0xF0) => self.full_frame(message),
            _ => Heard::Ignored,
        }
    }

    fn quarter_frame(&mut self, data: u8) -> Heard {
        let piece = (data >> 4) & 0x07;
        let value = data & 0x0F;

        // Which way the show is going. Pieces arriving in descending order is a
        // rewind, and a rewind must not be read as a wild jump forwards.
        if let Some(expected) = self.expecting {
            let forwards = piece == (expected) % 8;
            let backwards = piece == (expected + 6) % 8;
            if backwards && !forwards {
                self.reversing = true;
            } else if forwards {
                self.reversing = false;
            } else {
                // Neither: the sequence was interrupted. Start again rather
                // than assemble a position out of two different moments.
                self.seen = [false; 8];
            }
        }

        self.nibbles[piece as usize] = value;
        self.seen[piece as usize] = true;
        self.expecting = Some(if self.reversing {
            (piece + 7) % 8
        } else {
            (piece + 1) % 8
        });

        // A position is complete when the last piece of the run arrives — piece
        // 7 going forwards, piece 0 going backwards — and every nibble is in.
        let last = if self.reversing { 0 } else { 7 };
        if piece != last || !self.seen.iter().all(|had| *had) {
            return Heard::Building;
        }
        self.seen = [false; 8];

        let frames = self.nibbles[0] | (self.nibbles[1] << 4);
        let seconds = self.nibbles[2] | (self.nibbles[3] << 4);
        let minutes = self.nibbles[4] | (self.nibbles[5] << 4);
        let hours = self.nibbles[6] | ((self.nibbles[7] & 0x01) << 4);
        let rate = rate_of((self.nibbles[7] >> 1) & 0x03);

        let at = Timecode {
            hours,
            minutes,
            seconds,
            frames,
            drop_frame: rate == Rate::Fps2997Drop,
        };
        if !at.is_plausible() {
            return Heard::Ignored;
        }

        // The two frames that went by while the sequence was being spelled out.
        // Leaving them off puts everything downstream two frames behind, which
        // at 25 fps is 80 ms — the difference between a cue landing on the hit
        // and landing after it.
        Heard::At(moved_by_two(at, rate, self.reversing), rate, self.reversing)
    }

    fn full_frame(&mut self, message: &[u8]) -> Heard {
        // F0 7F <device> 01 01 hh mm ss ff F7
        if message.len() < 10 || message[1] != 0x7F || message[3] != 0x01 || message[4] != 0x01 {
            return Heard::Ignored;
        }
        let rate = rate_of((message[5] >> 5) & 0x03);
        let at = Timecode {
            hours: message[5] & 0x1F,
            minutes: message[6],
            seconds: message[7],
            frames: message[8],
            drop_frame: rate == Rate::Fps2997Drop,
        };
        if !at.is_plausible() {
            return Heard::Ignored;
        }
        // A whole position in one message: nothing was spelled out, so nothing
        // has to be added back. Start the sequence over from here.
        self.seen = [false; 8];
        self.expecting = None;
        self.reversing = false;
        Heard::At(at, rate, false)
    }
}

fn rate_of(code: u8) -> Rate {
    match code {
        0 => Rate::Fps24,
        1 => Rate::Fps25,
        2 => Rate::Fps2997Drop,
        _ => Rate::Fps30,
    }
}

/// Move a position by the two frames spent spelling it out.
///
/// Drop frame is the whole difficulty. At 29.97 the numbers **00 and 01 do not
/// exist** at the top of every minute except every tenth — they are skipped so
/// that the clock keeps up with real time. Counting straight through produces
/// a timecode that never happens, and a cue programmed a frame away from a
/// minute boundary then never lines up with it.
fn moved_by_two(at: Timecode, rate: Rate, reverse: bool) -> Timecode {
    let mut moved = at;
    for _ in 0..2 {
        if reverse {
            moved.retreat_one_frame(rate.fps().ceil() as u8);
        } else {
            moved = next_frame(moved, rate);
        }
    }
    moved
}

fn next_frame(at: Timecode, rate: Rate) -> Timecode {
    let fps = rate.fps().ceil() as u8;
    let mut frames = at.frames + 1;
    let mut seconds = at.seconds;
    let mut minutes = at.minutes;
    let mut hours = at.hours;

    if frames >= fps {
        frames = 0;
        seconds += 1;
        if seconds >= 60 {
            seconds = 0;
            minutes += 1;
            if minutes >= 60 {
                minutes = 0;
                hours = (hours + 1) % 24;
            }
            // The skip: at the top of every minute but every tenth, the first
            // two frame numbers are not used.
            if rate == Rate::Fps2997Drop && minutes % 10 != 0 {
                frames = 2;
            }
        }
    }

    Timecode {
        hours,
        minutes,
        seconds,
        frames,
        drop_frame: at.drop_frame,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mtc::quarter_frame;

    fn at(hours: u8, minutes: u8, seconds: u8, frames: u8) -> Timecode {
        Timecode {
            hours,
            minutes,
            seconds,
            frames,
            drop_frame: false,
        }
    }

    /// Spell a position out the way a sender would, and hand it to a reader.
    fn spell(reader: &mut Reader, position: Timecode, rate: Rate) -> Vec<Heard> {
        (0..8)
            .map(|piece| reader.take(&quarter_frame(piece, position, rate)))
            .collect()
    }

    #[test]
    fn a_whole_sequence_becomes_a_position_two_frames_later() {
        // The two frames are not an off-by-one to be tidied away: they went by
        // while the position was being spelled out, and every receiver adds
        // them back. Leaving them off is 80 ms of lateness at 25 fps.
        let mut reader = Reader::new();
        let heard = spell(&mut reader, at(10, 11, 12, 13), Rate::Fps25);

        assert!(
            heard[..7].iter().all(|step| *step == Heard::Building),
            "a position appeared before the sequence finished: {heard:?}"
        );
        assert_eq!(heard[7], Heard::At(at(10, 11, 12, 15), Rate::Fps25, false));
    }

    #[test]
    fn drop_frame_skips_the_numbers_that_do_not_exist() {
        // At 29.97 drop frame, 00 and 01 are not used at the top of a minute
        // unless the minute is a multiple of ten. Counting straight through
        // reported 10:01:00;01, which never happens — found by audit.
        let mut reader = Reader::new();
        let at_the_edge = Timecode {
            hours: 10,
            minutes: 0,
            seconds: 59,
            frames: 29,
            drop_frame: true,
        };
        match spell(&mut reader, at_the_edge, Rate::Fps2997Drop)[7] {
            Heard::At(got, _, _) => {
                assert_eq!((got.minutes, got.seconds, got.frames), (1, 0, 3));
            }
            other => panic!("expected a position, got {other:?}"),
        }

        // And on the tenth minute nothing is skipped.
        let mut reader = Reader::new();
        let tenth = Timecode {
            hours: 10,
            minutes: 9,
            seconds: 59,
            frames: 29,
            drop_frame: true,
        };
        match spell(&mut reader, tenth, Rate::Fps2997Drop)[7] {
            Heard::At(got, _, _) => {
                assert_eq!((got.minutes, got.seconds, got.frames), (10, 0, 1));
            }
            other => panic!("expected a position, got {other:?}"),
        }
    }

    #[test]
    fn the_two_frames_carry_over_the_end_of_a_second() {
        let mut reader = Reader::new();
        let heard = spell(&mut reader, at(10, 0, 0, 24), Rate::Fps25);
        assert_eq!(heard[7], Heard::At(at(10, 0, 1, 1), Rate::Fps25, false));

        // And over the end of an hour.
        let mut reader = Reader::new();
        let heard = spell(&mut reader, at(9, 59, 59, 29), Rate::Fps30);
        assert_eq!(heard[7], Heard::At(at(10, 0, 0, 1), Rate::Fps30, false));
    }

    #[test]
    fn the_rate_comes_out_of_the_last_piece() {
        for rate in [Rate::Fps24, Rate::Fps25, Rate::Fps2997Drop, Rate::Fps30] {
            let mut reader = Reader::new();
            match spell(&mut reader, at(1, 2, 3, 4), rate)[7] {
                Heard::At(_, got, false) => assert_eq!(got, rate),
                other => panic!("{rate:?} gave {other:?}"),
            }
        }
    }

    #[test]
    fn a_full_frame_is_a_position_as_it_stands() {
        // Nothing was spelled out, so there is nothing to add back. This is
        // what arrives after a locate, and adding two frames to it would put
        // the show two frames past where somebody just parked it.
        let mut reader = Reader::new();
        let message = crate::mtc::full_frame(at(10, 11, 12, 13), Rate::Fps25);
        assert_eq!(
            reader.take(&message),
            Heard::At(at(10, 11, 12, 13), Rate::Fps25, false)
        );
    }

    #[test]
    fn running_backwards_is_read_as_running_backwards() {
        // Rewind a DAW and the pieces arrive seven down to zero. Read as a
        // forward sequence that keeps being interrupted, it would report
        // nothing at all — and the show would look frozen while it moved.
        let position = at(10, 0, 5, 10);
        let mut reader = Reader::new();
        let mut last = Heard::Ignored;
        for piece in (0..8).rev() {
            last = reader.take(&quarter_frame(piece, position, Rate::Fps25));
        }
        match last {
            Heard::At(got, _, reverse) => {
                assert!(reverse);
                assert_eq!(got, at(10, 0, 5, 8));
            }
            other => panic!("a rewind reported {other:?}"),
        }
    }

    #[test]
    fn an_interrupted_sequence_does_not_invent_a_position() {
        // Half of one moment and half of another would assemble into a time
        // that never happened, and a cue would fire at it.
        let mut reader = Reader::new();
        for piece in 0..4 {
            reader.take(&quarter_frame(piece, at(10, 0, 0, 0), Rate::Fps25));
        }
        // A jump in the middle of the run: pieces 6, 7 with nothing between.
        reader.take(&quarter_frame(6, at(11, 0, 0, 0), Rate::Fps25));
        let outcome = reader.take(&quarter_frame(7, at(11, 0, 0, 0), Rate::Fps25));
        assert_eq!(
            outcome,
            Heard::Building,
            "assembled a position out of two different moments"
        );
    }

    #[test]
    fn other_midi_traffic_is_left_alone() {
        let mut reader = Reader::new();
        for other in [
            &[0x90, 60, 127][..],
            &[0xB0, 7, 100][..],
            &[0xC0, 5][..],
            // A sysex that is not a full frame.
            &[0xF0, 0x7F, 0x7F, 0x02, 0x01, 0x01, b'1', 0xF7][..],
        ] {
            assert_eq!(reader.take(other), Heard::Ignored, "{other:?}");
        }
    }

    #[test]
    fn an_impossible_position_is_refused_rather_than_reported() {
        // 61 seconds is not a time. Reporting it would put a wild value into
        // the cue engine, which would read it as a seek.
        let mut reader = Reader::new();
        let broken = Timecode {
            hours: 10,
            minutes: 0,
            seconds: 61,
            frames: 0,
            drop_frame: false,
        };
        assert_eq!(spell(&mut reader, broken, Rate::Fps25)[7], Heard::Ignored);
    }
}
