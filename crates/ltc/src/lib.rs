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

use std::fmt;

/// Sync word as it appears once a whole frame has been shifted in, playing forward.
const SYNC_FORWARD: u16 = 0xBFFC;
/// The same sync word seen when the signal is running backwards.
const SYNC_REVERSE: u16 = 0x3FFD;

/// Number of bits in one LTC frame.
const FRAME_BITS: u32 = 80;

/// A timecode value: hours, minutes, seconds, frames.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
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

    /// True if every field is inside the range the SMPTE spec allows.
    pub fn is_plausible(&self) -> bool {
        self.hours < 24 && self.minutes < 60 && self.seconds < 60 && self.frames < 60
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
        if self.drop_frame && self.minutes % 10 != 0 {
            self.frames = 2;
        }
        if self.minutes < 60 {
            return;
        }
        self.minutes = 0;
        self.hours = (self.hours + 1) % 24;
    }

    /// Position within the day, in whole frames, ignoring drop-frame numbering.
    pub fn as_frame_count(&self, nominal_fps: u32) -> u32 {
        ((self.hours as u32 * 60 + self.minutes as u32) * 60 + self.seconds as u32) * nominal_fps
            + self.frames as u32
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
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct DecodedFrame {
    pub timecode: Timecode,
    /// The signal is running backwards (rewind, or a scrubbing operator).
    pub reverse: bool,
    /// The 32 user bits, in case anyone ever wants the date stamp in them.
    pub user_bits: u32,
    /// Frame rate implied by the measured bit period.
    pub estimated_fps: f32,
    /// Index of the sample that closed this frame, counted from the first
    /// sample ever pushed. This is what lets a cue fire with a known offset
    /// instead of "whenever the GUI thread got round to it".
    pub end_sample: u64,
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
            register: 0,
            bits_filled: 0,
            sample_index: 0,
            samples_since_frame: 0,
        }
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

        let timecode = decode_timecode(payload);
        if !timecode.is_plausible() {
            return None;
        }

        let estimated_fps = (self.sample_rate / (FRAME_BITS as f64 * self.bit_period)) as f32;
        self.samples_since_frame = 0;

        Some(DecodedFrame {
            timecode,
            reverse,
            user_bits: decode_user_bits(payload),
            estimated_fps,
            end_sample: self.sample_index,
        })
    }

    /// Best current estimate of the incoming frame rate.
    pub fn estimated_fps(&self) -> f64 {
        self.sample_rate / (FRAME_BITS as f64 * self.bit_period)
    }

    /// Samples elapsed since the last good frame — how a caller notices the
    /// timecode has gone away and it is time to freewheel.
    pub fn samples_since_last_frame(&self) -> u64 {
        self.samples_since_frame
    }
}

fn field(payload: u128, start: u32, len: u32) -> u32 {
    ((payload >> start) & ((1u128 << len) - 1)) as u32
}

fn decode_timecode(payload: u128) -> Timecode {
    Timecode {
        frames: (field(payload, 0, 4) + field(payload, 8, 2) * 10) as u8,
        seconds: (field(payload, 16, 4) + field(payload, 24, 3) * 10) as u8,
        minutes: (field(payload, 32, 4) + field(payload, 40, 3) * 10) as u8,
        hours: (field(payload, 48, 4) + field(payload, 56, 2) * 10) as u8,
        drop_frame: field(payload, 10, 1) == 1,
    }
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
fn build_frame_bits(timecode: Timecode) -> u128 {
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
        fps: f64,
        sample_rate: f64,
        amplitude: f32,
        out: &mut Vec<f32>,
    ) {
        let bits = build_frame_bits(timecode);
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
        let mut timecode = spec.start;
        let mut written = Vec::with_capacity(spec.count as usize);
        for _ in 0..spec.count {
            self.encode_frame(timecode, spec.fps, spec.sample_rate, spec.amplitude, out);
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
    fn timecode_is_formatted_the_way_the_trade_writes_it() {
        assert_eq!(Timecode::new(1, 2, 3, 4).to_string(), "01:02:03:04");
        assert_eq!(
            Timecode::new(1, 2, 3, 4).with_drop_frame(true).to_string(),
            "01:02:03;04"
        );
    }
}
