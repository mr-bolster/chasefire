//! Getting LTC in from a real sound card.
//!
//! The decoding happens **inside the audio callback**, not after it. That is
//! deliberate: a frame is then timed by the sample that completed it rather
//! than by whenever a worker thread woke up, and the only thing crossing
//! between threads is a handful of decoded frames a second instead of a
//! torrent of samples. What crosses does so through [`queue::Queue`], which
//! never blocks and never allocates, because the audio thread cannot afford
//! either.

pub mod generator;
pub mod queue;

pub use generator::Generator;

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::SampleFormat;
use ltc::{DecodedFrame, Decoder};
use queue::Queue;
use std::fmt;
use std::sync::atomic::{AtomicU32, AtomicU64, Ordering};
use std::sync::Arc;

#[derive(Debug)]
pub enum AudioError {
    NoSuchDevice(String),
    NoDevices,
    Unsupported(String),
    /// Something else already has this input open.
    ///
    /// Worth its own case rather than being left inside a backend string. It is
    /// one of the most common ways this fails in real life — a DAW left running,
    /// a meter app, another copy of this program — and the backend's own words
    /// for it are "ALSA function 'snd_pcm_open' failed with error 'EBUSY'",
    /// which tells an operator nothing they can act on.
    Busy(String),
    Backend(String),
}

impl fmt::Display for AudioError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            // Deliberately covers both cases, because from here they are
            // indistinguishable: ALSA drops a device from the enumeration
            // entirely while another program holds it, so a card that is merely
            // busy looks exactly like a card that was never there. Saying only
            // "no such device" sends people looking for a cable when the real
            // answer is a DAW they left running.
            Self::NoSuchDevice(name) => write!(
                f,
                "'{name}' is not available — it may be unplugged, or another program may have it open"
            ),
            Self::NoDevices => write!(f, "no audio input devices at all"),
            Self::Unsupported(what) => write!(f, "the device cannot do that: {what}"),
            Self::Busy(name) => write!(
                f,
                "'{name}' is already in use by another program — close whatever has it, or choose another input"
            ),
            Self::Backend(message) => write!(f, "audio backend: {message}"),
        }
    }
}

impl std::error::Error for AudioError {}

/// What an operator needs to see when choosing where the timecode comes in.
#[derive(Debug, Clone)]
pub struct DeviceInfo {
    pub name: String,
    pub channels: u16,
    pub sample_rate: u32,
    pub is_default: bool,
}

pub fn list_input_devices() -> Result<Vec<DeviceInfo>, AudioError> {
    let host = cpal::default_host();
    let default_name = host
        .default_input_device()
        .and_then(|device| device.name().ok());

    let devices = host
        .input_devices()
        .map_err(|error| AudioError::Backend(error.to_string()))?;

    let mut found = Vec::new();
    for device in devices {
        let Ok(name) = device.name() else { continue };
        let Ok(config) = device.default_input_config() else {
            continue;
        };
        found.push(DeviceInfo {
            is_default: Some(&name) == default_name.as_ref(),
            name,
            channels: config.channels(),
            sample_rate: config.sample_rate().0,
        });
    }

    if found.is_empty() {
        return Err(AudioError::NoDevices);
    }
    Ok(found)
}

/// Backends do not agree on a code for "somebody else has it", but they all say
/// the word. Cheap to check, and it turns an unreadable line into an instruction.
fn looks_busy(message: &str) -> bool {
    let message = message.to_lowercase();
    message.contains("busy") || message.contains("in use") || message.contains("ebusy")
}

/// Pick a sample rate the device can really do, preferring the ones LTC lives at.
///
/// The obvious `default_input_config` is not good enough: a cheap interface
/// will happily *report* 44.1 kHz as its default while its capture side only
/// runs at 48 — ask for the default and you either fail to open or end up
/// quietly resampled. Asking what it actually supports and choosing costs
/// nothing and avoids both.
fn choose_input_config(device: &cpal::Device) -> Result<cpal::SupportedStreamConfig, AudioError> {
    let ranges: Vec<_> = device
        .supported_input_configs()
        .map_err(|error| AudioError::Backend(error.to_string()))?
        .collect();

    // Rate first, then resolution. Taking the first entry that merely *fits* is
    // how you end up capturing timecode in 8 bits: this very interface offers
    // an 8-bit mode alongside its 16-bit one, and lists it just as eagerly.
    let rate_score = |rate: u32| match rate {
        48_000 => 3,
        44_100 => 2,
        96_000 => 1,
        _ => 0,
    };
    let format_score = |format: SampleFormat| match format {
        SampleFormat::F32 => 4,
        SampleFormat::I16 => 3,
        SampleFormat::U16 => 2,
        _ => 0,
    };

    let mut best: Option<(i32, cpal::SupportedStreamConfig)> = None;
    for range in &ranges {
        if format_score(range.sample_format()) == 0 {
            continue;
        }
        for wanted in [48_000u32, 44_100, 96_000] {
            if range.min_sample_rate().0 > wanted || wanted > range.max_sample_rate().0 {
                continue;
            }
            let score = rate_score(wanted) * 10 + format_score(range.sample_format());
            if best.as_ref().map(|(top, _)| score > *top).unwrap_or(true) {
                best = Some((score, range.with_sample_rate(cpal::SampleRate(wanted))));
            }
        }
    }

    if let Some((_, config)) = best {
        return Ok(config);
    }

    device
        .default_input_config()
        .map_err(|error| AudioError::Backend(error.to_string()))
}

