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
use std::collections::HashMap;
use std::path::PathBuf;
use std::time::{Duration, Instant};

const USAGE: &str = "\
chasefire — chase timecode, fire cues

USAGE:
    chasefire-cli simulate --cues <file> [options]
    chasefire-cli wav <file> --cues <file> [options]
    chasefire-cli gen <file> [options]
    chasefire-cli listen --cues <file> [options]
    chasefire-cli devices
    chasefire-cli pablo
    chasefire-cli latency --device <in> --out-device <out> [options]

MODES:
    simulate          Generate timecode internally, in real time
    wav <file>        Decode LTC from a WAV file as fast as it can
    gen <file>        Write a WAV of LTC to test with (--seconds, --from, --fps)
    listen            Read LTC live from a sound card and fire cues
    devices           List the audio inputs this machine has
    pablo             Draw the sprites in the terminal, every mood in turn
                      (--sober for the transport marks)
    latency           Measure the round trip: generate LTC out of one device,
                      read it back on another, and time the difference

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
    --channel <n>     Channel holding the LTC      [default: 1]
    --device <name>   Input device                 [default: system default]
    --out-device <n>  Output device for latency    [default: system default]
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
    device: Option<String>,
    out_device: Option<String>,
    dry_run: bool,
    /// Show the transport marks instead of the little guitarist.
    sober: bool,
}

enum Mode {
    Pablo,
    Latency,
    Listen,
    Devices,
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
    if matches!(options.mode, Mode::Devices) {
        return list_devices();
    }
    if matches!(options.mode, Mode::Latency) {
        return measure_latency(&options);
    }
    if matches!(options.mode, Mode::Pablo) {
        return show_pablo(if options.sober {
            pablo::Presentation::Plain
        } else {
            pablo::Presentation::Pablo
        });
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
        Mode::Listen => listen(&mut engine, &mut output, &options),
        Mode::Gen(_) | Mode::Devices | Mode::Latency | Mode::Pablo => {
            unreachable!("handled above")
        }
    }
}

fn show_pablo(presentation: pablo::Presentation) -> Result<(), String> {
    use pablo::Mood;
    let sheet = pablo::sprites::Sheet::load(presentation).map_err(|error| error.to_string())?;
    println!(
        "{presentation:?}: {} frames of {}x{}\n",
        sheet.frame_count(),
        sheet.cell(),
        sheet.cell()
    );

    for mood in [
        Mood::Asleep,
        Mood::Pyjamas,
        Mood::Playing,
        Mood::Wobbling,
        Mood::Shivering,
    ] {
        let range = pablo::sprites::frames_for(mood);
        println!("\x1b[1m── {mood:?} — {} ──\x1b[0m", mood.describe());
        for frame in range {
            for y in 0..sheet.cell() {
                let mut line = String::new();
                for x in 0..sheet.cell() {
                    let [red, green, blue, alpha] = sheet.pixel(frame, x, y);
                    if alpha < 32 {
                        line.push_str("  ");
                    } else {
                        line.push_str(&format!("\x1b[38;2;{red};{green};{blue}m██"));
                    }
                }
                line.push_str("\x1b[0m");
                println!("{line}");
            }
            println!("\x1b[0m  frame {frame}\n");
        }
    }
    Ok(())
}

fn list_devices() -> Result<(), String> {
    let devices = audio::list_input_devices().map_err(|error| error.to_string())?;
    println!("Audio inputs:");
    for device in devices {
        println!(
            "  {}{}  ({} ch, {} Hz)",
            device.name,
            if device.is_default { "  [default]" } else { "" },
            device.channels,
            device.sample_rate
        );
    }
    Ok(())
}

