// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
//
// Copyright (c) Leo Bolster.

//! SMPTE Linear Timecode (LTC), decoded from — and encoded to — raw audio samples.
//!
//! This crate does no I/O of any kind. You feed it `f32` samples from whatever
//! source you like (a sound card, a WAV file, a test generator) and it hands you
//! back decoded frames. That keeps the part that has to be *correct* separate
//! from the part that has to talk to Windows, and makes the whole thing testable
//! on any machine without a sound card in sight.
//!
//! # How LTC works, briefly
//!
//! Every frame is exactly 80 bits, biphase-mark encoded: there is always a
//! transition at the start of a bit cell, and a `1` has an extra transition in
//! the middle. So a `0` is one long interval and a `1` is two short ones. Bits
//! 0..64 carry the timecode and user bits as BCD; bits 64..80 are a fixed sync
//! word that also tells you which direction the tape (or the file) is running.

use serde::{Deserialize, Serialize};
use std::fmt;

/// Sync word as it appears once a whole frame has been shifted in, playing forward.
const SYNC_FORWARD: u16 = 0xBFFC;
/// The same sync word seen when the signal is running backwards.
const SYNC_REVERSE: u16 = 0x3FFD;

/// Number of bits in one LTC frame.
const FRAME_BITS: u32 = 80;

/// A timecode value: hours, minutes, seconds, frames.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct Timecode {
    pub hours: u8,
    pub minutes: u8,
    pub seconds: u8,
    pub frames: u8,
    pub drop_frame: bool,
}

impl Timecode {
    pub fn new(hours: u8, minutes: u8, seconds: u8, frames: u8) -> Self {
        Self {
            hours,
            minutes,
            seconds,
            frames,
            drop_frame: false,
        }
    }

    pub fn with_drop_frame(mut self, drop_frame: bool) -> Self {
        self.drop_frame = drop_frame;
        self
    }

    /// True if every field can exist in the timecode formats Chasefire supports.
    ///
    /// LTC and MTC only carry frame labels up to 29. The physical signal can
    /// run faster, but 50/60 fps needs an explicit paired-frame convention;
    /// treating 40..59 as native labels aliases them back to 00..19.
    pub fn is_plausible(&self) -> bool {
        self.hours < 24
            && self.minutes < 60
            && self.seconds < 60
            && self.frames < 30
            && !(self.drop_frame && self.seconds == 0 && self.minutes % 10 != 0 && self.frames < 2)
    }

    /// True when this label exists at the given integer counting rate.
    pub fn is_valid_at(&self, nominal_fps: u8) -> bool {
        self.is_plausible()
            && matches!(nominal_fps, 24 | 25 | 30)
            && self.frames < nominal_fps
            && (!self.drop_frame || nominal_fps == 30)
    }

    /// Advance by one frame at the given nominal rate.
    ///
    /// `nominal_fps` is the integer rate (30 for 29.97, 25 for 25, and so on).
    /// Drop-frame skips frames 0 and 1 at the start of every minute except
    /// those divisible by ten — which is exactly what makes 29.97 track the
    /// wall clock instead of drifting away from it.
    pub fn advance_one_frame(&mut self, nominal_fps: u8) {
        self.frames += 1;
        if self.frames < nominal_fps {
            return;
        }
        self.frames = 0;
        self.seconds += 1;
        if self.seconds < 60 {
            return;
        }
        self.seconds = 0;
        self.minutes += 1;
        if self.drop_frame && nominal_fps == 30 && self.minutes % 10 != 0 {
            self.frames = 2;
        }
        if self.minutes < 60 {
            return;
        }
        self.minutes = 0;
        self.hours = (self.hours + 1) % 24;
    }

    /// Move back by one real frame at the given integer counting rate.
    pub fn retreat_one_frame(&mut self, nominal_fps: u8) {
        if self.drop_frame
            && nominal_fps == 30
            && self.seconds == 0
            && self.minutes % 10 != 0
            && self.frames == 2
        {
            self.frames = nominal_fps - 1;
            if self.minutes > 0 {
                self.minutes -= 1;
            } else {
                self.minutes = 59;
                if self.hours > 0 {
                    self.hours -= 1;
                } else {
                    self.hours = 23;
                }
            }
            self.seconds = 59;
            return;
        }

        if self.frames > 0 {
            self.frames -= 1;
            return;
        }
        self.frames = nominal_fps - 1;
        if self.seconds > 0 {
            self.seconds -= 1;
            return;
        }
        self.seconds = 59;
        if self.minutes > 0 {
            self.minutes -= 1;
            return;
        }
        self.minutes = 59;
        self.hours = if self.hours > 0 { self.hours - 1 } else { 23 };
    }

    /// Position within the day in real frame steps.
    pub fn as_frame_count(&self, nominal_fps: u32) -> u32 {
        let minutes = self.hours as u32 * 60 + self.minutes as u32;
        let labelled = (minutes * 60 + self.seconds as u32) * nominal_fps + self.frames as u32;
        if self.drop_frame && nominal_fps == 30 {
            // Two labels are skipped at every minute except each tenth.
            let dropped = 2 * (minutes - minutes / 10);
            labelled.saturating_sub(dropped)
        } else {
            labelled
        }
    }
}

impl fmt::Display for Timecode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        // Broadcast convention: a semicolon before the frames means drop-frame.
        let sep = if self.drop_frame { ';' } else { ':' };
        write!(
            f,
            "{:02}:{:02}:{:02}{}{:02}",
            self.hours, self.minutes, self.seconds, sep, self.frames
        )
    }
}

/// One successfully decoded LTC frame.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct DecodedFrame {
    pub timecode: Timecode,
    /// The signal is running backwards (rewind, or a scrubbing operator).
    pub reverse: bool,
    /// The 32 user bits, in case anyone ever wants the date stamp in them.
    pub user_bits: u32,
    /// Frame rate implied by the measured bit period.
    pub estimated_fps: f32,
    /// Whether the 80-bit word has an even number of ones, as the parity bit
    /// is supposed to guarantee. `false` means at least one bit is wrong —
    /// though only if the source bothered to set the bit at all, which is why
    /// [`Decoder::source_respects_parity`] exists.
    pub parity_ok: bool,
    /// Index of the sample that closed this frame, counted from the first
    /// sample ever pushed. This is what lets a cue fire with a known offset
    /// instead of "whenever the GUI thread got round to it".
    pub end_sample: u64,
}

