//! How long does it take to start reading?
//!
//! On a stage there is no run-up: the timecode starts and the first cue may be
//! moments later. So the delay between "signal appears" and "first frame
//! decoded" is a number worth pinning down rather than discovering.
//!
//! One frame is the floor and no implementation can beat it — a frame is 80
//! bits and the sync word that identifies it is the last 16, so nothing can be
//! recognised until a whole frame has arrived. Told the frame rate, this
//! decoder hits that floor. Left to work the rate out for itself it costs two
//! frames more, which is the price of the convenience and is worth knowing
//! when deciding whether to pin the rate in the settings.

use ltc::{Decoder, Encoder, Sequence, Timecode};

const SAMPLE_RATE: f64 = 48_000.0;
const FPS: f64 = 25.0;

/// Build noise for `seconds`, then clean LTC. Returns the audio, the sample the
/// timecode starts at, and the timecodes written.
fn noise_then_timecode(seconds: f64) -> (Vec<f32>, usize, Vec<Timecode>) {
    let mut audio = Vec::new();
    let mut seed = 0x5EED_1234u32;
    for _ in 0..(SAMPLE_RATE * seconds) as usize {
        seed = seed.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
        audio.push(((seed >> 8) as f32 / 8_388_608.0 - 1.0) * 0.02);
    }
    let starts_at = audio.len();
    let written = Encoder::new().encode_sequence(
        Sequence {
            start: Timecode::new(10, 0, 0, 0),
            count: 30,
            nominal_fps: 25,
            fps: FPS,
            sample_rate: SAMPLE_RATE,
            amplitude: 0.4,
        },
        &mut audio,
    );
    (audio, starts_at, written)
}

/// Which frame of the run came out first, counting from one.
fn first_frame_number(mut decoder: Decoder, audio: &[f32], written: &[Timecode]) -> usize {
    let mut decoded = Vec::new();
    decoder.push_samples(audio, &mut decoded);
    let first = decoded.first().expect("nothing decoded at all");
    written
        .iter()
        .position(|timecode| *timecode == first.timecode)
        .expect("decoded a timecode that was never sent")
        + 1
}

#[test]
fn told_the_rate_it_locks_on_the_very_first_frame() {
    for noise in [0.0, 2.0] {
        let (audio, _, written) = noise_then_timecode(noise);
        let number = first_frame_number(Decoder::new(SAMPLE_RATE, FPS), &audio, &written);
        assert_eq!(
            number, 1,
            "with {noise} s of noise first, the first usable frame was number {number} \
             — one is the floor and we were hitting it"
        );
    }
}

#[test]
fn working_the_rate_out_costs_two_extra_frames_and_no_more() {
    for noise in [0.0, 2.0] {
        let (audio, _, written) = noise_then_timecode(noise);
        let number = first_frame_number(Decoder::detecting(SAMPLE_RATE), &audio, &written);
        assert!(
            number <= 3,
            "auto-detection took {number} frames to produce anything after {noise} s of noise"
        );
    }
}

#[test]
fn the_first_frame_is_reported_at_the_sample_that_completed_it() {
    // Less a claim about speed than a promise to whoever is compensating for
    // latency downstream: end_sample means what it says.
    let (audio, starts_at, _) = noise_then_timecode(0.0);
    let mut decoder = Decoder::new(SAMPLE_RATE, FPS);
    let mut decoded = Vec::new();
    decoder.push_samples(&audio, &mut decoded);

    let first = decoded.first().expect("nothing decoded");
    let samples_into_the_signal = first.end_sample as usize - starts_at;
    let frames = samples_into_the_signal as f64 / (SAMPLE_RATE / FPS);
    assert!(
        (0.9..=1.1).contains(&frames),
        "the first frame closed {frames:.2} frames in, which is not one frame"
    );
}
