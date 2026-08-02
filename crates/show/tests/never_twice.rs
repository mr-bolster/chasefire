// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
//
// Copyright (c) Leo Bolster.

//! One cue, one firing. The property, not the rules.
//!
//! A cue that goes off twice is the worst thing this program can do — worse
//! than one that does not go off, because the second one lands in the middle of
//! something and nobody knows why. It has happened once: the encoder truncated
//! frame 42 to 02, the chaser saw the show jump backwards, re-armed everything
//! it had already passed, and fired it all again on the way through.
//!
//! Every rule that would have caught that had a test. What did not have a test
//! was the **property** — so a defect that broke no individual rule, in a crate
//! nobody was looking at, walked straight through.
//!
//! So this does not test a rule. It runs whole shows through the real chain —
//! encode, decode, chase, fire — at every supported rate and sample rate, with
//! the signal deliberately damaged, and asserts the only thing that actually
//! matters: **each cue fired exactly once, in order, and nothing fired that
//! should not have.**

use chase::Chaser;
use cue::{Cue, Engine, Message};
use ltc::{Decoder, Encoder, Sequence, Timecode};

/// The counting rates that exist. Anything above 30 is carried on the wire at
/// half rate, so there is nothing else for a decoder to meet.
const RATES: [(u8, f64); 5] = [
    (24, 24_000.0 / 1001.0),
    (24, 24.0),
    (25, 25.0),
    (30, 30_000.0 / 1001.0),
    (30, 30.0),
];

fn a_cue(id: u32, at: Timecode) -> Cue {
    Cue::new(
        id,
        format!("cue {id}"),
        at,
        Message::Osc {
            address: "/go".into(),
            args: Vec::new(),
        },
    )
}

/// Run a whole show and report which cues fired, in the order they fired.
///
/// Deliberately the same wiring the command line uses: decoder, then chaser,
/// then engine. A test that runs its own simplified chain proves something
/// about the test.
fn run_a_show(
    nominal: u8,
    fps: f64,
    sample_rate: f64,
    cues: Vec<Cue>,
    damage: impl Fn(usize, &mut f32),
) -> Vec<u32> {
    let mut audio = Vec::new();
    Encoder::new().encode_sequence(
        Sequence {
            start: Timecode::new(10, 0, 0, 0),
            count: 90,
            nominal_fps: nominal,
            fps,
            sample_rate,
            amplitude: 0.5,
        },
        &mut audio,
    );
    for (index, sample) in audio.iter_mut().enumerate() {
        damage(index, sample);
    }

    let mut engine = Engine::new(nominal);
    engine.set_cues(cues);
    engine.set_armed(true);
    let mut chaser = Chaser::new(nominal);
    let mut decoder = Decoder::new(sample_rate, fps);

    let mut fired = Vec::new();
    for sample in audio {
        if let Some(frame) = decoder.push_sample(sample) {
            if let Some(tick) = chaser.on_frame(&frame) {
                for firing in engine.update(tick.timecode, tick.reverse) {
                    fired.push(firing.cue_id);
                }
            }
        }
    }
    fired
}

/// Cues spread across the run, plus two that the show never reaches.
fn a_show_worth_of_cues(nominal: u8) -> Vec<Cue> {
    let mut cues = vec![
        a_cue(1, Timecode::new(10, 0, 0, 10)),
        a_cue(2, Timecode::new(10, 0, 1, 0)),
        a_cue(3, Timecode::new(10, 0, 1, nominal / 2)),
        a_cue(4, Timecode::new(10, 0, 2, 0)),
    ];
    // Before the start and after the end: neither may ever go off.
    cues.push(a_cue(90, Timecode::new(9, 59, 59, 0)));
    cues.push(a_cue(91, Timecode::new(10, 5, 0, 0)));
    cues
}

#[test]
fn a_clean_show_fires_every_cue_exactly_once() {
    for (nominal, fps) in RATES {
        for sample_rate in [44_100.0, 48_000.0, 96_000.0] {
            let fired = run_a_show(
                nominal,
                fps,
                sample_rate,
                a_show_worth_of_cues(nominal),
                |_, _| {},
            );
            assert_eq!(
                fired,
                [1, 2, 3, 4],
                "at {nominal} fps ({fps:.3}) and {sample_rate} Hz"
            );
        }
    }
}

#[test]
fn a_damaged_signal_still_fires_every_cue_exactly_once() {
    // Bursts of noise heavy enough to lose frames outright. Losing a cue would
    // be bad; firing one twice is worse, and this is the shape of signal that
    // produced the double firing in the first place.
    for (nominal, fps) in RATES {
        let fired = run_a_show(
            nominal,
            fps,
            48_000.0,
            a_show_worth_of_cues(nominal),
            |i, s| {
                // A quarter of a second of rubbish, twice, away from the cues.
                if (14_000..26_000).contains(&i) || (60_000..72_000).contains(&i) {
                    *s += if i % 3 == 0 { 0.9 } else { -0.9 };
                }
            },
        );

        let mut seen = std::collections::HashMap::new();
        for id in &fired {
            *seen.entry(*id).or_insert(0) += 1;
        }
        // Every cue in the show, once each. Counting only what did fire would
        // pass a run in which nothing fired at all — which is how this test
        // first stayed green while the encoder was broken.
        for id in 1..=4 {
            assert_eq!(
                seen.get(&id).copied().unwrap_or(0),
                1,
                "cue {id} fired {:?} times at {nominal} fps: {fired:?}",
                seen.get(&id)
            );
        }
        assert!(
            !seen.contains_key(&90) && !seen.contains_key(&91),
            "a cue outside the show fired at {nominal} fps: {fired:?}"
        );
        assert!(
            fired.windows(2).all(|pair| pair[0] < pair[1]),
            "cues fired out of order at {nominal} fps: {fired:?}"
        );
    }
}

#[test]
fn the_engine_cannot_be_made_to_fire_twice_by_any_sequence_of_positions() {
    // Straight at the engine, with the shapes a broken decoder produces: a
    // frame repeated, a frame that steps back, a frame from nowhere. This is
    // the layer the truncation bug reached, and it must not be possible to
    // walk forwards through a cue and have it go off more than once without
    // going properly backwards first.
    let nominal = 25u8;
    let mut engine = Engine::new(nominal);
    engine.set_cues(vec![a_cue(1, Timecode::new(10, 0, 1, 0))]);
    engine.set_armed(true);

    let mut fired = 0;
    let mut at = Timecode::new(10, 0, 0, 20);
    for step in 0..60 {
        // Every fifth frame is a glitch of some kind.
        let position = match step % 5 {
            1 => at, // repeated
            2 => {
                // one frame back, the shape truncation produces
                let mut back = at;
                back.retreat_one_frame(nominal);
                back
            }
            3 => Timecode::new(10, 0, 0, 0), // a wild one
            _ => at,
        };
        fired += engine.update(position, false).len();
        at.advance_one_frame(nominal);
    }

    assert_eq!(
        fired, 1,
        "the cue went off {fired} times walking forwards through it once"
    );
}