/// The frame rates worth snapping a measurement to.
pub const KNOWN_FRAME_RATES: [f64; 5] = [24_000.0 / 1001.0, 24.0, 25.0, 30_000.0 / 1001.0, 30.0];

/// Round a measured rate to the closest rate anyone actually uses, when it is
/// close enough to be that rate rather than a bad measurement.
///
/// Note what this cannot do: 30 and 29.97 are one part in a thousand apart, as
/// are 24 and 23.98. No measurement of a sound card's output separates those
/// reliably — the card's own clock error is the same size. Happily it does not
/// matter for firing cues, because both members of each pair count their frames
/// identically; see [`Timecode::advance_one_frame`]. Where it does matter —
/// generating, and what the screen says — ask the operator.
pub fn snap_to_known_frame_rate(measured: f64) -> Option<f64> {
    KNOWN_FRAME_RATES
        .iter()
        .copied()
        .filter(|rate| (measured - rate).abs() / rate < 0.02)
        .min_by(|left, right| {
            let distance = |rate: &f64| (measured - rate).abs();
            distance(left).total_cmp(&distance(right))
        })
}

/// How the decoder is getting its bit period.
enum Lock {
    /// Measuring the incoming signal before trusting it. Only used when the
    /// decoder was built without being told the frame rate.
    Searching {
        intervals: Vec<u32>,
    },
    Locked,
}

/// Streaming LTC decoder.
///
/// Push samples in, get frames out. It tracks the bit period as it goes, so it
/// copes with a sound card whose clock is a little off, with pull-up/pull-down,
/// and with an operator riding the gain.
pub struct Decoder {
    sample_rate: f64,
    nominal_bit_period: f64,
    bit_period: f64,

    // Input conditioning: a DC blocker feeding a Schmitt trigger whose
    // threshold follows the envelope, so a hot signal and a weak one both work.
    previous_input: f32,
    previous_output: f32,
    envelope: f32,
    is_high: bool,
    samples_since_edge: u32,

    // Biphase state: a `1` arrives as two short intervals, so the first one
    // has to wait here until we see its other half.
    pending_half: Option<u32>,

    lock: Lock,
    /// True when the rate came from measurement rather than from being told.
    /// Only a self-taught lock is allowed to give up and start over.
    auto_detect: bool,
    samples_since_lock: u64,
    /// Frames actually decoded at the current lock. Zero means the rate we
    /// settled on has not proved itself yet.
    frames_at_this_lock: u32,
    /// How many times we have had to go back and measure again. Worth showing
    /// an operator: a number that keeps climbing means the signal is not what
    /// the cable says it is.
    detection_attempts: u32,
    parity_checked: u32,
    parity_failed: u32,
    rejected_frames: u32,
    register: u128,
    bits_filled: u32,
    sample_index: u64,
    samples_since_frame: u64,
}

impl Decoder {
    /// `nominal_fps` is a hint, not a promise — the decoder locks onto whatever
    /// actually arrives. It just stops us from mistaking 60 fps for 30 fps
    /// during the first few bits, before anything has been measured.
    pub fn new(sample_rate: f64, nominal_fps: f64) -> Self {
        let nominal_bit_period = sample_rate / (FRAME_BITS as f64 * nominal_fps);
        Self {
            sample_rate,
            nominal_bit_period,
            bit_period: nominal_bit_period,
            previous_input: 0.0,
            previous_output: 0.0,
            envelope: 0.0,
            is_high: false,
            samples_since_edge: 0,
            pending_half: None,
            lock: Lock::Locked,
            auto_detect: false,
            samples_since_lock: 0,
            frames_at_this_lock: 0,
            detection_attempts: 0,
            parity_checked: 0,
            parity_failed: 0,
            rejected_frames: 0,
            register: 0,
            bits_filled: 0,
            sample_index: 0,
            samples_since_frame: 0,
        }
    }

    /// Work the frame rate out from the signal instead of being told it.
    ///
    /// This is the constructor the application should use. An operator plugging
    /// a cable into a strange rig does not know what is coming down it, and
    /// should not have to: the bit period is right there in the waveform.
    pub fn detecting(sample_rate: f64) -> Self {
        // The nominal only seeds the sanity limits; measurement replaces it.
        let mut decoder = Self::new(sample_rate, 30.0);
        decoder.lock = Lock::Searching {
            intervals: Vec::with_capacity(BOOTSTRAP_INTERVALS),
        };
        decoder.auto_detect = true;
        decoder
    }

    /// The frame rate measured from the signal, snapped to a real one when it
    /// is close enough. `None` while still searching, or when what is arriving
    /// is not a rate anybody uses.
    pub fn detected_frame_rate(&self) -> Option<f64> {
        if matches!(self.lock, Lock::Searching { .. }) {
            return None;
        }
        snap_to_known_frame_rate(self.estimated_fps())
    }

    /// Throw away any partially received frame and go back to the nominal rate.
    pub fn reset(&mut self) {
        self.bit_period = self.nominal_bit_period;
        self.pending_half = None;
        self.register = 0;
        self.bits_filled = 0;
        self.envelope = 0.0;
        self.samples_since_frame = 0;
    }

    /// Feed a block of samples; every frame recognised inside it is appended to `out`.
    pub fn push_samples(&mut self, samples: &[f32], out: &mut Vec<DecodedFrame>) {
        for &sample in samples {
            if let Some(frame) = self.push_sample(sample) {
                out.push(frame);
            }
        }
    }

