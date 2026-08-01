**Chase timecode, fire cues.**

Chasefire chases timecode — **SMPTE LTC** off a sound card or **MTC** off a
MIDI port — and at the values you programme it fires OSC, MIDI, MIDI Show
Control and RTP-MIDI. It can also send the clock back out as MTC, which turns
the same machine into the converter a rig with LTC on a cable and an MTC-only
device has been missing.

It runs on the machine you already own. No kernel drivers, no licence that
expires halfway through a tour, no phoning home.

## What it talks to

A preset writes a working cue for each of these, built from the manufacturers'
own documentation rather than from memory:

**Resolume · QLab · grandMA3 · grandMA2 (by MSC) · ChamSys MagicQ ·
Behringer X32/M32 · Behringer Wing · Waves SuperRack**

A cue is a *list* of messages, each with its own destination, because a moment
in a show is not a wire: the cue that starts the video also changes a snapshot
on the desk, and a Behringer Wing needs two messages in the right order to
recall one item. QLab wants no arguments at all; grandMA3 wants a whole command
line as a string. All of that can be written down.

## The rules that matter

Anyone can compare two numbers. These are the edge cases, and they are tests
rather than good intentions:

- A cue fires when timecode **crosses** it, not when it equals it — a dropped
  frame must never silently eat a cue.
- A **big jump is a seek**: drag the playhead to the encore and the cues in
  between stay put instead of all going off at once.
- **Rewinding re-arms.** Nothing fires in reverse. Starting mid-show fires
  nothing.
- LTC has no checksum, so a corrupt frame decodes into a plausible wrong time.
  Frames are checked for valid BCD, checked against the parity bit where the
  source keeps one, and **held back until a second frame confirms any jump**.
- When the signal goes it **freewheels** for eight frames before admitting it.

## Measured, not guessed

On a real analogue loop — sound card out, cable, mic preamp, converter in:
clean decoding from **−53 dBFS up to hard clipping**; turning the preamp up
buys nothing; corrupt frames start below about **12 dB signal-to-noise**. Told
the frame rate it locks on **the first frame**, and on the third if left to
work it out.

## Before you download

- **Windows will warn you.** These builds are not code-signed yet: SmartScreen
  will say the publisher is unknown. *More info* → *Run anyway*. Signing is
  being sorted out; until then there is no way around it with an unsigned
  binary, and anybody telling you otherwise is telling you to ignore a warning
  that is doing its job.
- **This is a prerelease.** It has been proved against real hardware — a real
  preamp, a real MIDI port, sockets that answer — but it has not yet been
  through a hundred nights in a hundred venues. Try it in rehearsal before you
  try it on a show.
- **Linux** needs ALSA at runtime; on Debian and Ubuntu that is already there.

## Two languages

English and Spanish, chosen in Options. Not a lookup table: every string is a
field of one struct that both languages have to fill in, so a missing
translation is a compile error rather than something found on a stage.

## Licence

The **engine** — decoder, firing rules, chaser, outputs — is **MPL-2.0**.
Improvements to those files stay open and can be used by anything. The
**program** is **GPL-3.0-or-later**.

Free, and it stays free. If it earns you money, there is a Donate button in
Options. Once, never a subscription.

## Not built yet

A control input so a surface can arm the show, and Art-Net timecode. Issues and
stories from real gigs are more useful than pull requests — see the README for
why.