/// Measure how long a frame takes to get from the output to a decoded result.
///
/// Loop one device's output into another's input and this times the whole path:
/// output buffer, converter, cable, converter, input buffer, decode, and the
/// poll that collects it. That number is what the offset setting exists to
/// cancel, and measuring beats guessing.
fn measure_latency(options: &Options) -> Result<(), String> {
    let generator = audio::Generator::open(
        options.out_device.as_deref(),
        Timecode::new(10, 0, 0, 0),
        options.nominal_fps,
        options.fps,
        0.5,
    )
    .map_err(|error| format!("output: {error}"))?;

    let capture = audio::Capture::open(
        options.device.as_deref(),
        options.channel,
        Some(options.fps),
    )
    .map_err(|error| format!("input: {error}"))?;

    println!(
        "Out: \"{}\" at {} Hz\nIn:  \"{}\" channel {} at {} Hz\nMeasuring...\n",
        generator.device_name(),
        generator.sample_rate(),
        capture.device_name(),
        capture.channel(),
        capture.sample_rate()
    );

    // When each timecode was handed to the sound card, so the arrival of the
    // same value on the way back can be subtracted from it.
    let mut sent: HashMap<String, u64> = HashMap::new();
    let mut round_trips: Vec<f64> = Vec::new();
    let deadline = Instant::now() + Duration::from_secs(12);

    while Instant::now() < deadline && round_trips.len() < 200 {
        while let Some(emitted) = generator.next_emitted() {
            sent.insert(emitted.timecode.to_string(), emitted.at_nanos);
        }
        while let Some(frame) = capture.next_frame() {
            let arrived = generator.elapsed_nanos();
            if let Some(&departed) = sent.get(&frame.timecode.to_string()) {
                round_trips.push((arrived - departed) as f64 / 1.0e6);
            }
        }
        std::thread::sleep(Duration::from_millis(1));
    }

    if round_trips.len() < 10 {
        return Err(format!(
            "only matched {} frames — is the output really cabled to the input, \
             and is the level high enough?",
            round_trips.len()
        ));
    }

    round_trips.sort_by(f64::total_cmp);
    let median = round_trips[round_trips.len() / 2];
    let lowest = round_trips[0];
    let highest = round_trips[round_trips.len() - 1];
    let spread = highest - lowest;
    let frame_ms = 1000.0 / options.fps;

    let reported_in = capture.input_latency_ms();
    let reported_out = generator.output_latency_ms();

    println!("Matched {} frames", round_trips.len());
    println!("  fastest  {lowest:>7.1} ms");
    println!(
        "  median   {median:>7.1} ms   =  {:.1} frames at {:.2} fps",
        median / frame_ms,
        options.fps
    );
    println!("  slowest  {highest:>7.1} ms");
    println!("  spread   {spread:>7.1} ms");

    // The drivers themselves will say what each half costs, if the backend
    // supports it. Worth printing next to the measurement: if the two agree,
    // the reported figure can be trusted on machines with nothing patched.
    println!("\nWhat the drivers report:");
    println!("  input path   {reported_in:>7.1} ms");
    println!("  output path  {reported_out:>7.1} ms");
    let reported_total = reported_in + reported_out;
    if reported_total > 0.0 {
        println!(
            "  sum          {reported_total:>7.1} ms   vs {median:.1} ms measured \
             ({:+.1} ms out)",
            reported_total - median
        );
    }

    println!(
        "\nOffset for a loop like this one: {} frames",
        (median / frame_ms).round() as i32
    );
    if reported_in > 0.0 {
        println!(
            "Offset for timecode arriving from elsewhere: {} frames — only the \ninput path counts then.",
            (reported_in / frame_ms).round() as i32
        );
    }
    Ok(())
}