    /// Feed one sample. Returns a frame on the sample that completes one.
    pub fn push_sample(&mut self, sample: f32) -> Option<DecodedFrame> {
        self.sample_index += 1;
        self.samples_since_frame += 1;

        // A lock that produces no frames is not a lock. Give up on it and
        // measure again rather than sitting deaf on a bad estimate — the case
        // that matters is opening the input before the timecode starts, which
        // is how anyone actually plugs in.
        //
        // How long to wait depends on whether this lock has ever worked.
        // An unproven one gets three frames: if the rate were right, a frame
        // would have come out by now, so keep re-measuring rather than sitting
        // there deaf. Better to detect five times over than to never start.
        // A lock that has decoded frames has earned patience — a dropout is not
        // a reason to throw away a rate we know is correct.
        if self.auto_detect && matches!(self.lock, Lock::Locked) {
            self.samples_since_lock += 1;
            let patience = if self.frames_at_this_lock == 0 {
                (self.bit_period * FRAME_BITS as f64 * 3.0) as u64
            } else {
                (self.sample_rate * 2.0) as u64
            };
            if self.samples_since_frame > patience && self.samples_since_lock > patience {
                self.start_searching();
            }
        }
        self.samples_since_edge = self.samples_since_edge.saturating_add(1);

        // Strip any DC offset the interface may add; LTC is symmetric about
        // zero. One-pole high pass at roughly 40 Hz, which settles in a few
        // milliseconds instead of eating the first frame while it converges.
        let value = sample - self.previous_input + 0.995 * self.previous_output;
        self.previous_input = sample;
        self.previous_output = value;

        // Envelope follower with a slow release, so the trigger threshold sits
        // sensibly below the peaks whatever the input level is.
        let magnitude = value.abs();
        self.envelope = if magnitude > self.envelope {
            magnitude
        } else {
            self.envelope * 0.9995
        };
        let threshold = (self.envelope * 0.3).max(1.0e-4);

        let edge = if !self.is_high && value > threshold {
            self.is_high = true;
            true
        } else if self.is_high && value < -threshold {
            self.is_high = false;
            true
        } else {
            false
        };

        // If the signal goes away entirely, drop the half-built frame rather
        // than splicing silence onto whatever arrives next.
        if self.samples_since_edge as f64 > self.bit_period * 4.0 {
            self.pending_half = None;
            self.bits_filled = 0;
        }

        if !edge {
            return None;
        }

        let interval = self.samples_since_edge;
        self.samples_since_edge = 0;
        self.classify_interval(interval)
    }

    /// Turn the gap between two transitions into a bit, biphase-mark style.
    fn classify_interval(&mut self, interval: u32) -> Option<DecodedFrame> {
        if let Lock::Searching { intervals } = &mut self.lock {
            intervals.push(interval);
            if intervals.len() < BOOTSTRAP_INTERVALS {
                return None;
            }
            match estimate_bit_period(intervals, self.sample_rate) {
                Some(period) => {
                    self.bit_period = period;
                    self.nominal_bit_period = period;
                    self.lock = Lock::Locked;
                    self.samples_since_lock = 0;
                    self.frames_at_this_lock = 0;
                }
                None => {
                    // Nothing sensible in there. Drop the oldest half and keep
                    // listening rather than locking onto noise.
                    intervals.drain(..BOOTSTRAP_INTERVALS / 2);
                }
            }
            return None;
        }

        let short_limit = self.bit_period * 0.75;
        let interval_f = interval as f64;

        match self.pending_half.take() {
            Some(first_half) => {
                if interval_f > short_limit {
                    // The "first half" was noise: this interval is a whole bit.
                    self.track_bit_period(interval_f);
                    return self.push_bit(false);
                }
                let whole = first_half as f64 + interval_f;
                self.track_bit_period(whole);
                self.push_bit(true)
            }
            None => {
                if interval_f > short_limit {
                    self.track_bit_period(interval_f);
                    self.push_bit(false)
                } else {
                    self.pending_half = Some(interval);
                    None
                }
            }
        }
    }

    /// Follow the incoming bit rate, but refuse to be dragged somewhere absurd.
    fn track_bit_period(&mut self, observed: f64) {
        if observed < self.nominal_bit_period * 0.4 || observed > self.nominal_bit_period * 2.5 {
            return;
        }
        self.bit_period += 0.05 * (observed - self.bit_period);
    }

    fn push_bit(&mut self, bit: bool) -> Option<DecodedFrame> {
        self.register = (self.register >> 1) | ((bit as u128) << (FRAME_BITS - 1));
        if self.bits_filled < FRAME_BITS {
            self.bits_filled += 1;
            if self.bits_filled < FRAME_BITS {
                return None;
            }
        }

        // Playing forward, the sync word is the newest thing in the register, so
        // it sits in the top 16 bits. Running backwards the frame arrives bit 79
        // first, which parks the sync word down at the bottom instead — same
        // pattern, opposite end. Miss that and reverse never decodes at all.
        let (payload, reverse) = if ((self.register >> 64) & 0xFFFF) as u16 == SYNC_FORWARD {
            (self.register, false)
        } else if (self.register & 0xFFFF) as u16 == SYNC_REVERSE {
            (reverse_bits_80(self.register), true)
        } else {
            return None;
        };

        let Some(timecode) = decode_timecode(payload) else {
            self.rejected_frames = self.rejected_frames.saturating_add(1);
            return None;
        };

        let parity_ok = parity_is_even(payload);
        self.parity_checked = self.parity_checked.saturating_add(1);
        if !parity_ok {
            self.parity_failed = self.parity_failed.saturating_add(1);
        }

        let estimated_fps = (self.sample_rate / (FRAME_BITS as f64 * self.bit_period)) as f32;
        self.samples_since_frame = 0;
        self.frames_at_this_lock = self.frames_at_this_lock.saturating_add(1);

        Some(DecodedFrame {
            timecode,
            reverse,
            user_bits: decode_user_bits(payload),
            estimated_fps,
            parity_ok,
            end_sample: self.sample_index,
        })
    }

    /// Best current estimate of the incoming frame rate.
    pub fn estimated_fps(&self) -> f64 {
        self.sample_rate / (FRAME_BITS as f64 * self.bit_period)
    }

    /// Throw the current rate away and start measuring again.
    fn start_searching(&mut self) {
        self.lock = Lock::Searching {
            intervals: Vec::with_capacity(BOOTSTRAP_INTERVALS),
        };
        self.samples_since_lock = 0;
        self.frames_at_this_lock = 0;
        self.bits_filled = 0;
        self.pending_half = None;
        self.detection_attempts = self.detection_attempts.saturating_add(1);
    }

    /// How many times the rate has had to be worked out again from scratch.
    pub fn detection_attempts(&self) -> u32 {
        self.detection_attempts
    }

