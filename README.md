# Chasefire

Chase timecode, fire cues.

Chasefire reads **SMPTE LTC** from a sound card (and, later, **MTC** from a MIDI
port), watches for the timecode values you have programmed, and fires **MIDI**,
**RTP-MIDI** and **OSC** at exactly those moments — so your mixer snapshots,
lighting cues and video clips land on the frame, every night, without an operator
holding their breath over a GO button.

It is meant for the machine you already own: no kernel drivers, no licence that
expires halfway through a tour, no phoning home.

> **Status: early.** The LTC decoder is written and tested. Audio capture, the
> cue engine and the outputs are next. There is nothing to download yet.

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

## Layout

```
crates/ltc      SMPTE LTC decoder and encoder. Pure DSP, no I/O, fully tested.
```

The crates are deliberately free of any platform or application code. They are
the reusable core, and they are meant to be lifted out into other tools later.

## Building

```bash
cargo test          # run the test suite
cargo build --release
```

## Licence

GPL-3.0-or-later. The source is open and always will be. Ready-made signed
builds are what you pay for — once, not every year.
