//! Chasefire on the command line.
//!
//! The sound card comes later. This binary exists so the whole chain —
//! timecode in, cue table, OSC out — can be pointed at a real receiver and
//! proved today, either from a WAV file of LTC or from an internal generator.
//! It doubles as the simulator every tool like this needs anyway: rehearsing a
//! cue list at eleven at night without dragging a timecode source into the room.

use chase::{Chaser, Source};
use cue::{Cue, Engine, Firing};
use ltc::{Decoder, Timecode};
use sink::{OscSink, Sink};
use std::path::PathBuf;
use std::time::{Duration, Instant};

const USAGE: &str = "\
chasefire — chase timecode, fire cues

USAGE:
    chasefire-cli simulate --cues <file> [options]
    chasefire-cli wav <file> --cues <file> [options]
    chasefire-cli gen <file> [options]

MODES:
    simulate          Generate timecode internally, in real time
    wav <file>        Decode LTC from a WAV file as fast as it can
    gen <file>        Write a WAV of LTC to test with (--seconds, --from, --fps)

OPTIONS:
    --cues <file>     Cue list, JSON (required)
    --osc <host:port> Where to send OSC            [default: 127.0.0.1:7000]
    --fps <n>         24, 25, 29.97, 30, 50, 59.94.
                      Left out, wav mode works it
                      out from the signal itself
    --from <tc>       Start timecode; write it with a
                      semicolon (10:00:00;00) for drop
                      frame numbering    [default: 10:00:00:00]
    --offset <n>      Fire n frames early          [default: 0]
    --channel <n>     WAV channel holding the LTC  [default: 1]
    --seconds <n>     Length for gen               [default: 30]
    --rate <hz>       Sample rate for gen:
                      44100, 48000, 96000          [default: 48000]
    --dry-run         Print what would fire, send nothing
    -h, --help        This
";

struct Options {
    mode: Mode,
    cues: PathBuf,
    osc: String,
    /// The rate the audio really runs at: 30000/1001 for 29.97.
    fps: f64,
    /// The rate the numbers are counted at: 30 for 29.97.
    nominal_fps: u8,
    /// False means "nobody told us, go and measure it".
    fps_given: bool,
    from: Timecode,
    offset: i32,
    channel: usize,
    seconds: u32,
    sample_rate: f64,
    dry_run: bool,
}

enum Mode {
    Simulate,
    Wav(PathBuf),
    Gen(PathBuf),
}

fn main() {
    if let Err(error) = run() {
        eprintln!("chasefire: {error}");
        std::process::exit(1);
    }
}

fn run() -> Result<(), String> {
    let options = parse_arguments()?;

    if let Mode::Gen(path) = &options.mode {
        return generate_wav(&options, path);
    }

    let cues = load_cues(&options.cues)?;
    println!("Loaded {} cues from {}", cues.len(), options.cues.display());

    let mut engine = Engine::new(options.nominal_fps);
    engine.set_cues(cues);
    engine.set_offset_frames(options.offset);
    engine.set_armed(true);

    let mut output: Option<OscSink> = if options.dry_run {
        println!("Dry run: nothing will be sent.");
        None
    } else {
        let sink = OscSink::connect(options.osc.as_str())
            .map_err(|error| format!("cannot open {}: {error}", options.osc))?;
        println!("Sending {}", sink.describe());
        Some(sink)
    };

    match &options.mode {
        Mode::Simulate => simulate(&mut engine, &mut output, &options),
        Mode::Wav(path) => decode_wav(&mut engine, &mut output, &options, path),
        Mode::Gen(_) => unreachable!("handled above"),
    }
}

