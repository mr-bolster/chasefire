// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
//
// Copyright (c) Leo Bolster.

//! MIDI Time Code out: turning the timecode we are chasing back into a clock
//! other machines can chase.
//!
//! This is what makes the program useful to people who do not need cues at all.
//! A rig with LTC on a cable and a DAW that only speaks MTC has a hole in it,
//! and filling that hole is a sound card and this.
//!
//! MTC carries a timecode position in **eight quarter-frame messages spanning
//! two frames** — a nibble at a time, least significant first. That is not a
//! quirk to work around: it is why an MTC receiver is always a frame or two
//! behind, and why the position in the messages is the position at the *start*
//! of the sequence rather than now.

use ltc::Timecode;

/// The frame rate, as MTC spells it. Two bits, and only four possibilities —
/// 50 and 60 fps have no code, which is a limit of the standard and not of
/// this program.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Rate {
    Fps24,
    Fps25,
    Fps2997Drop,
    Fps30,
}

impl Rate {
    /// The nearest rate MTC can represent, or `None` for an unsupported rate.
    pub fn nearest(fps: f64, drop_frame: bool) -> Option<Self> {
        if !(23.0..=30.5).contains(&fps) {
            return None;
        }
        Some(match fps {
            rate if rate < 24.5 => Rate::Fps24,
            rate if rate < 27.0 => Rate::Fps25,
            _ if drop_frame => Rate::Fps2997Drop,
            _ => Rate::Fps30,
        })
    }

    pub fn code(self) -> u8 {
        match self {
            Rate::Fps24 => 0,
            Rate::Fps25 => 1,
            Rate::Fps2997Drop => 2,
            Rate::Fps30 => 3,
        }
    }

    /// How many frames a second, for pacing.
    pub fn fps(self) -> f64 {
        match self {
            Rate::Fps24 => 24.0,
            Rate::Fps25 => 25.0,
            Rate::Fps2997Drop => 30000.0 / 1001.0,
            Rate::Fps30 => 30.0,
        }
    }
}

/// One quarter-frame message. `piece` runs 0 to 7 and wraps.
///
/// Piece 7 carries the hours' high nibble *and* the frame rate, which is why a
/// receiver cannot know the rate until the whole sequence has gone by.
pub fn quarter_frame(piece: u8, at: Timecode, rate: Rate) -> [u8; 2] {
    let piece = piece & 0x07;
    let value = match piece {
        0 => at.frames & 0x0F,
        1 => at.frames >> 4,
        2 => at.seconds & 0x0F,
        3 => at.seconds >> 4,
        4 => at.minutes & 0x0F,
        5 => at.minutes >> 4,
        6 => at.hours & 0x0F,
        // Hours' high bit, with the rate sitting above it.
        _ => (at.hours >> 4) | (rate.code() << 1),
    };
    [0xF1, (piece << 4) | (value & 0x0F)]
}

/// A whole position in one message, for jumping rather than running.
///
/// Sent when the timecode locks or leaps: a receiver crawling through eight
/// quarter-frames would take two frames to find out, and after a seek it needs
/// to know immediately.
pub fn full_frame(at: Timecode, rate: Rate) -> Vec<u8> {
    vec![
        0xF0,
        0x7F,
        0x7F,
        0x01,
        0x01,
        (at.hours & 0x1F) | (rate.code() << 5),
        at.minutes,
        at.seconds,
        at.frames,
        0xF7,
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    fn at(hours: u8, minutes: u8, seconds: u8, frames: u8) -> Timecode {
        Timecode {
            hours,
            minutes,
            seconds,
            frames,
            drop_frame: false,
        }
    }

    #[test]
    fn a_whole_position_is_spelled_out_in_eight_nibbles() {
        // 10:11:12:13 — nothing round, so a swapped nibble would show.
        let position = at(10, 11, 12, 13);
        let pieces: Vec<u8> = (0..8)
            .map(|piece| quarter_frame(piece, position, Rate::Fps25)[1] & 0x0F)
            .collect();

        assert_eq!(pieces[0], 13 & 0x0F, "frames, low");
        assert_eq!(pieces[1], 13 >> 4, "frames, high");
        assert_eq!(pieces[2], 12 & 0x0F, "seconds, low");
        assert_eq!(pieces[3], 12 >> 4, "seconds, high");
        assert_eq!(pieces[4], 11 & 0x0F, "minutes, low");
        assert_eq!(pieces[5], 11 >> 4, "minutes, high");
        assert_eq!(pieces[6], 10 & 0x0F, "hours, low");
        // Hour 10 has nothing in its high nibble, so this piece is the rate
        // and only the rate — which is exactly the trap: get the shift wrong
        // and it still looks plausible.
        assert_eq!(
            pieces[7],
            Rate::Fps25.code() << 1,
            "hours high, with the rate"
        );
    }

    #[test]
    fn every_quarter_frame_says_which_piece_it_is() {
        for piece in 0..8u8 {
            let message = quarter_frame(piece, at(1, 2, 3, 4), Rate::Fps30);
            assert_eq!(message[0], 0xF1, "the status byte");
            assert_eq!(message[1] >> 4, piece, "the piece number lives up here");
            assert!(message[1] < 0x80, "data bytes never have the top bit set");
        }
    }

    #[test]
    fn the_rate_rides_in_the_last_piece_and_in_a_full_frame() {
        for (rate, code) in [
            (Rate::Fps24, 0),
            (Rate::Fps25, 1),
            (Rate::Fps2997Drop, 2),
            (Rate::Fps30, 3),
        ] {
            let last = quarter_frame(7, at(0, 0, 0, 0), rate)[1];
            assert_eq!((last & 0x0F) >> 1, code, "in the eighth quarter-frame");

            let full = full_frame(at(0, 0, 0, 0), rate);
            assert_eq!(full[5] >> 5, code, "and in a full frame");
        }
    }

    #[test]
    fn a_full_frame_is_a_position_a_receiver_can_jump_to() {
        let full = full_frame(at(23, 59, 58, 12), Rate::Fps25);
        assert_eq!(
            full,
            vec![
                0xF0,
                0x7F,
                0x7F,
                0x01,
                0x01,
                23 | (1 << 5),
                59,
                58,
                12,
                0xF7
            ]
        );
    }

    #[test]
    fn twenty_three_hours_still_fits_beside_the_rate() {
        // Hours run to 23, which needs five bits, and the rate sits in the two
        // above them. Getting this wrong turns 23:00 into something else at
        // the one hour of the day nobody is watching.
        let full = full_frame(at(23, 0, 0, 0), Rate::Fps30);
        assert_eq!(full[5] & 0x1F, 23, "the hours survive");
        assert_eq!(full[5] >> 5, 3, "and so does the rate");
    }

    #[test]
    fn unsupported_rates_are_refused_instead_of_aliased() {
        assert_eq!(Rate::nearest(50.0, false), None);
        assert_eq!(Rate::nearest(60.0, false), None);
        assert_eq!(Rate::nearest(25.0, false), Some(Rate::Fps25));
        assert_eq!(Rate::nearest(24.0, false), Some(Rate::Fps24));
        assert_eq!(Rate::nearest(30.0, false), Some(Rate::Fps30));
        assert_eq!(Rate::nearest(29.97, true), Some(Rate::Fps2997Drop));
    }
}