/// Shared between the audio callback and everyone else.
struct Shared {
    frames: Queue<DecodedFrame, 64>,
    /// Signal envelope, as the bits of an f32, for the level meter.
    level_bits: AtomicU32,
    samples: AtomicU64,
    /// Times the callback found the queue full — nobody is collecting frames.
    overruns: AtomicU64,
    /// Nanoseconds between a sample being captured and the callback that
    /// carries it running. This is the delay the driver itself admits to, and
    /// it is per-device — which is what makes automatic offset calibration
    /// possible without asking anyone to patch a cable.
    input_latency_nanos: AtomicU64,
    /// Times the decoder has had to work the frame rate out again. Climbing
    /// means what is on the cable is not behaving like timecode.
    detection_attempts: AtomicU32,
}

/// A live LTC input.
///
/// Dropping this stops the stream.
pub struct Capture {
    _stream: cpal::Stream,
    shared: Arc<Shared>,
    sample_rate: u32,
    device_name: String,
    channel: usize,
}

impl Capture {
    /// Open an input and start decoding.
    ///
    /// `channel` is one-based, the way it is written on the front of the box.
    /// `nominal_fps` of `None` means work it out from the signal, which is what
    /// an operator plugging into someone else's rig actually wants.
    pub fn open(
        device_name: Option<&str>,
        channel: usize,
        nominal_fps: Option<f64>,
    ) -> Result<Self, AudioError> {
        let host = cpal::default_host();
        let device = match device_name {
            Some(wanted) => host
                .input_devices()
                .map_err(|error| AudioError::Backend(error.to_string()))?
                .find(|device| device.name().map(|name| name == wanted).unwrap_or(false))
                .ok_or_else(|| AudioError::NoSuchDevice(wanted.to_string()))?,
            None => host.default_input_device().ok_or(AudioError::NoDevices)?,
        };

        let name = device.name().unwrap_or_else(|_| "unknown".into());
        let config = choose_input_config(&device)?;
        let format = config.sample_format();
        let config: cpal::StreamConfig = config.into();

        let channels = config.channels as usize;
        if channel == 0 || channel > channels {
            return Err(AudioError::Unsupported(format!(
                "channel {channel} on a {channels}-channel input"
            )));
        }
        let offset = channel - 1;
        let sample_rate = config.sample_rate.0;

        let shared = Arc::new(Shared {
            frames: Queue::new(),
            level_bits: AtomicU32::new(0),
            samples: AtomicU64::new(0),
            overruns: AtomicU64::new(0),
            input_latency_nanos: AtomicU64::new(0),
            detection_attempts: AtomicU32::new(0),
        });

        let mut decoder = match nominal_fps {
            Some(fps) => Decoder::new(sample_rate as f64, fps),
            None => Decoder::detecting(sample_rate as f64),
        };

        // Everything below runs on the audio thread. No allocation, no locking,
        // no formatting, no logging. Decode, publish, return.
        let callback_shared = Arc::clone(&shared);
        let mut envelope = 0.0f32;
        let mut process = move |samples: &[f32], info: &cpal::InputCallbackInfo| {
            let timestamp = info.timestamp();
            if let Some(delay) = timestamp.callback.duration_since(&timestamp.capture) {
                callback_shared
                    .input_latency_nanos
                    .store(delay.as_nanos() as u64, Ordering::Relaxed);
            }
            let mut count = 0u64;
            for frame in samples.chunks(channels) {
                let Some(&sample) = frame.get(offset) else {
                    continue;
                };
                count += 1;

                let magnitude = sample.abs();
                envelope = if magnitude > envelope {
                    magnitude
                } else {
                    // About a 200 ms fall at 48 kHz: slow enough to read.
                    envelope * 0.9999
                };

                if let Some(decoded) = decoder.push_sample(sample) {
                    if !callback_shared.frames.push(decoded) {
                        callback_shared.overruns.fetch_add(1, Ordering::Relaxed);
                    }
                }
            }
            callback_shared
                .level_bits
                .store(envelope.to_bits(), Ordering::Relaxed);
            callback_shared
                .detection_attempts
                .store(decoder.detection_attempts(), Ordering::Relaxed);
            callback_shared.samples.fetch_add(count, Ordering::Relaxed);
        };

        let on_error = |error| {
            // Nothing clever to do from here; the level meter going flat is
            // what the operator will actually notice.
            eprintln!("audio stream error: {error}");
        };

        let stream = match format {
            SampleFormat::F32 => device.build_input_stream(
                &config,
                move |data: &[f32], info| process(data, info),
                on_error,
                None,
            ),
            SampleFormat::I16 => {
                let mut scratch = vec![0.0f32; 4096];
                device.build_input_stream(
                    &config,
                    move |data: &[i16], info| {
                        // Converting in place into a buffer sized once, up
                        // front: no allocation happens here.
                        if scratch.len() < data.len() {
                            scratch.resize(data.len(), 0.0);
                        }
                        for (destination, &source) in scratch.iter_mut().zip(data) {
                            *destination = source as f32 / 32_768.0;
                        }
                        process(&scratch[..data.len()], info);
                    },
                    on_error,
                    None,
                )
            }
            SampleFormat::U16 => {
                let mut scratch = vec![0.0f32; 4096];
                device.build_input_stream(
                    &config,
                    move |data: &[u16], info| {
                        if scratch.len() < data.len() {
                            scratch.resize(data.len(), 0.0);
                        }
                        for (destination, &source) in scratch.iter_mut().zip(data) {
                            *destination = (source as f32 - 32_768.0) / 32_768.0;
                        }
                        process(&scratch[..data.len()], info);
                    },
                    on_error,
                    None,
                )
            }
            other => {
                return Err(AudioError::Unsupported(format!("sample format {other:?}")));
            }
        }
        .map_err(|error| {
            let message = error.to_string();
            if looks_busy(&message) {
                AudioError::Busy(name.clone())
            } else {
                AudioError::Backend(message)
            }
        })?;

        stream.play().map_err(|error| {
            let message = error.to_string();
            if looks_busy(&message) {
                AudioError::Busy(name.clone())
            } else {
                AudioError::Backend(message)
            }
        })?;

        Ok(Self {
            _stream: stream,
            shared,
            sample_rate,
            device_name: name,
            channel,
        })
    }

