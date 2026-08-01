// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
//
// Copyright (c) Leo Bolster.

//! Chasing MTC off a real MIDI port, with no sound card anywhere.
//!
//! This is the case the program could not do at all until now: a DAW on this
//! machine, or a Mac across the network, and no LTC and no audio interface.
//! The decoder has its own tests; what this proves is the whole path — bytes
//! arriving on a port, a position coming out, and a cue going off at it.
//!
//! Virtual ports are an ALSA/CoreMIDI feature, so on Windows there is nothing
//! to bind to and this skips itself rather than failing.

#[cfg(unix)]
mod unix_only {
    use midir::os::unix::VirtualOutput;
    use show::{Event, Runner};
    use std::time::{Duration, Instant};

    /// A port that pretends to be a DAW sending timecode.
    ///
    /// Names here must not be prefixes of one another. Port matching falls back
    /// to "contains" on purpose — ALSA decorates names with client numbers that
    /// change between reboots, and nobody should have to re-pick their desk for
    /// that — but two tests running at once with names like `x` and `x-stop`
    /// will happily grab each other's port. This failed one run in three until
    /// the names stopped overlapping.
    fn a_sender(name: &str) -> Option<midir::MidiOutputConnection> {
        let midi = midir::MidiOutput::new("chasefire-mtc-source").ok()?;
        let connection = midi.create_virtual(name).ok()?;
        std::thread::sleep(Duration::from_millis(200));
        Some(connection)
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

    /// Spell a position out as the eight quarter-frames a sender would send.
    fn spell(out: &mut midir::MidiOutputConnection, position: ltc::Timecode) {
        for piece in 0..8 {
            let message = sink::mtc::quarter_frame(piece, position, sink::mtc::Rate::Fps25);
            let _ = out.send(&message);
            std::thread::sleep(Duration::from_millis(10));
        }
    }

    #[test]
    fn a_cue_fires_from_timecode_that_arrived_over_midi() {
        let Some(mut out) = a_sender("cf-mtc-fire") else {
            eprintln!("no MIDI stack here; skipping");
            return;
        };

        let mut runner = Runner::new(25);
        if runner.open_mtc_input("cf-mtc-fire").is_err() {
            eprintln!("the virtual port did not appear; skipping");
            return;
        }

        // A cue at 10:00:02:00. The sequences below straddle it.
        runner.set_cues(vec![cue::Cue::new(
            1,
            "por MTC",
            at(10, 0, 2, 0),
            cue::Message::Osc {
                address: "/go".into(),
                args: vec![],
            },
        )]);
        runner.set_armed(true);

        // Each sequence reports the position two frames later, so 10:00:01:20
        // is heard as :22 and 10:00:01:23 as 10:00:02:00 — the cue's moment.
        let mut events = Vec::new();
        for position in [
            at(10, 0, 1, 15),
            at(10, 0, 1, 20),
            at(10, 0, 1, 23),
            at(10, 0, 2, 5),
        ] {
            spell(&mut out, position);
            // Give the callback thread a moment to hand them over.
            std::thread::sleep(Duration::from_millis(60));
            events.extend(runner.poll());
        }

        let fired: Vec<_> = events
            .iter()
            .filter_map(|event| match event {
                Event::Fired { firing, .. } => Some(firing.name.clone()),
                _ => None,
            })
            .collect();
        assert_eq!(fired, ["por MTC"], "the cue did not fire from MTC");

        // And it worked out the rate from the stream, the way it does with LTC.
        assert_eq!(runner.source(), Some(show::Source::Mtc));
        assert!(
            events
                .iter()
                .any(|event| matches!(event, Event::Locked { nominal: 25, .. })),
            "never reported locking to 25 fps"
        );
    }

    #[test]
    fn silence_is_how_mtc_says_it_stopped() {
        // There is no "stopped" message in MTC. The stream simply ends, so a
        // timeout is the whole of the detection — and until it fires, the show
        // still looks like it is running.
        let Some(mut out) = a_sender("cf-mtc-stop") else {
            return;
        };
        let mut runner = Runner::new(25);
        if runner.open_mtc_input("cf-mtc-stop").is_err() {
            return;
        }

        spell(&mut out, at(10, 0, 0, 0));
        std::thread::sleep(Duration::from_millis(60));
        runner.poll();
        assert!(runner.timecode().is_some(), "never picked the position up");

        let deadline = Instant::now() + Duration::from_secs(3);
        let mut lost = false;
        while !lost && Instant::now() < deadline {
            std::thread::sleep(Duration::from_millis(100));
            lost = runner
                .poll()
                .iter()
                .any(|event| matches!(event, Event::SignalLost));
        }
        assert!(lost, "the stream stopped and nothing noticed");
        assert!(runner.timecode().is_none());
    }
}
