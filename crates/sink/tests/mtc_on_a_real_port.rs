// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
//
// Copyright (c) Leo Bolster.

//! MTC out of a real port, at the rate a receiver locks to.
//!
//! Whether the numbers are right is settled by the encoder's own tests. What
//! this is for is the part that cannot be checked in a struct: an MTC receiver
//! locks to **when** quarter-frames arrive, and a generator that sends all
//! eight in a burst carries the same numbers and clocks nothing at all.

#[cfg(unix)]
mod unix_only {
    use midir::os::unix::VirtualInput;
    use std::sync::mpsc;
    use std::time::{Duration, Instant};

    /// What arrived and exactly when, which is the half that matters here.
    type Heard = mpsc::Receiver<(Instant, Vec<u8>)>;

    fn listen(name: &str) -> Option<(Heard, midir::MidiInputConnection<()>)> {
        let input = midir::MidiInput::new("chasefire-mtc-test").ok()?;
        let (sender, receiver) = mpsc::channel();
        let connection = input
            .create_virtual(
                name,
                move |_stamp, bytes, _| {
                    let _ = sender.send((Instant::now(), bytes.to_vec()));
                },
                (),
            )
            .ok()?;
        std::thread::sleep(Duration::from_millis(200));
        Some((receiver, connection))
    }

    fn at(hours: u8, minutes: u8, seconds: u8, frames: u8) -> ltc::Timecode {
        ltc::Timecode {
            hours,
            minutes,
            seconds,
            frames,
            drop_frame: false,
        }
    }

    #[test]
    fn it_sends_quarter_frames_spread_out_rather_than_in_a_burst() {
        let Some((heard, _keep)) = listen("chasefire-mtc") else {
            eprintln!("no MIDI stack here; skipping");
            return;
        };
        let Ok(clock) = sink::MtcClock::start("chasefire-mtc") else {
            eprintln!("the virtual port did not appear; skipping");
            return;
        };

        // Run a second of 25 fps timecode past it, the way the runner would.
        std::thread::spawn(move || {
            for frame in 0..25u8 {
                clock.at(at(10, 0, 0, frame), 25.0, false);
                std::thread::sleep(Duration::from_millis(40));
            }
            // Held until the end so the clock is not dropped early.
            drop(clock);
        });

        let mut quarter_frames = Vec::new();
        let mut full_frames = 0;
        let deadline = Instant::now() + Duration::from_secs(3);
        while Instant::now() < deadline {
            match heard.recv_timeout(Duration::from_millis(200)) {
                Ok((when, bytes)) if bytes.first() == Some(&0xF1) => {
                    quarter_frames.push((when, bytes));
                }
                Ok((_, bytes)) if bytes.first() == Some(&0xF0) => full_frames += 1,
                Ok(_) => {}
                Err(_) => break,
            }
        }

        assert!(
            full_frames >= 1,
            "no full frame: a receiver joining mid-show would have nothing to jump to"
        );
        assert!(
            quarter_frames.len() > 40,
            "only {} quarter-frames in a second of 25 fps timecode; a hundred were due",
            quarter_frames.len()
        );

        // Every piece number should appear, in order and wrapping.
        let pieces: Vec<u8> = quarter_frames
            .iter()
            .map(|(_, bytes)| bytes[1] >> 4)
            .collect();
        for window in pieces.windows(2) {
            assert_eq!(
                window[1],
                (window[0] + 1) & 0x07,
                "the pieces came out of order: {pieces:?}"
            );
        }

        // And they must be spread out. At 25 fps a quarter-frame is due every
        // 10 ms; a burst would put most of the gaps at nearly zero.
        let gaps: Vec<f64> = quarter_frames
            .windows(2)
            .map(|pair| (pair[1].0 - pair[0].0).as_secs_f64() * 1000.0)
            .collect();
        let middle = {
            let mut sorted = gaps.clone();
            sorted.sort_by(|a, b| a.partial_cmp(b).unwrap());
            sorted[sorted.len() / 2]
        };
        assert!(
            (4.0..=20.0).contains(&middle),
            "the typical gap was {middle:.1} ms; 10 ms was due, and a burst would be near zero"
        );
    }
}
