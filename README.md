# Chasefire

Chase timecode, fire cues.

Chasefire reads **SMPTE LTC** from a sound card (and, later, **MTC** from a MIDI
port), watches for the timecode values you have programmed, and fires **MIDI**,
**RTP-MIDI** and **OSC** at exactly those moments — so your mixer snapshots,
lighting cues and video clips land on the frame, every night, without an operator
holding their breath over a GO button.

It is meant for the machine you already own: no kernel drivers, no licence that
expires halfway through a tour, no phoning home.

> **Status: early, but it already does something useful.** LTC decoding, the cue
> engine and OSC output all work and are tested end to end. Live audio capture
> and the graphical interface are next, so for now it runs from the command line
> against a WAV file or its own generator.

## Try it now

No sound card required — generate a timecode file, point the cue list at your
media server, and watch it fire.

```bash
cargo build --release

# 25 seconds of 25 fps LTC to play with
./target/release/chasefire-cli gen test.wav --from 10:00:00:00 --fps 25 --seconds 25

# Decode it and fire the example cues at Resolume on this machine
./target/release/chasefire-cli wav test.wav \
    --cues examples/resolume.cues.json --fps 25 --osc 127.0.0.1:7000

# Or run a cue list in real time with no timecode source at all
./target/release/chasefire-cli simulate \
    --cues examples/resolume.cues.json --from 10:00:04:20 --osc 127.0.0.1:7000
```

Add `--dry-run` to see what would fire without sending anything, and `--offset N`
to fire *N* frames early, which is how you compensate for the latency of the
sound card, the network and whatever is on the receiving end.

## Why another one

There are good tools around this problem, but the open ones each solve a slice
of it — LTC to Program Change for one specific mixer, or LTC to OSC and nothing
else — and the complete ones are closed and rented by the year. Chasefire aims
at the combination nobody covers: **LTC and MTC in, a proper cue table, and MIDI
/ RTP-MIDI / OSC out**, with behaviour you can trust when the signal gets dirty.

RTP-MIDI is the interesting part. On Windows it currently means installing a
kernel driver that predates Windows 11 and breaks with security updates.
Chasefire speaks the protocol itself, so there is nothing to install and nothing
for Windows Update to take away.

## The rules that matter

Anyone can compare two numbers. What separates a tool an operator trusts from
one they switch off after the first show is the edge cases, so those are written
down as tests rather than discovered on stage:

- A cue fires when timecode **crosses** it, not when it exactly equals it — a
  dropped frame must never silently eat a cue.
- A **large jump is a seek, not a crossing.** Drag the playhead to the encore and
  the cues in between stay put instead of all going off at once.
- **Rewinding re-arms**, because that is what "from the top" means.
- **Nothing fires in reverse**, and **starting mid-show fires nothing**.
- Disarmed means nothing leaves the machine — and arming mid-show does not dump
  everything you walked past while it was off.

## Layout

```
crates/ltc      SMPTE LTC decoder and encoder. Pure DSP, no I/O.
crates/cue      The cue table and the firing rules. No sockets.
crates/sink     Where a fired cue goes out. OSC today; MIDI and RTP-MIDI next.
apps/chasefire-cli   Command line front end, and the simulator.
```

The crates are deliberately free of platform and application code. They are the
reusable core, meant to be lifted out into other tools later.

## Building

```bash
cargo test          # 25 tests, no hardware needed
cargo build --release
```

## Licence

GPL-3.0-or-later. The source is open and always will be. Ready-made signed
builds are what you pay for — once, not every year.
