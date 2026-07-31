# Chasefire

Chase timecode, fire cues.

Chasefire reads **SMPTE LTC** from a sound card, watches for the timecode values
you have programmed, and fires **OSC** — with MIDI and RTP-MIDI next — at exactly
those moments. Mixer snapshots, lighting cues and video clips land on the frame,
every night, without an operator holding their breath over a GO button.

It runs on the machine you already own: no kernel drivers, no licence that
expires halfway through a tour, no phoning home.

> **Status: it works, and it is not finished.** Live capture, frame-rate
> detection, the cue engine and OSC output have all been proved against real
> hardware and a real media server. The settings screen is not built yet, so the
> window is configured from the command line. MIDI and RTP-MIDI are still to come.

## What it looks like

A small window you leave in a corner. Four things: whether the show is armed, a
way into the settings, the timecode, and Pablo.

Pablo is the little guitarist, and he is not decoration. At three in the morning
in a dark venue nobody reads the word "locked", but anyone notices out of the
corner of an eye whether the little man is playing or fast asleep. He is a status
display for peripheral vision, which is the only kind of attention an operator
has spare.

| Pablo | What is actually happening |
|---|---|
| Asleep, snot bubble, zzz | No timecode arriving |
| Awake but in pyjamas and a nightcap | Timecode running, **disarmed — nothing will fire** |
| Playing, nodding along | Locked and armed |
| Playing but shivering | Working, but the signal is close to the floor |
| Playing but unsteady, `?` overhead | Signal gone, freewheeling on our own count |

He cannot lie: a test walks every combination of armed, locked, freewheeling and
signal level, and fails if the face he pulls ever disagrees with whether a cue
would really go out.

Not everyone wants a cartoon on the screen at work, so `--sober` swaps him for
the transport symbols the trade already reads without thinking — stop, pause,
play — animated through the same five states. Same information, same rules, no
cartoon. The resting mark is also the application's icon.

And when a cue fires the whole window flashes: green when it went out, **red
when it did not**, longer and stronger, because a cue that failed is the one
thing here worth interrupting somebody for.

## Running it

```bash
cargo build --release

# List what this machine can hear
./target/release/chasefire-cli devices

# The window, reading a sound card and firing at a media server
./target/release/chasefire \
    --device "hw:CARD=CODEC,DEV=0" --channel 1 \
    --cues examples/resolume-columns.cues.json \
    --osc 192.168.1.50:7000
```

Arming is done with the button and only with the button. There is deliberately
no keyboard shortcut: the window sits above everything else, so it can take
focus without anyone noticing, and a stray keypress that silently disarms a
running show is a worse problem than having to aim at a button.

## Trying it with no hardware at all

```bash
# Write a WAV of clean LTC
./target/release/chasefire-cli gen test.wav --fps 25 --seconds 25

# Decode it and fire the cues
./target/release/chasefire-cli wav test.wav --cues examples/resolume.cues.json

# Or run a cue list in real time with no timecode source whatsoever
./target/release/chasefire-cli simulate --cues examples/resolume.cues.json

# And measure what your own sound card costs you, output looped to input
./target/release/chasefire-cli latency --out-device "..." --device "..."
```

## The rules that matter

Anyone can compare two numbers. What separates a tool an operator trusts from one
they switch off after the first show is the edge cases, so those are written down
as tests rather than discovered on stage.

- A cue fires when timecode **crosses** it, not when it exactly equals it — a
  dropped frame must never silently eat a cue.
- A **large jump is a seek, not a crossing.** Drag the playhead to the encore and
  the cues in between stay put instead of all going off at once.
- **Rewinding re-arms**, because that is what "from the top" means. **Nothing
  fires in reverse**, and **starting mid-show fires nothing.**
- Arming mid-show does not dump everything you walked past while it was off.
- LTC has no checksum, so a corrupted frame decodes into a plausible wrong time.
  Frames are checked for valid BCD, checked against the parity bit where the
  source maintains one, and **held back until a second frame confirms any jump**.
  One bad frame otherwise fires a cue early and then again at the right moment:
  one glitch, two triggers, and nothing in the cue list afterwards to explain it.
- When the signal drops, it **freewheels** for eight frames before admitting
  defeat — the professional norm is eight to forty.

Every one of those is a test, and several of them were written after the code
proved a comfortable assumption wrong.

## Measured, not guessed

On a real analogue loop — sound card out, cable, mic preamp, converter in:

- Clean decoding from **-53 dBFS peak up to hard clipping**. Clipping does no
  harm at all: biphase is essentially a square wave already.
- **Turning the preamp up buys nothing.** Signal and noise rise together; the
  signal-to-noise ratio stayed within 1 dB across eight gain settings. With LTC
  you want a clean feed, not a loud one.
- Corrupted frames start appearing below about **12 dB signal-to-noise**. Above
  16 dB, none at all. That threshold is what the level meter is calibrated to.
- Told the frame rate, the decoder locks on **the first frame** — the floor, since
  a frame is 80 bits and the sync word is the last 16. Left to work the rate out
  for itself, three frames.

## Layout

```
crates/ltc      SMPTE LTC decoder and encoder. Pure DSP, no I/O.
crates/chase    Decides which decoded frames to believe.
crates/cue      The cue table and the firing rules. No sockets.
crates/audio    Live capture and generation, decoded in the audio callback.
crates/sink     Where a fired cue goes out. OSC today.
crates/pablo    The little guitarist, and the rule that he cannot lie.
crates/show     All of the above, wired together in one place.
apps/chasefire       The window.
apps/chasefire-cli   Command line, simulator and measuring tools.
tools/               Building the sprite sheet from the artist's strips.
```

## Building

```bash
cargo test          # no hardware needed
cargo build --release
```

On Linux you will need ALSA's headers: `sudo apt install libasound2-dev`.

## Licence

GPL-3.0-or-later. The source is open and always will be. Ready-made signed
builds are what you pay for — once, not every year.