    /// Whether this source actually maintains the parity bit.
    ///
    /// `None` until enough frames have gone by to tell. Plenty of gear in the
    /// wild leaves the bit alone — libltc's own decoder never looks at it — so
    /// parity is only worth enforcing once a source has shown it keeps it.
    pub fn source_respects_parity(&self) -> Option<bool> {
        if self.parity_checked < 8 {
            return None;
        }
        Some(self.parity_failed * 4 < self.parity_checked)
    }

    /// Frames that arrived with an intact sync word but impossible contents.
    /// A rising count is the earliest sign that a feed is going bad.
    pub fn rejected_frames(&self) -> u32 {
        self.rejected_frames
    }

    /// Samples elapsed since the last good frame — how a caller notices the
    /// timecode has gone away and it is time to freewheel.
    pub fn samples_since_last_frame(&self) -> u64 {
        self.samples_since_frame
    }
}

/// How many transitions to measure before trusting a rate. Two frames' worth:
/// enough for both a long and a short interval to show up many times over.
const BOOTSTRAP_INTERVALS: usize = 160;

/// Work out the bit period from a pile of raw transition intervals.
///
/// A biphase signal only ever produces two lengths: one bit cell for a `0`, and
/// half of one for each half of a `1`. So the intervals fall into two clusters
/// whose centres are a factor of two apart. Split them with a two-means pass —
/// no assumption about which is more common, which matters because that depends
/// entirely on the timecode value being sent.
fn estimate_bit_period(intervals: &[u32], sample_rate: f64) -> Option<f64> {
    let smallest = *intervals.iter().min()? as f64;
    let largest = *intervals.iter().max()? as f64;
    if largest < smallest * 1.4 {
        // Only one cluster: not biphase, or we caught a run of identical bits.
        return None;
    }

    let (mut short_centre, mut long_centre) = (smallest, largest);
    for _ in 0..12 {
        let (mut short_sum, mut short_count) = (0.0, 0usize);
        let (mut long_sum, mut long_count) = (0.0, 0usize);
        for &interval in intervals {
            let value = interval as f64;
            if (value - short_centre).abs() <= (value - long_centre).abs() {
                short_sum += value;
                short_count += 1;
            } else {
                long_sum += value;
                long_count += 1;
            }
        }
        if short_count == 0 || long_count == 0 {
            return None;
        }
        short_centre = short_sum / short_count as f64;
        long_centre = long_sum / long_count as f64;
    }

    // The two clusters must really be a half and a whole bit cell. Anything
    // else means we are looking at something that is not LTC.
    let ratio = long_centre / short_centre;
    if !(1.6..=2.4).contains(&ratio) {
        return None;
    }

    // And the answer has to be a frame rate that exists. Noise can produce two
    // clusters a factor of two apart by chance; noise cannot produce them at a
    // believable LTC bit rate. Without this check the decoder can lock onto
    // silence before the show starts and then stay deaf to the real signal,
    // because the tracking guard refuses to move far from a locked estimate.
    let implied_fps = sample_rate / (FRAME_BITS as f64 * long_centre);
    if !(23.0..=61.0).contains(&implied_fps) {
        return None;
    }
    Some(long_centre)
}

fn field(payload: u128, start: u32, len: u32) -> u32 {
    ((payload >> start) & ((1u128 << len) - 1)) as u32
}

/// Pull the timecode out of a frame, refusing anything that is not valid BCD.
///
/// Each field is binary-coded decimal, so a units digit above 9 cannot happen
/// in a frame that arrived intact. Checking costs nothing and catches a good
/// share of corruption — and the reference implementation does not do it, which
/// is part of why a weak feed produces plausible-looking wrong times.
fn decode_timecode(payload: u128) -> Option<Timecode> {
    let frame_units = field(payload, 0, 4);
    let second_units = field(payload, 16, 4);
    let second_tens = field(payload, 24, 3);
    let minute_units = field(payload, 32, 4);
    let minute_tens = field(payload, 40, 3);
    let hour_units = field(payload, 48, 4);
    let hour_tens = field(payload, 56, 2);

    if frame_units > 9
        || second_units > 9
        || second_tens > 5
        || minute_units > 9
        || minute_tens > 5
        || hour_units > 9
        || hour_tens > 2
    {
        return None;
    }

    let timecode = Timecode {
        frames: (frame_units + field(payload, 8, 2) * 10) as u8,
        seconds: (second_units + second_tens * 10) as u8,
        minutes: (minute_units + minute_tens * 10) as u8,
        hours: (hour_units + hour_tens * 10) as u8,
        drop_frame: field(payload, 10, 1) == 1,
    };
    timecode.is_plausible().then_some(timecode)
}

/// True when the 80-bit word carries an even number of ones.
///
/// That is what the biphase mark phase correction bit is for: the encoder sets
/// it so the total comes out even. Verifying costs one instruction and needs no
/// knowledge of which bit it is — handy, because it lives at bit 27 for 24 and
/// 30 fps but at bit 59 for 25.
fn parity_is_even(payload: u128) -> bool {
    (payload & ((1u128 << FRAME_BITS) - 1)).count_ones() % 2 == 0
}

fn decode_user_bits(payload: u128) -> u32 {
    let mut bits = 0u32;
    for (index, start) in [4, 12, 20, 28, 36, 44, 52, 60].iter().enumerate() {
        bits |= field(payload, *start, 4) << (index * 4);
    }
    bits
}

fn reverse_bits_80(value: u128) -> u128 {
    let mut reversed = 0u128;
    for bit in 0..FRAME_BITS {
        if (value >> bit) & 1 == 1 {
            reversed |= 1u128 << (FRAME_BITS - 1 - bit);
        }
    }
    reversed
}

fn set_field(bits: &mut u128, start: u32, len: u32, value: u32) {
    *bits |= ((value as u128) & ((1u128 << len) - 1)) << start;
}