/// Write a WAV of clean LTC: a test signal for this tool and for anything else.
fn generate_wav(options: &Options, path: &PathBuf) -> Result<(), String> {
    let sample_rate = options.sample_rate;
    let frames = (options.seconds as f64 * options.fps).round() as u32;

    let mut audio = Vec::new();
    ltc::Encoder::new().encode_sequence(
        ltc::Sequence {
            start: options.from,
            count: frames,
            nominal_fps: options.nominal_fps,
            fps: options.fps,
            sample_rate,
            amplitude: 0.5,
        },
        &mut audio,
    );

    let spec = hound::WavSpec {
        channels: 1,
        sample_rate: sample_rate as u32,
        bits_per_sample: 16,
        sample_format: hound::SampleFormat::Int,
    };
    let mut writer = hound::WavWriter::create(path, spec)
        .map_err(|error| format!("{}: {error}", path.display()))?;
    for sample in &audio {
        writer
            .write_sample((sample * i16::MAX as f32) as i16)
            .map_err(|error| format!("{}: {error}", path.display()))?;
    }
    writer
        .finalize()
        .map_err(|error| format!("{}: {error}", path.display()))?;

    println!(
        "Wrote {} — {} s of {:.2} fps LTC at {} Hz from {}",
        path.display(),
        options.seconds,
        options.fps,
        sample_rate as u32,
        options.from
    );
    Ok(())
}

/// Generate timecode internally and run the cue list against it in real time.
fn simulate(
    engine: &mut Engine,
    output: &mut Option<OscSink>,
    options: &Options,
) -> Result<(), String> {
    println!(
        "Simulating {:.2} fps from {}. Ctrl-C to stop.",
        options.fps, options.from
    );

    let frame_duration = Duration::from_secs_f64(1.0 / options.fps);
    let started = Instant::now();
    let mut timecode = options.from;
    let mut elapsed_frames: u32 = 0;

    loop {
        for firing in engine.update(timecode, false) {
            report(&firing, output);
        }

        elapsed_frames += 1;
        timecode.advance_one_frame(options.nominal_fps);

        // Sleep against the wall clock rather than accumulating drift by adding
        // up sleeps that are each a fraction of a millisecond too long.
        let target = frame_duration * elapsed_frames;
        if let Some(remaining) = target.checked_sub(started.elapsed()) {
            std::thread::sleep(remaining);
        }
    }
}

/// Read LTC out of a WAV file — a capture from a real show, or a generated one.
fn decode_wav(
    engine: &mut Engine,
    output: &mut Option<OscSink>,
    options: &Options,
    path: &PathBuf,
) -> Result<(), String> {
    let (sample_rate, channels, samples) = read_wav(path)?;
    let channel = options.channel.saturating_sub(1);
    if channel >= channels {
        return Err(format!(
            "asked for channel {} but the file has {channels}",
            options.channel
        ));
    }

    println!(
        "{}: {sample_rate} Hz, {channels} ch, {:.1} s — reading channel {}",
        path.display(),
        samples.len() as f64 / channels as f64 / sample_rate as f64,
        options.channel
    );

    let mono: Vec<f32> = samples
        .chunks(channels)
        .filter_map(|frame| frame.get(channel).copied())
        .collect();

    let mut decoder = if options.fps_given {
        Decoder::new(sample_rate as f64, options.fps)
    } else {
        Decoder::detecting(sample_rate as f64)
    };
    let mut frames = Vec::new();
    decoder.push_samples(&mono, &mut frames);

    if frames.is_empty() {
        return Err("no LTC found — wrong channel, or the level is too low".into());
    }

    if !options.fps_given {
        match decoder.detected_frame_rate() {
            Some(rate) => {
                let nominal = rate.ceil() as u8;
                println!("Detected {rate:.2} fps — counting at {nominal}");
                engine.set_nominal_fps(nominal);
            }
            // It decoded frames but the rate is not one anybody uses. Say so
            // rather than quietly counting at whatever the default was.
            None => println!(
                "Warning: measured {:.2} fps, which is not a standard rate. \
                 Counting at {}; pass --fps to override.",
                decoder.estimated_fps(),
                engine.nominal_fps()
            ),
        }
    }

    println!(
        "Decoded {} frames, {} to {}, about {:.2} fps",
        frames.len(),
        frames.first().unwrap().timecode,
        frames.last().unwrap().timecode,
        frames.last().unwrap().estimated_fps,
    );

    // Count frames that do not follow on from the one before. LTC has no
    // checksum, so a corrupted frame with an intact sync word decodes into a
    // perfectly plausible-looking wrong time. Counting the discontinuities is
    // how you tell a clean feed from one that is about to embarrass you.
    let mut discontinuities = 0;
    let mut previous: Option<Timecode> = None;
    for frame in &frames {
        if let Some(previous) = previous {
            let mut expected = previous;
            expected.advance_one_frame(engine.nominal_fps());
            if frame.timecode != expected {
                discontinuities += 1;
            }
        }
        previous = Some(frame.timecode);
    }
    if discontinuities > 0 {
        println!(
            "Warning: {discontinuities} of {} frames did not follow the one before ({:.2}%)",
            frames.len(),
            100.0 * discontinuities as f64 / frames.len() as f64
        );
    }

    // Everything the decoder produced now goes through the chaser, which
    // decides what is believable before the cue engine ever sees it.
    let mut chaser = Chaser::new(engine.nominal_fps());
    if decoder.source_respects_parity() == Some(true) {
        chaser.set_trust_parity(true);
        println!("Source maintains the parity bit — enforcing it");
    }

    let mut fired = 0;
    let mut freewheeled = 0;
    for frame in &frames {
        let Some(tick) = chaser.on_frame(frame) else {
            continue;
        };
        if tick.source == Source::Freewheeled {
            freewheeled += 1;
        }
        for firing in engine.update(tick.timecode, tick.reverse) {
            report(&firing, output);
            fired += 1;
        }
    }

    let rejections = chaser.rejections();
    println!(
        "Chaser: {} accepted, {} held back on continuity, {} on parity, {} seeks, {freewheeled} counted",
        rejections.accepted,
        rejections.broke_continuity,
        rejections.failed_parity,
        rejections.seeks
    );
    println!(
        "{fired} cues fired, {} still pending",
        engine.pending_count()
    );
    Ok(())
}

