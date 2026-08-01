# How it works, and why

The front page says what Chasefire does. This is the rest: what the window is
telling you, the edge cases the cue engine gets right, and the numbers those
were measured against rather than guessed at.

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

Not everyone wants a cartoon on the screen at work, so what turns up on its own
is the transport symbols the trade already reads without thinking — stop,
pause, play — animated through the same five states. Same information, same
rules, no cartoon. The resting mark is also the application's icon.

Pablo is one click away in Options, or `--pablo` on the command line, for
whoever wants him. He is worth having: peripheral vision reads a little man
who has fallen asleep far faster than it reads a symbol.

And when a cue fires the whole window flashes: green when it went out, **red
when it did not**, longer and stronger, because a cue that failed is the one
thing here worth interrupting somebody for.

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
crates/sink     Where a fired cue goes out: OSC, MIDI, MSC, RTP-MIDI, MTC.
crates/rtpmidi  RTP-MIDI (AppleMIDI) sessions, spoken here rather than driven.
crates/pablo    The little guitarist, and the rule that he cannot lie.
crates/show     All of the above, wired together in one place.
apps/chasefire       The window.
apps/chasefire-cli   Command line, simulator and measuring tools.
tools/               Building the sprite sheet from the artist's strips.
```