/// Build the 80 bits of one LTC frame.
fn build_frame_bits(timecode: Timecode, nominal_fps: u8) -> u128 {
    let mut bits = 0u128;

    set_field(&mut bits, 0, 4, (timecode.frames % 10) as u32);
    set_field(&mut bits, 8, 2, (timecode.frames / 10) as u32);
    set_field(&mut bits, 16, 4, (timecode.seconds % 10) as u32);
    set_field(&mut bits, 24, 3, (timecode.seconds / 10) as u32);
    set_field(&mut bits, 32, 4, (timecode.minutes % 10) as u32);
    set_field(&mut bits, 40, 3, (timecode.minutes / 10) as u32);
    set_field(&mut bits, 48, 4, (timecode.hours % 10) as u32);
    set_field(&mut bits, 56, 2, (timecode.hours / 10) as u32);
    if timecode.drop_frame {
        set_field(&mut bits, 10, 1, 1);
    }
    // Sync word, bits 64..80.
    set_field(&mut bits, 64, 16, SYNC_FORWARD as u32);

    // Parity, so the whole word carries an even number of ones. Bit 27 for 24
    // and 30 fps, bit 59 for 25 — the one place the standards disagree.
    if !parity_is_even(bits) {
        let parity_bit = if nominal_fps == 25 { 59 } else { 27 };
        set_field(&mut bits, parity_bit, 1, 1);
    }
    bits
}

/// Encoder state, so consecutive frames join up without a glitch at the seam.
#[derive(Debug, Clone, Copy, Default)]
pub struct Encoder {
    level_is_high: bool,
}

impl Encoder {
    pub fn new() -> Self {
        Self::default()
    }

    /// Append one frame of LTC audio to `out`.
    ///
    /// Biphase mark: flip at every bit boundary, and flip again in the middle
    /// of the cell when the bit is a `1`.
    pub fn encode_frame(
        &mut self,
        timecode: Timecode,
        nominal_fps: u8,
        fps: f64,
        sample_rate: f64,
        amplitude: f32,
        out: &mut Vec<f32>,
    ) {
        // A label that cannot exist gets a frame of silence, not a panic.
        //
        // This runs inside the audio callback. A panic there unwinds across a
        // C boundary, which aborts the process — and it would abort it while
        // generating timecode for a show. Silence is the honest output: a
        // receiver sees no timecode, which is true, rather than a wrong one,
        // which is worse. `encode_sequence` refuses loudly instead, because it
        // is offline and the caller can be told.
        if !timecode.is_valid_at(nominal_fps) {
            let samples = (sample_rate / fps).round() as usize;
            out.resize(out.len() + samples, 0.0);
            return;
        }
        let bits = build_frame_bits(timecode, nominal_fps);
        let samples_per_bit = sample_rate / (FRAME_BITS as f64 * fps);

        for index in 0..FRAME_BITS {
            let start = (index as f64 * samples_per_bit).round() as usize;
            let middle = ((index as f64 + 0.5) * samples_per_bit).round() as usize;
            let end = ((index as f64 + 1.0) * samples_per_bit).round() as usize;

            self.level_is_high = !self.level_is_high;
            let mut level = if self.level_is_high {
                amplitude
            } else {
                -amplitude
            };
            for _ in start..middle {
                out.push(level);
            }

            if (bits >> index) & 1 == 1 {
                self.level_is_high = !self.level_is_high;
                level = -level;
            }
            for _ in middle..end {
                out.push(level);
            }
        }
    }

    /// Append `spec.count` consecutive frames, returning the timecodes written.
    pub fn encode_sequence(&mut self, spec: Sequence, out: &mut Vec<f32>) -> Vec<Timecode> {
        // Offline, so the caller is present to be told. `encode_frame` cannot
        // do this — it runs in the audio callback, where a panic aborts the
        // process mid-show.
        assert!(
            spec.start.is_valid_at(spec.nominal_fps),
            "{} is not a valid label at {} fps",
            spec.start,
            spec.nominal_fps
        );
        let mut timecode = spec.start;
        let mut written = Vec::with_capacity(spec.count as usize);
        for _ in 0..spec.count {
            self.encode_frame(
                timecode,
                spec.nominal_fps,
                spec.fps,
                spec.sample_rate,
                spec.amplitude,
                out,
            );
            written.push(timecode);
            timecode.advance_one_frame(spec.nominal_fps);
        }
        written
    }
}

/// A run of consecutive LTC frames to generate.
///
/// `nominal_fps` is the integer rate used for counting (30 for 29.97), while
/// `fps` is the real rate the audio is written at (30000/1001 for 29.97). They
/// differ for exactly the rates that gave the world drop-frame in the first place.
#[derive(Debug, Clone, Copy)]
pub struct Sequence {
    pub start: Timecode,
    pub count: u32,
    pub nominal_fps: u8,
    pub fps: f64,
    pub sample_rate: f64,
    pub amplitude: f32,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sequence(start: Timecode, count: u32, nominal_fps: u8, fps: f64) -> Sequence {
        Sequence {
            start,
            count,
            nominal_fps,
            fps,
            sample_rate: 48_000.0,
            amplitude: 0.5,
        }
    }

    fn round_trip(spec: Sequence) -> (Vec<Timecode>, Vec<DecodedFrame>) {
        let mut audio = Vec::new();
        let expected = Encoder::new().encode_sequence(spec, &mut audio);

        let mut decoder = Decoder::new(spec.sample_rate, spec.nominal_fps as f64);
        let mut decoded = Vec::new();
        decoder.push_samples(&audio, &mut decoded);
        (expected, decoded)
    }

    #[test]
    fn decodes_25fps_at_48k() {
        let (expected, decoded) = round_trip(sequence(Timecode::new(10, 23, 45, 12), 12, 25, 25.0));

        // The very first frame is spent locking on, so we never expect all of them.
        assert!(
            decoded.len() >= expected.len() - 1,
            "decoded {}",
            decoded.len()
        );
        for frame in &decoded {
            assert!(
                expected.contains(&frame.timecode),
                "unexpected {}",
                frame.timecode
            );
            assert!(!frame.reverse);
        }
        // And the sequence must arrive in order, without repeats.
        for pair in decoded.windows(2) {
            assert_ne!(pair[0].timecode, pair[1].timecode);
        }
    }

    #[test]
    fn decodes_drop_frame_29_97() {
        let fps = 30_000.0 / 1001.0;
        let start = Timecode::new(1, 0, 0, 0).with_drop_frame(true);
        let (expected, decoded) = round_trip(sequence(start, 10, 30, fps));

        assert!(decoded.len() >= expected.len() - 1);
        for frame in &decoded {
            assert!(frame.timecode.drop_frame, "drop-frame flag lost");
            assert!(expected.contains(&frame.timecode));
        }
    }