/// Read timecode live off a sound card and run the show from it.
fn listen(
    engine: &mut Engine,
    output: &mut Option<OscSink>,
    options: &Options,
) -> Result<(), String> {
    let requested_fps = options.fps_given.then_some(options.fps);
    let capture = audio::Capture::open(options.device.as_deref(), options.channel, requested_fps)
        .map_err(|error| error.to_string())?;

    println!(
        "Listening on \"{}\" channel {} at {} Hz{}",
        capture.device_name(),
        capture.channel(),
        capture.sample_rate(),
        if options.fps_given {
            format!(", expecting {:.2} fps", options.fps)
        } else {
            ", working the frame rate out from the signal".to_string()
        }
    );
    println!("Ctrl-C to stop.\n");

    let mut chaser = Chaser::new(engine.nominal_fps());
    let mut samples_per_frame = capture.sample_rate() as f64 / options.fps;
    let mut last_good_sample = capture.samples_processed();
    let mut freewheel_ticks = 0u64;
    let mut settled_rate = options.fps_given;
    let mut last_status = Instant::now();
    let mut current = None;

    loop {
        while let Some(frame) = capture.next_frame() {
            // The first frames tell us the rate; adopt it once and for all.
            if !settled_rate {
                if let Some(rate) = ltc::snap_to_known_frame_rate(frame.estimated_fps as f64) {
                    let nominal = rate.ceil() as u8;
                    println!("Locked: {rate:.2} fps, counting at {nominal}");
                    engine.set_nominal_fps(nominal);
                    chaser.set_nominal_fps(nominal);
                    samples_per_frame = capture.sample_rate() as f64 / rate;
                    settled_rate = true;
                }
            }

            last_good_sample = capture.samples_processed();
            freewheel_ticks = 0;
            if let Some(tick) = chaser.on_frame(&frame) {
                current = Some(tick.timecode);
                for firing in engine.update(tick.timecode, tick.reverse) {
                    report(&firing, output);
                }
            }
        }

        // Nothing arrived: work out from the audio clock how many frames have
        // gone by, and let the chaser count through them if it is willing.
        let elapsed = capture.samples_processed().saturating_sub(last_good_sample);
        let due = (elapsed as f64 / samples_per_frame) as u64;
        while freewheel_ticks < due {
            freewheel_ticks += 1;
            match chaser.on_missing_frame() {
                Some(tick) => {
                    current = Some(tick.timecode);
                    for firing in engine.update(tick.timecode, tick.reverse) {
                        report(&firing, output);
                    }
                }
                None => engine.signal_lost(),
            }
        }

        if last_status.elapsed() >= Duration::from_millis(500) {
            last_status = Instant::now();
            print_status(&capture, &chaser, engine, current);
        }

        std::thread::sleep(Duration::from_millis(2));
    }
}

fn print_status(
    capture: &audio::Capture,
    chaser: &Chaser,
    engine: &Engine,
    current: Option<Timecode>,
) {
    let level = match capture.level_dbfs() {
        Some(dbfs) => format!("{dbfs:>6.1} dBFS"),
        None => "  silent".to_string(),
    };
    // The threshold is measured, not guessed: on a real analogue loop, frames
    // start coming back corrupted once the signal drops towards the noise.
    let quality = match capture.level_dbfs() {
        Some(dbfs) if dbfs > -50.0 => "good",
        Some(dbfs) if dbfs > -55.0 => "WEAK",
        Some(_) => "TOO LOW",
        None => "no signal",
    };
    let state = match chaser.signal() {
        chase::Signal::Searching => "searching".to_string(),
        chase::Signal::Locked => "locked".to_string(),
        chase::Signal::Freewheeling { frames } => format!("freewheel {frames}"),
        chase::Signal::Lost => "LOST".to_string(),
    };
    let rejections = chaser.rejections();
    let retries = capture.detection_attempts();
    print!(
        "\r{}  {level} {quality:<8} {state:<13} pending {:<4} held {:<4} {:<12}",
        current
            .map(|timecode| timecode.to_string())
            .unwrap_or_else(|| "--:--:--:--".into()),
        engine.pending_count(),
        rejections.broke_continuity,
        if retries > 0 {
            format!("re-detect {retries}")
        } else {
            String::new()
        },
    );
    use std::io::Write;
    let _ = std::io::stdout().flush();
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
        device: None,
        out_device: None,
        dry_run: false,
        sober: false,
    };

    let mode = arguments.next().ok_or_else(|| USAGE.to_string())?;
    match mode.as_str() {
        "-h" | "--help" => {
            println!("{USAGE}");
            std::process::exit(0);
        }
        "simulate" => options.mode = Mode::Simulate,
        "listen" => options.mode = Mode::Listen,
        "devices" => options.mode = Mode::Devices,
        "latency" => options.mode = Mode::Latency,
        "pablo" => options.mode = Mode::Pablo,
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
            "--device" => options.device = Some(value()?),
            "--out-device" => options.out_device = Some(value()?),
            "--dry-run" => options.dry_run = true,
            "--sober" => options.sober = true,
            "-h" | "--help" => {
                println!("{USAGE}");
                std::process::exit(0);
            }
            other => return Err(format!("unknown option '{other}'\n\n{USAGE}")),
        }
    }

    if options.cues.as_os_str().is_empty()
        && !matches!(
            options.mode,
            Mode::Gen(_) | Mode::Devices | Mode::Latency | Mode::Pablo
        )
    {
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
