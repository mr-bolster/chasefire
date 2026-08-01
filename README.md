**English** · [Español](README.es.md)

# Chasefire

Chase timecode, fire cues.

Chasefire follows timecode — **SMPTE LTC** off a sound card, or **MTC** off a
MIDI port with no sound card at all — and at the moments you programme it fires
**OSC, MIDI, MIDI Show Control** and **RTP-MIDI**. Mixer snapshots, lighting
cues and video clips land on the frame, every night, without anybody holding
their breath over a GO button.

It can also send the clock back out as **MTC**, so the same machine is the
converter between a rig with LTC on a cable and a device that only speaks MTC.

## The gap it fills

There is no shortage of software that *converts* timecode, and no shortage of
show controllers that play media. What there is no product for is the box in
between: **something that follows timecode and fires at everything else,
without pretending to be a media server.**

Today that job is done by chaining two applications together — a converter and
a control surface — or by building it yourself out of a toolkit. Every extra
link is another thing to boot, another clock, and another place the show can
fall over.

| | |
|---|---|
| Free converters (TXL20 and friends) | convert timecode; they do not fire cues |
| TimeLord | plays media and generates timecode |
| Show Cue System | a full show controller for Windows |
| QLab | the one everybody wants — macOS only |
| Chataigne | a toolkit: enormously capable, and you build it yourself |
| **Chasefire** | follows timecode, fires at everything, and does nothing else |

## What it talks to

Pick a preset and it writes a working cue, built from each manufacturer's own
documentation:

**Resolume · QLab · grandMA3 · grandMA2 (by MSC) · ChamSys MagicQ ·
Behringer X32/M32 · Behringer Wing · Waves SuperRack**

A cue is a **list of messages, each with its own destination**, because a
moment in a show is not a wire: the cue that starts the video also changes a
snapshot on the desk. QLab wants no arguments at all, grandMA3 wants a whole
command line as a string, and a Behringer Wing needs two messages in the right
order. All of it can be written down.

## What it has

- **In:** LTC off a sound card at 44.1 / 48 / 96 kHz, or MTC off a MIDI port.
  24, 25, 30, 50 and 60 fps, drop frame included.
- **Out:** OSC, MIDI, MSC, RTP-MIDI — several destinations at once, each with a
  name a cue can address.
- **Clock out:** MTC, properly paced, so a receiver can lock to it.
- **Offset** in frames, to cancel the latency of the card, the network and the
  far end. **Freewheel** for as long as you tell it.
- A small window you leave in a corner, in **English or Spanish**.
- Cue lists are plain JSON files you can read, diff and email.

## Get it

**[Downloads](https://github.com/mr-bolster/chasefire/releases)** — Windows and
Linux, nothing to install.

Windows will warn you that the publisher is unknown: these builds are not
code-signed yet. *More info* → *Run anyway*.

## Support it

**Nothing here costs money.** Not the program, not the builds, not an update,
not next year. There is no licence key, no trial, no expiry, and nothing
switched off if you never pay a penny.

It runs on the honour system. If Chasefire earns you money there is a
**Donate** button in Options — pay what you think it was worth, once, whenever
you like. That is the whole arrangement.

## Building it yourself

```bash
cargo test          # no hardware needed
cargo build --release
```

On Linux you need ALSA's headers: `sudo apt install libasound2-dev`.

The edge cases the cue engine gets right, and the numbers measured on real
hardware rather than guessed at, are in
[`docs/how-it-works.md`](docs/how-it-works.md).

## Licence

**The engine is MPL-2.0** — `ltc`, `cue`, `chase`, `audio`, `sink`, `rtpmidi`,
`show`: the decoder, the firing rules, the chaser and the outputs. Improvements
to those files stay open and can be used by anything.

**The program is GPL-3.0-or-later** — everything under `apps/`, and `pablo`,
which carries the artwork. Pablo and the transport marks were drawn by Claude to
a brief, examples and corrections from Leo Bolster.

### About patches

Please **open an issue rather than a pull request.** Not out of unfriendliness:
merged code belongs to whoever wrote it, and a handful of accepted lines can
permanently prevent the author from licensing his own work another way later.
Describe the problem, or the fix, and it will be written here and credited to
you in the commit.