    #[test]
    fn drop_frame_skips_two_frames_each_minute() {
        let mut timecode = Timecode::new(0, 0, 59, 29).with_drop_frame(true);
        timecode.advance_one_frame(30);
        assert_eq!(timecode.to_string(), "00:01:00;02");

        timecode.retreat_one_frame(30);
        assert_eq!(timecode.to_string(), "00:00:59;29");

        let before = Timecode::new(0, 0, 59, 29).with_drop_frame(true);
        let after = Timecode::new(0, 1, 0, 2).with_drop_frame(true);
        assert_eq!(after.as_frame_count(30) - before.as_frame_count(30), 1);

        // ...except on the tenth minute, where nothing is skipped.
        let mut tenth = Timecode::new(0, 9, 59, 29).with_drop_frame(true);
        tenth.advance_one_frame(30);
        assert_eq!(tenth.to_string(), "00:10:00;00");
    }

    #[test]
    fn survives_a_weak_and_dirty_signal() {
        // A quiet feed with DC offset and noise on top: the classic
        // "it comes off a long cable through a mic preamp" situation.
        let mut audio = Vec::new();
        let mut spec = sequence(Timecode::new(2, 0, 0, 0), 12, 25, 25.0);
        spec.amplitude = 0.03;
        let expected = Encoder::new().encode_sequence(spec, &mut audio);

        let mut seed = 0x1234_5678u32;
        for sample in audio.iter_mut() {
            seed = seed.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
            let noise = (seed >> 8) as f32 / 8_388_608.0 - 1.0;
            *sample += 0.1 + noise * 0.004;
        }

        let mut decoder = Decoder::new(48_000.0, 25.0);
        let mut decoded = Vec::new();
        decoder.push_samples(&audio, &mut decoded);

        assert!(
            decoded.len() >= expected.len() - 2,
            "decoded {}",
            decoded.len()
        );
        for frame in &decoded {
            assert!(expected.contains(&frame.timecode));
        }
    }

    #[test]
    fn reports_reverse_when_the_signal_runs_backwards() {
        let mut audio = Vec::new();
        Encoder::new().encode_sequence(
            sequence(Timecode::new(4, 30, 0, 0), 10, 25, 25.0),
            &mut audio,
        );
        audio.reverse();

        let mut decoder = Decoder::new(48_000.0, 25.0);
        let mut decoded = Vec::new();
        decoder.push_samples(&audio, &mut decoded);

        assert!(!decoded.is_empty(), "nothing decoded backwards");
        assert!(decoded.iter().all(|frame| frame.reverse));
    }

    #[test]
    fn estimates_the_frame_rate_it_is_given() {
        for (nominal, fps) in [(24u8, 24.0), (25, 25.0), (30, 30.0)] {
            let (_, decoded) = round_trip(sequence(Timecode::new(0, 0, 10, 0), 10, nominal, fps));
            let last = decoded.last().expect("no frames decoded");
            assert!(
                (last.estimated_fps as f64 - fps).abs() < 0.5,
                "{fps} fps estimated as {}",
                last.estimated_fps
            );
        }
    }

    #[test]
    fn decodes_every_sample_rate_at_every_frame_rate() {
        // The matrix a live rig actually throws at you. 96 kHz gives the decoder
        // twice the samples per bit; every supported counting rate must survive
        // every sample rate without relying on the encoder's favourite case.
        for sample_rate in [44_100.0, 48_000.0, 96_000.0] {
            for (nominal, fps) in [(24u8, 24.0), (25, 25.0), (30, 30.0)] {
                let spec = Sequence {
                    start: Timecode::new(10, 0, 30, 0),
                    count: 12,
                    nominal_fps: nominal,
                    fps,
                    sample_rate,
                    amplitude: 0.5,
                };
                let mut audio = Vec::new();
                let expected = Encoder::new().encode_sequence(spec, &mut audio);

                let mut decoder = Decoder::new(sample_rate, fps);
                let mut decoded = Vec::new();
                decoder.push_samples(&audio, &mut decoded);

                assert!(
                    decoded.len() >= expected.len() - 1,
                    "{sample_rate} Hz at {fps} fps: only {} of {} frames decoded \
                     ({:.2} samples per bit)",
                    decoded.len(),
                    expected.len(),
                    sample_rate / (80.0 * fps)
                );
                for frame in &decoded {
                    assert!(
                        expected.contains(&frame.timecode),
                        "{sample_rate} Hz at {fps} fps decoded a wrong value: {}",
                        frame.timecode
                    );
                }
            }
        }
    }

    #[test]
    fn works_out_the_frame_rate_without_being_told() {
        // The point of the whole exercise: plug a cable into an unknown rig and
        // have the software figure out what is coming down it.
        for sample_rate in [44_100.0, 48_000.0, 96_000.0] {
            for (nominal, fps) in [
                (24u8, 24.0),
                (25, 25.0),
                (30, 30.0),
                (30, 30_000.0 / 1001.0),
            ] {
                let mut audio = Vec::new();
                let expected = Encoder::new().encode_sequence(
                    Sequence {
                        start: Timecode::new(9, 30, 15, 0),
                        count: 20,
                        nominal_fps: nominal,
                        fps,
                        sample_rate,
                        amplitude: 0.4,
                    },
                    &mut audio,
                );

                let mut decoder = Decoder::detecting(sample_rate);
                let mut decoded = Vec::new();
                decoder.push_samples(&audio, &mut decoded);

                let detected = decoder
                    .detected_frame_rate()
                    .unwrap_or_else(|| panic!("{sample_rate} Hz at {fps} fps: never locked on"));
                assert!(
                    (detected - fps).abs() / fps < 0.02,
                    "{sample_rate} Hz: {fps} fps came out as {detected}"
                );
                assert!(
                    !decoded.is_empty(),
                    "{sample_rate} Hz at {fps} fps: locked but decoded nothing"
                );
                for frame in &decoded {
                    assert!(
                        expected.contains(&frame.timecode),
                        "{sample_rate} Hz at {fps} fps: wrong value {}",
                        frame.timecode
                    );
                }
            }
        }
    }

