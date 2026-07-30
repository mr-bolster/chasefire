//! Putting LTC out of a sound card, in real time.
//!
//! The mirror image of capture: the encoder runs inside the output callback,
//! filling whatever the card asks for, frame by frame. Nothing is allocated
//! once the stream is running — the frame buffer is sized on the first pass and
//! reused for ever after.
//!
//! It also notes, for each frame it hands to the card, the moment that frame
//! was handed over. That is what makes an honest latency measurement possible:
//! feed the output into an input, match the timecodes at both ends, and the
//! difference is the round trip through the converters, the cable and the
//! buffers — the delay an offset setting exists to cancel.

use crate::queue::Queue;
use crate::{AudioError, SampleFormat};
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use ltc::{Encoder, Timecode};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Instant;

/// A frame that has just been handed to the sound card, and when.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Emitted {
    pub timecode: Timecode,
    /// Nanoseconds since the generator started.
    pub at_nanos: u64,
}

struct Shared {
    emitted: Queue<Emitted, 256>,
    frames_sent: AtomicU64,
    /// Nanoseconds between the callback running and those samples reaching the
    /// converter, as reported by the backend.
    output_latency_nanos: AtomicU64,
}

/// A live LTC output. Dropping it stops the stream.
pub struct Generator {
    _stream: cpal::Stream,
    shared: Arc<Shared>,
    started: Instant,
    sample_rate: u32,
    device_name: String,
}

impl Generator {
    pub fn open(
        device_name: Option<&str>,
        start: Timecode,
        nominal_fps: u8,
        fps: f64,
        amplitude: f32,
    ) -> Result<Self, AudioError> {
        let host = cpal::default_host();
        let device = match device_name {
            Some(wanted) => host
                .output_devices()
                .map_err(|error| AudioError::Backend(error.to_string()))?
                .find(|device| device.name().map(|name| name == wanted).unwrap_or(false))
                .ok_or_else(|| AudioError::NoSuchDevice(wanted.to_string()))?,
            None => host.default_output_device().ok_or(AudioError::NoDevices)?,
        };

        let name = device.name().unwrap_or_else(|_| "unknown".into());
        let config = choose_output_config(&device)?;
        let format = config.sample_format();
        let config: cpal::StreamConfig = config.into();
        let channels = config.channels as usize;
        let sample_rate = config.sample_rate.0;

        let shared = Arc::new(Shared {
            emitted: Queue::new(),
            frames_sent: AtomicU64::new(0),
            output_latency_nanos: AtomicU64::new(0),
        });
        let started = Instant::now();

        let callback_shared = Arc::clone(&shared);
        let mut encoder = Encoder::new();
        let mut timecode = start;
        // Sized on the first frame and reused: no allocation on the audio
        // thread after that.
        let mut frame_samples: Vec<f32> = Vec::with_capacity(4096);
        let mut position = 0usize;

        let mut fill = move |output: &mut [f32], info: &cpal::OutputCallbackInfo| {
            let timestamp = info.timestamp();
            // How far ahead of the sound this callback is running.
            let ahead_nanos = timestamp
                .playback
                .duration_since(&timestamp.callback)
                .map(|delay| delay.as_nanos() as u64)
                .unwrap_or(0);
            callback_shared
                .output_latency_nanos
                .store(ahead_nanos, Ordering::Relaxed);

            let filled_at = started.elapsed().as_nanos() as u64;
            let nanos_per_sample = 1.0e9 / sample_rate as f64;

            for (written, slot) in output.chunks_mut(channels).enumerate() {
                if position >= frame_samples.len() {
                    frame_samples.clear();
                    encoder.encode_frame(
                        timecode,
                        nominal_fps,
                        fps,
                        sample_rate as f64,
                        amplitude,
                        &mut frame_samples,
                    );
                    position = 0;
                    // When this frame will actually leave the converter, not
                    // when it was encoded. The callback fills a whole buffer at
                    // once, so without this every frame in the buffer would
                    // claim the same instant while sounding tens of
                    // milliseconds apart — which is exactly the shape of the
                    // error the first version of this measurement produced.
                    callback_shared.emitted.push(Emitted {
                        timecode,
                        at_nanos: filled_at
                            + ahead_nanos
                            + (written as f64 * nanos_per_sample) as u64,
                    });
                    callback_shared.frames_sent.fetch_add(1, Ordering::Relaxed);
                    timecode.advance_one_frame(nominal_fps);
                }
                let sample = frame_samples[position];
                position += 1;
                // Same signal on every channel: whichever one the cable is on
                // carries timecode.
                for destination in slot.iter_mut() {
                    *destination = sample;
                }
            }
        };

        let on_error = |error| eprintln!("audio output error: {error}");

        let stream = match format {
            SampleFormat::F32 => device.build_output_stream(
                &config,
                move |data: &mut [f32], info| fill(data, info),
                on_error,
                None,
            ),
            SampleFormat::I16 => {
                let mut scratch = vec![0.0f32; 4096];
                device.build_output_stream(
                    &config,
                    move |data: &mut [i16], info| {
                        if scratch.len() < data.len() {
                            scratch.resize(data.len(), 0.0);
                        }
                        let window = &mut scratch[..data.len()];
                        fill(window, info);
                        for (destination, source) in data.iter_mut().zip(window.iter()) {
                            *destination = (source * 32_767.0) as i16;
                        }
                    },
                    on_error,
                    None,
                )
            }
            other => return Err(AudioError::Unsupported(format!("sample format {other:?}"))),
        }
        .map_err(|error| AudioError::Backend(error.to_string()))?;

        stream
            .play()
            .map_err(|error| AudioError::Backend(error.to_string()))?;

        Ok(Self {
            _stream: stream,
            shared,
            started,
            sample_rate,
            device_name: name,
        })
    }

    /// Take the record of the next frame handed to the card, if any.
    pub fn next_emitted(&self) -> Option<Emitted> {
        self.shared.emitted.pop()
    }

    /// Nanoseconds since this generator started — the same clock `Emitted` uses.
    pub fn elapsed_nanos(&self) -> u64 {
        self.started.elapsed().as_nanos() as u64
    }

    /// What the driver says the output path costs, in milliseconds.
    pub fn output_latency_ms(&self) -> f64 {
        self.shared.output_latency_nanos.load(Ordering::Relaxed) as f64 / 1.0e6
    }

    pub fn frames_sent(&self) -> u64 {
        self.shared.frames_sent.load(Ordering::Relaxed)
    }

    pub fn sample_rate(&self) -> u32 {
        self.sample_rate
    }

    pub fn device_name(&self) -> &str {
        &self.device_name
    }
}

fn choose_output_config(device: &cpal::Device) -> Result<cpal::SupportedStreamConfig, AudioError> {
    let ranges: Vec<_> = device
        .supported_output_configs()
        .map_err(|error| AudioError::Backend(error.to_string()))?
        .collect();

    let format_score = |format: SampleFormat| match format {
        SampleFormat::F32 => 4,
        SampleFormat::I16 => 3,
        _ => 0,
    };
    let rate_score = |rate: u32| match rate {
        48_000 => 3,
        44_100 => 2,
        96_000 => 1,
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

    match best {
        Some((_, config)) => Ok(config),
        None => device
            .default_output_config()
            .map_err(|error| AudioError::Backend(error.to_string())),
    }
}