fn report(firing: &Firing, output: &mut Option<OscSink>) {
    let status = match output {
        None => "(dry run)".to_string(),
        Some(sink) => match sink.deliver(&firing.action) {
            Ok(()) => "sent".to_string(),
            // A cue that cannot go out is worth shouting about, but it must
            // never stop the ones after it.
            Err(error) => format!("FAILED: {error}"),
        },
    };
    println!(
        "{}  cue {} \"{}\"  programmed {}  {status}",
        firing.fired_at, firing.cue_id, firing.name, firing.at
    );
}

fn load_cues(path: &PathBuf) -> Result<Vec<Cue>, String> {
    let text =
        std::fs::read_to_string(path).map_err(|error| format!("{}: {error}", path.display()))?;
    serde_json::from_str(&text).map_err(|error| format!("{}: {error}", path.display()))
}

fn read_wav(path: &PathBuf) -> Result<(u32, usize, Vec<f32>), String> {
    let mut reader =
        hound::WavReader::open(path).map_err(|error| format!("{}: {error}", path.display()))?;
    let spec = reader.spec();

    let samples: Vec<f32> = match spec.sample_format {
        hound::SampleFormat::Float => reader.samples::<f32>().filter_map(Result::ok).collect(),
        hound::SampleFormat::Int => {
            let scale = 1.0 / (1i64 << (spec.bits_per_sample - 1)) as f32;
            reader
                .samples::<i32>()
                .filter_map(Result::ok)
                .map(|value| value as f32 * scale)
                .collect()
        }
    };

    Ok((spec.sample_rate, spec.channels as usize, samples))
}