    #[test]
    fn the_ntsc_pairs_cannot_be_told_apart_but_count_the_same() {
        // Pinned deliberately. 30 and 29.97 differ by a thousandth, which is
        // smaller than the clock error of the sound card carrying them, so any
        // claim to distinguish them by measurement is a lie. What saves us is
        // that both count 0..29, so a cue lands in the same place either way.
        for (fast, slow) in [(30.0f64, 30_000.0 / 1001.0), (24.0f64, 24_000.0 / 1001.0)] {
            assert!(
                (fast - slow) / fast < 0.002,
                "these rates are supposed to be nearly identical"
            );
            assert_eq!(
                fast.ceil() as u8,
                slow.ceil() as u8,
                "but they must count identically"
            );
        }

        // A clean measurement still snaps to the nearer of the two.
        assert_eq!(snap_to_known_frame_rate(30.0), Some(30.0));
        assert_eq!(snap_to_known_frame_rate(25.01), Some(25.0));
        assert_eq!(snap_to_known_frame_rate(37.0), None);
    }

    #[test]
    fn locks_on_after_listening_to_noise_first() {
        // The way anyone actually plugs in: open the input, wait, and only then
        // does the timecode start. A first version of the auto-detection could
        // lock its bit period onto the noise and then stay deaf for ever,
        // because the tracking guard refuses to move far from a locked
        // estimate. Found on a real rig, not in a test — hence this test.
        let mut audio = Vec::new();
        let mut seed = 0x5EED_1234u32;
        for _ in 0..(48_000 * 2) {
            seed = seed.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
            audio.push(((seed >> 8) as f32 / 8_388_608.0 - 1.0) * 0.02);
        }

        let expected = Encoder::new().encode_sequence(
            sequence(Timecode::new(10, 0, 0, 0), 60, 25, 25.0),
            &mut audio,
        );

        let mut decoder = Decoder::detecting(48_000.0);
        let mut decoded = Vec::new();
        decoder.push_samples(&audio, &mut decoded);

        assert!(
            decoded.len() > expected.len() / 2,
            "only {} of {} frames decoded after a noisy start",
            decoded.len(),
            expected.len()
        );
        for frame in &decoded {
            assert!(expected.contains(&frame.timecode));
        }
    }

    #[test]
    fn a_lock_that_never_produces_a_frame_is_abandoned() {
        // Same failure from the other side: force a bad lock by handing it a
        // signal, then silence, then a *different* rate. It has to re-measure.
        let mut audio = Vec::new();
        Encoder::new().encode_sequence(
            sequence(Timecode::new(1, 0, 0, 0), 25, 25, 25.0),
            &mut audio,
        );
        audio.extend(std::iter::repeat_n(0.0f32, 48_000 * 2));
        let expected = Encoder::new().encode_sequence(
            sequence(Timecode::new(2, 0, 0, 0), 60, 30, 30.0),
            &mut audio,
        );

        let mut decoder = Decoder::detecting(48_000.0);
        let mut decoded = Vec::new();
        decoder.push_samples(&audio, &mut decoded);

        let at_new_rate: Vec<_> = decoded
            .iter()
            .filter(|frame| expected.contains(&frame.timecode))
            .collect();
        assert!(
            at_new_rate.len() > 20,
            "only {} frames at the new rate — the old lock was never released",
            at_new_rate.len()
        );
    }

    /// Biphase-mark encode an arbitrary bit pattern. Used to build a signal
    /// that is convincing enough to lock onto and yet contains no timecode.
    fn biphase(bits: &[bool], samples_per_bit: f64, out: &mut Vec<f32>) {
        let mut high = false;
        for (index, bit) in bits.iter().enumerate() {
            let start = (index as f64 * samples_per_bit).round() as usize;
            let middle = ((index as f64 + 0.5) * samples_per_bit).round() as usize;
            let end = ((index as f64 + 1.0) * samples_per_bit).round() as usize;
            high = !high;
            for _ in start..middle {
                out.push(if high { 0.4 } else { -0.4 });
            }
            if *bit {
                high = !high;
            }
            for _ in middle..end {
                out.push(if high { 0.4 } else { -0.4 });
            }
        }
    }

    #[test]
    fn gives_up_fast_on_a_lock_that_produces_nothing() {
        // Leo's rule, in test form: better to detect five times over than to
        // sit there deaf. This signal has exactly the shape the rate estimator
        // is looking for — two interval clusters a factor of two apart, at a
        // believable LTC bit rate — but its bits alternate 0101… for ever, so
        // the sync word never appears and no frame can ever come out. A
        // decoder that locks and waits is stuck; this one must keep re-measuring.
        let samples_per_bit = 48_000.0 / (80.0 * 25.0);
        let mut audio = Vec::new();
        let decoy: Vec<bool> = (0..4_000).map(|index| index % 2 == 0).collect();
        biphase(&decoy, samples_per_bit, &mut audio);
        let decoy_samples = audio.len();

        let expected = Encoder::new().encode_sequence(
            sequence(Timecode::new(10, 0, 0, 0), 30, 25, 25.0),
            &mut audio,
        );

        let mut decoder = Decoder::detecting(48_000.0);
        let mut decoded = Vec::new();
        decoder.push_samples(&audio, &mut decoded);

        assert!(
            decoder.detection_attempts() > 1,
            "never went back to measure again: it was sitting on a dead lock"
        );

        let first = decoded.first().expect("never recovered at all");
        assert!(
            expected.contains(&first.timecode),
            "recovered into nonsense: {}",
            first.timecode
        );
        // And the recovery has to be quick, not eventual.
        let frames_late = (first.end_sample as usize - decoy_samples) as f64 / (48_000.0 / 25.0);
        assert!(
            frames_late < 6.0,
            "took {frames_late:.1} frames to shake off the false lock"
        );
    }

