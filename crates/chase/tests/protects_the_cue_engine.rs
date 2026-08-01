// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
//
// Copyright (c) Leo Bolster.

//! Does this layer actually earn its place?
//!
//! Rather than asserting a claim, this test builds the situation and runs the
//! cue engine both ways: raw frames straight in, and frames through the chaser.
//! If the chaser were pointless both would behave identically.
//!
//! Worth recording how this test started out, because it was wrong. The first
//! version claimed a corrupted frame would *lose* a cue, and the engine proved
//! otherwise: a wildly wrong frame reads as a seek, and the very next good
//! frame reads as a seek back, which re-arms everything ahead of it. A big
//! glitch heals itself.
//!
//! The damage is done by a *small* wrong value, and it is worse than "early".
//! A frame decoding a few frames ahead of the truth stays inside the seek
//! threshold, so it reads as ordinary playback and the engine fires the cue it
//! just "passed". The next frame steps backwards a little, which the engine
//! correctly treats as a rehearsal crawl and re-arms what lies ahead. Then the
//! timecode reaches the cue for real and fires it **again**.
//!
//! One corrupted frame, one cue, two triggers. On a lighting desk or a media
//! server that is a visible double-take, and no amount of staring at the cue
//! list afterwards explains it.
//!
//! The frame values used here are not invented. They are the shape of what a
//! real capture produced on a rig with the preamps up and an unbalanced cable:
//! a plausible-looking timecode, far from the truth, arriving alone.

use chase::Chaser;
use cue::{Action, Cue, Engine, OscArg};
use ltc::{DecodedFrame, Timecode};

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

fn engine_with_one_cue_at(at: Timecode) -> Engine {
    let mut engine = Engine::new(FPS);
    engine.set_cues(vec![Cue::new(
        1,
        "the one that matters",
        at,
        Action::Osc {
            address: "/go".into(),
            args: vec![OscArg::Int(1)],
        },
    )]);
    engine.set_armed(true);
    engine
}

/// A clean run with one frame whose value is wrong by a little: far enough
/// ahead to step over the cue, close enough that it does not look like a seek.
/// This is the shape corruption takes when only a couple of bits flip.
fn stream_with_garbage_before_the_cue() -> Vec<DecodedFrame> {
    let mut frames = Vec::new();
    let mut timecode = Timecode::new(10, 0, 0, 0);
    for index in 0..100 {
        // The cue sits at 10:00:02:00 — frame 50 of this run.
        if index == 46 {
            // Truth here is 10:00:01:21. This frame says 10:00:02:04: thirteen
            // frames out, under the two-thirds-of-a-second seek threshold.
            frames.push(frame(Timecode::new(10, 0, 2, 4)));
            continue;
        }
        frames.push(frame(timecode));
        timecode.advance_one_frame(FPS);
    }
    frames
}

#[test]
fn one_corrupt_frame_fires_a_cue_twice_when_nothing_is_filtering() {
    let cue_at = Timecode::new(10, 0, 2, 0);
    let mut engine = engine_with_one_cue_at(cue_at);

    let mut fired = Vec::new();
    for frame in stream_with_garbage_before_the_cue() {
        fired.extend(engine.update(frame.timecode, frame.reverse));
    }

    assert_eq!(
        fired.len(),
        2,
        "premise check: one bad frame should produce a double trigger"
    );
    assert_eq!(
        fired[0].fired_at,
        Timecode::new(10, 0, 2, 4),
        "the first firing rides the corrupted frame"
    );
    assert_eq!(
        fired[1].fired_at, cue_at,
        "and the second lands when the timecode really gets there"
    );
}

#[test]
fn the_chaser_saves_it() {
    let cue_at = Timecode::new(10, 0, 2, 0);
    let mut engine = engine_with_one_cue_at(cue_at);
    let mut chaser = Chaser::new(FPS);

    let mut fired = Vec::new();
    for frame in stream_with_garbage_before_the_cue() {
        if let Some(tick) = chaser.on_frame(&frame) {
            fired.extend(engine.update(tick.timecode, tick.reverse));
        }
    }

    assert_eq!(fired.len(), 1, "the cue did not survive the glitch");
    assert_eq!(fired[0].at, cue_at);
    assert_eq!(
        fired[0].fired_at, cue_at,
        "the cue fired, but not where it was programmed"
    );
    assert_eq!(
        chaser.rejections().broke_continuity,
        1,
        "exactly one frame should have been held back"
    );
    assert_eq!(chaser.rejections().seeks, 0, "nothing here was a real seek");
}

#[test]
fn and_a_genuine_seek_still_gets_through() {
    // The other half of the bargain: filtering must not make the tool deaf to
    // an operator actually moving the playhead.
    let mut engine = engine_with_one_cue_at(Timecode::new(10, 5, 1, 0));
    let mut chaser = Chaser::new(FPS);

    let mut timecode = Timecode::new(10, 0, 0, 0);
    let mut fired = Vec::new();
    for _ in 0..25 {
        if let Some(tick) = chaser.on_frame(&frame(timecode)) {
            fired.extend(engine.update(tick.timecode, tick.reverse));
        }
        timecode.advance_one_frame(FPS);
    }

    // Jump to just before the cue and carry on.
    let mut jumped = Timecode::new(10, 5, 0, 20);
    for _ in 0..40 {
        if let Some(tick) = chaser.on_frame(&frame(jumped)) {
            fired.extend(engine.update(tick.timecode, tick.reverse));
        }
        jumped.advance_one_frame(FPS);
    }

    assert_eq!(fired.len(), 1, "the cue after a real seek never fired");
    assert_eq!(chaser.rejections().seeks, 1);
}