fn parse_arguments() -> Result<Options, String> {
    let mut arguments = std::env::args().skip(1);
    let mut options = Options {
        mode: Mode::Simulate,
        cues: PathBuf::new(),
        osc: "127.0.0.1:7000".into(),
        fps: 25.0,
        nominal_fps: 25,
        fps_given: false,
        from: Timecode::new(10, 0, 0, 0),
        offset: 0,
        channel: 1,
        seconds: 30,
        sample_rate: 48_000.0,
        dry_run: false,
    };

    let mode = arguments.next().ok_or_else(|| USAGE.to_string())?;
    match mode.as_str() {
        "-h" | "--help" => {
            println!("{USAGE}");
            std::process::exit(0);
        }
        "simulate" => options.mode = Mode::Simulate,
        "gen" => {
            let path = arguments
                .next()
                .ok_or_else(|| "gen needs an output file".to_string())?;
            options.mode = Mode::Gen(PathBuf::from(path));
        }
        "wav" => {
            let path = arguments
                .next()
                .ok_or_else(|| "wav needs a file".to_string())?;
            options.mode = Mode::Wav(PathBuf::from(path));
        }
        other => return Err(format!("unknown mode '{other}'\n\n{USAGE}")),
    }

    while let Some(flag) = arguments.next() {
        let mut value = || {
            arguments
                .next()
                .ok_or_else(|| format!("{flag} needs a value"))
        };
        match flag.as_str() {
            "--cues" => options.cues = PathBuf::from(value()?),
            "--osc" => options.osc = value()?,
            "--fps" => {
                let (fps, nominal) = parse_frame_rate(&value()?)?;
                options.fps = fps;
                options.nominal_fps = nominal;
                options.fps_given = true;
            }
            "--from" => options.from = parse_timecode(&value()?)?,
            "--offset" => {
                options.offset = value()?.parse().map_err(|_| "bad --offset".to_string())?
            }
            "--channel" => {
                options.channel = value()?.parse().map_err(|_| "bad --channel".to_string())?
            }
            "--seconds" => {
                options.seconds = value()?.parse().map_err(|_| "bad --seconds".to_string())?
            }
            "--rate" => {
                options.sample_rate = value()?.parse().map_err(|_| "bad --rate".to_string())?
            }
            "--dry-run" => options.dry_run = true,
            "-h" | "--help" => {
                println!("{USAGE}");
                std::process::exit(0);
            }
            other => return Err(format!("unknown option '{other}'\n\n{USAGE}")),
        }
    }

    if options.cues.as_os_str().is_empty() && !matches!(options.mode, Mode::Gen(_)) {
        return Err(format!("--cues is required\n\n{USAGE}"));
    }
    Ok(options)
}

/// Accept the rates the trade actually says out loud, and work out both the
/// real rate and the counting rate from each. They only differ for the NTSC
/// ones, which is the whole reason drop-frame exists.
fn parse_frame_rate(text: &str) -> Result<(f64, u8), String> {
    Ok(match text {
        "23.98" | "23.976" => (24_000.0 / 1001.0, 24),
        "24" => (24.0, 24),
        "25" => (25.0, 25),
        "29.97" => (30_000.0 / 1001.0, 30),
        "30" => (30.0, 30),
        "50" => (50.0, 50),
        "59.94" => (60_000.0 / 1001.0, 60),
        "60" => (60.0, 60),
        other => return Err(format!("'{other}' is not a frame rate I know")),
    })
}

fn parse_timecode(text: &str) -> Result<Timecode, String> {
    // Accept either separator before the frames: a semicolon means drop-frame,
    // which is how the trade writes it and how our own display prints it.
    let drop_frame = text.contains(';');
    let parts: Vec<&str> = text.split([':', ';']).collect();
    if parts.len() != 4 {
        return Err(format!("'{text}' is not HH:MM:SS:FF"));
    }
    let mut numbers = [0u8; 4];
    for (index, part) in parts.iter().enumerate() {
        numbers[index] = part
            .parse()
            .map_err(|_| format!("'{text}' is not HH:MM:SS:FF"))?;
    }
    let timecode = Timecode {
        hours: numbers[0],
        minutes: numbers[1],
        seconds: numbers[2],
        frames: numbers[3],
        drop_frame,
    };
    if !timecode.is_plausible() {
        return Err(format!("'{text}' is out of range"));
    }
    Ok(timecode)
}