    /// Take the next decoded frame, if one is waiting.
    pub fn next_frame(&self) -> Option<DecodedFrame> {
        self.shared.frames.pop()
    }

    /// Current signal envelope, 0.0 to 1.0. This is what a level meter shows.
    pub fn level(&self) -> f32 {
        f32::from_bits(self.shared.level_bits.load(Ordering::Relaxed))
    }

    /// Level in dBFS, or `None` when there is effectively nothing there.
    pub fn level_dbfs(&self) -> Option<f32> {
        let level = self.level();
        (level > 1.0e-6).then(|| 20.0 * level.log10())
    }

    pub fn sample_rate(&self) -> u32 {
        self.sample_rate
    }

    pub fn device_name(&self) -> &str {
        &self.device_name
    }

    pub fn channel(&self) -> usize {
        self.channel
    }

    /// Samples processed since the stream opened. The audio clock, in effect —
    /// and the only honest way to know how much time has really passed.
    pub fn samples_processed(&self) -> u64 {
        self.shared.samples.load(Ordering::Relaxed)
    }

    /// Times the frame rate has had to be worked out again from scratch.
    pub fn detection_attempts(&self) -> u32 {
        self.shared.detection_attempts.load(Ordering::Relaxed)
    }

    /// What the driver says the input path costs, in milliseconds.
    ///
    /// Zero means the backend does not report it — some do not, and an honest
    /// zero is better than a number made up to look tidy.
    pub fn input_latency_ms(&self) -> f64 {
        self.shared.input_latency_nanos.load(Ordering::Relaxed) as f64 / 1.0e6
    }

    /// Frames thrown away because the main loop was not collecting them.
    /// Should be zero; anything else means the program is too busy.
    pub fn overruns(&self) -> u64 {
        self.shared.overruns.load(Ordering::Relaxed)
    }
}