    #[test]
    fn a_proven_lock_is_not_thrown_away_over_a_short_dropout() {
        // The other side of the bargain. Once a rate has decoded real frames it
        // has earned patience: a gap in the signal must not cost the lock, or
        // every dropout would be followed by a re-detection nobody asked for.
        let mut audio = Vec::new();
        Encoder::new().encode_sequence(
            sequence(Timecode::new(10, 0, 0, 0), 25, 25, 25.0),
            &mut audio,
        );
        audio.extend(std::iter::repeat_n(0.0f32, 48_000 / 2)); // half a second
        let after = Encoder::new().encode_sequence(
            sequence(Timecode::new(10, 0, 5, 0), 25, 25, 25.0),
            &mut audio,
        );

        let mut decoder = Decoder::detecting(48_000.0);
        let mut decoded = Vec::new();
        decoder.push_samples(&audio, &mut decoded);

        assert_eq!(
            decoder.detection_attempts(),
            0,
            "a working lock was thrown away over half a second of silence"
        );
        assert!(
            decoded.iter().any(|frame| after.contains(&frame.timecode)),
            "did not pick the signal back up"
        );
    }

    #[test]
    fn refuses_to_lock_onto_something_that_is_not_timecode() {
        // Music, hiss, an unplugged input: it must report nothing rather than
        // invent a frame rate and start firing cues off it.
        let mut seed = 0x9E3779B9u32;
        let noise: Vec<f32> = (0..48_000 * 2)
            .map(|_| {
                seed = seed.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
                (seed >> 8) as f32 / 8_388_608.0 - 1.0
            })
            .collect();

        let mut decoder = Decoder::detecting(48_000.0);
        let mut decoded = Vec::new();
        decoder.push_samples(&noise, &mut decoded);
        assert!(
            decoded.is_empty(),
            "decoded {} frames of noise",
            decoded.len()
        );
    }

    #[test]
    fn a_label_that_cannot_exist_gives_silence_rather_than_a_panic() {
        // `encode_frame` runs inside the audio callback. A panic there unwinds
        // across a C boundary and aborts the process — while generating
        // timecode for a show. Silence is true (there is no timecode) where a
        // wrong label would be a lie, and a whole frame of it keeps the clock
        // where it should be.
        let mut audio = Vec::new();
        Encoder::new().encode_frame(
            Timecode::new(1, 0, 0, 42),
            25,
            25.0,
            48_000.0,
            0.5,
            &mut audio,
        );
        assert_eq!(audio.len(), 1920, "one frame of 25 fps at 48 kHz");
        assert!(
            audio.iter().all(|sample| *sample == 0.0),
            "a label that cannot exist must not go out sounding like one that can"
        );
    }

    #[test]
    #[should_panic(expected = "is not a valid label")]
    fn frame_numbers_above_29_are_refused_instead_of_aliased() {
        // The old encoder truncated the tens field: frame 40 came back as 00,
        // the chaser re-armed, and a cue at frame 10 fired twice. Programmer
        // misuse must be loud even though the CLI validates before this point.
        let spec = Sequence {
            start: Timecode::new(1, 0, 0, 42),
            count: 1,
            nominal_fps: 60,
            fps: 60.0,
            sample_rate: 48_000.0,
            amplitude: 0.5,
        };
        let mut audio = Vec::new();
        Encoder::new().encode_sequence(spec, &mut audio);
    }

    #[test]
    fn validation_uses_the_rate_and_refuses_nonexistent_drop_labels() {
        assert!(Timecode::new(1, 2, 3, 23).is_valid_at(24));
        assert!(!Timecode::new(1, 2, 3, 24).is_valid_at(24));
        assert!(!Timecode::new(1, 2, 3, 25).is_valid_at(25));
        assert!(Timecode::new(1, 2, 3, 29).is_valid_at(30));
        assert!(!Timecode::new(1, 2, 3, 30).is_plausible());
        assert!(!Timecode::new(1, 1, 0, 0)
            .with_drop_frame(true)
            .is_plausible());
        assert!(Timecode::new(1, 10, 0, 0)
            .with_drop_frame(true)
            .is_valid_at(30));
    }

    #[test]
    fn what_we_generate_carries_correct_parity() {
        // If our own encoder does not maintain the bit, our own decoder has no
        // business enforcing it. Every rate, because the bit moves at 25 fps.
        for (nominal, fps) in [(24u8, 24.0), (25, 25.0), (30, 30.0)] {
            let mut audio = Vec::new();
            Encoder::new().encode_sequence(
                sequence(Timecode::new(3, 20, 10, 5), 10, nominal, fps),
                &mut audio,
            );
            let mut decoder = Decoder::new(48_000.0, fps);
            let mut decoded = Vec::new();
            decoder.push_samples(&audio, &mut decoded);

            assert!(!decoded.is_empty(), "{fps} fps decoded nothing");
            assert!(
                decoded.iter().all(|frame| frame.parity_ok),
                "{fps} fps: our own frames fail their own parity check"
            );
            assert_eq!(decoder.source_respects_parity(), Some(true));
        }
    }

    #[test]
    fn rejects_digits_that_cannot_exist_in_bcd() {
        // Corrupt a single nibble into an impossible decimal digit and the
        // frame must be thrown away rather than passed on as a plausible time.
        // libltc would hand this one straight to the application.
        let mut bits = build_frame_bits(Timecode::new(1, 2, 3, 4), 25);
        bits &= !(0xFu128 << 16); // clear the seconds units
        bits |= 0xFu128 << 16; // ...and make it 15, which is not a digit
        assert!(decode_timecode(bits).is_none(), "impossible BCD accepted");

        // While a clean frame still decodes.
        let clean = build_frame_bits(Timecode::new(1, 2, 3, 4), 25);
        assert_eq!(decode_timecode(clean), Some(Timecode::new(1, 2, 3, 4)));
    }

    #[test]
    fn parity_notices_a_single_flipped_bit() {
        let bits = build_frame_bits(Timecode::new(5, 5, 5, 5), 30);
        assert!(parity_is_even(bits), "encoder failed to set parity");
        for bit in [0, 17, 33, 50, 63] {
            assert!(
                !parity_is_even(bits ^ (1u128 << bit)),
                "flipping bit {bit} went unnoticed"
            );
        }
    }

    #[test]
    fn timecode_is_formatted_the_way_the_trade_writes_it() {
        assert_eq!(Timecode::new(1, 2, 3, 4).to_string(), "01:02:03:04");
        assert_eq!(
            Timecode::new(1, 2, 3, 4).with_drop_frame(true).to_string(),
            "01:02:03;04"
        );
    }
}
