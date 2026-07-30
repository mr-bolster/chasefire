//! Chasefire on the command line.
//!
//! The sound card comes later. This binary exists so the whole chain —
//! timecode in, cue table, OSC out — can be pointed at a real receiver and
//! proved today, either from a WAV file of LTC or from an internal generator.
//! It doubles as the simulator every tool like this needs anyway: rehearsing a
//! cue list at eleven at night without dragging a timecode source into the room.

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
    --fps <n>         Frame rate: 24, 25, 30       [default: 25]
    --from <tc>       Start timecode, simulate only [default: 10:00:00:00]
    --offset <n>      Fire n frames early          [default: 0]
    --channel <n>     WAV channel holding the LTC  [default: 1]
    --seconds <n>     Length for gen               [default: 30]
    --dry-run         Print what would fire, send nothing
    -h, --help        This
";

struct Options {
    mode: Mode,
    cues: PathBuf,
    osc: String,
    fps: u8,
    from: Timecode,
    offset: i32,
    channel: usize,
    seconds: u32,
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

    let mut engine = Engine::new(options.fps);
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
    let sample_rate = 48_000.0;
    let frames = options.seconds * options.fps as u32;

    let mut audio = Vec::new();
    ltc::Encoder::new().encode_sequence(
        ltc::Sequence {
            start: options.from,
            count: frames,
            nominal_fps: options.fps,
            fps: options.fps as f64,
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
        "Wrote {} — {} s of {} fps LTC from {}",
        path.display(),
        options.seconds,
        options.fps,
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
        "Simulating {} fps from {}. Ctrl-C to stop.",
        options.fps, options.from
    );

    let frame_duration = Duration::from_secs_f64(1.0 / options.fps as f64);
    let started = Instant::now();
    let mut timecode = options.from;
    let mut elapsed_frames: u32 = 0;

    loop {
        for firing in engine.update(timecode, false) {
            report(&firing, output);
        }

        elapsed_frames += 1;
        timecode.advance_one_frame(options.fps);

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

    let mut decoder = Decoder::new(sample_rate as f64, options.fps as f64);
    let mut frames = Vec::new();
    decoder.push_samples(&mono, &mut frames);

    if frames.is_empty() {
        return Err("no LTC found — wrong channel, or the level is too low".into());
    }

    println!(
        "Decoded {} frames, {} to {}, about {:.2} fps",
        frames.len(),
        frames.first().unwrap().timecode,
        frames.last().unwrap().timecode,
        frames.last().unwrap().estimated_fps,
    );

    let mut fired = 0;
    for frame in &frames {
        for firing in engine.update(frame.timecode, frame.reverse) {
            report(&firing, output);
            fired += 1;
        }
    }
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
        fps: 25,
        from: Timecode::new(10, 0, 0, 0),
        offset: 0,
        channel: 1,
        seconds: 30,
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
            "--fps" => options.fps = value()?.parse().map_err(|_| "bad --fps".to_string())?,
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
