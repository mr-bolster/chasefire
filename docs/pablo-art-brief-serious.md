# Art brief: animated transport marks

A commission for a pixel artist, or a prompt for an image AI. Self-contained:
everything needed is here.

## What this is for

A small tool for live shows. It follows incoming timecode and fires cues at
exact moments, and it sits in the corner of an operator's screen for hours at a
time. What is needed is the little animated mark in that corner that says, at a
glance, what the software is doing.

It is read out of the corner of an eye, in a dark room, by someone doing three
other jobs. So the requirement is not that it looks nice up close. The
requirement is that **five states are unmistakable from across a room**, and that
two of them can never be confused.

## The idea

Use the transport symbols everyone in this trade already knows — play, stop,
pause and their relatives — but **animated**, one loop per state. Nobody has to
be taught what they mean; the whole vocabulary is already in the reader's head.

**One warning that shapes the drawing.** These marks are a *status display, not
a control*. If they look like buttons, people will try to press them, or assume
the software is transporting something — it is not, it follows somebody else's
clock. So: no button housings, no bevels, no rounded plates behind them, no
hover-looking highlights. A mark drawn on the background, not a control sitting
on a surface.

## Hard requirements

| | |
|---|---|
| Format | PNG with a real alpha channel |
| Frame size | **48 × 48 pixels**, exactly |
| Background | **Fully transparent**, alpha 0. Not white, not a colour key |
| Layout | One horizontal strip per animation: frames left to right, no gaps, no padding, no labels |
| Sheet width | Exactly `48 × number of frames`. Sheet height exactly 48 |
| Edges | **Hard pixel edges only.** No anti-aliasing, no soft edges, no gradients |
| Alignment | The mark occupies the same position in every frame. It must not drift within the cell |
| Loops | Every strip loops seamlessly: the last frame leads back into the first without a jolt |

## The five states, and their colours

These colours are fixed — the software already uses them elsewhere, so they have
to match.

| State | Colour | What it means | Suggested mark |
|---|---|---|---|
| `asleep` | `#5A5F6E` slate | Nothing arriving | Stop. At rest, dim, breathing slowly |
| `pyjamas` | `#D29628` amber | **Timecode running, but nothing will fire** | Pause. Alive and keeping up, visibly holding |
| `playing` | `#46C86E` green | Following, and cues will fire | Play. Steady, confident, obviously cyclic |
| `shivering` | `#C8BE3C` yellow | Working, but the input is marginal | Play, unsteady: jitter, flicker, an edge that will not settle |
| `wobbling` | `#E67832` orange | Signal gone, counting on its own | Play, adrift: ghosting, dashed, running free |

**Colour must never be the only difference between two states.** A fair number of
people in this trade cannot tell red from green, and the window may be dimmed.
The shape or the motion has to carry it as well.

### The one that matters most

`pyjamas` is the state that ruins shows. Timecode is arriving, the numbers on
screen are moving, everything looks healthy — and **nothing will go out**. The
operator glances over, relaxes, and the cue never fires.

So it must be impossible to mistake for `playing`, from any distance, at any
brightness, even by somebody who is not really looking. Alive but held. That
distinction is the single most important thing in this commission.

### The two that are nearly the same

`shivering` and `wobbling` are both "play, but something is wrong", and they must
still be tellable apart: one is a signal that is weak, the other is a signal that
has gone. Give them different *kinds* of wrongness — trembling in place versus
drifting away from where it should be — not just different amounts.

## The animations

Eight strips.

```
asleep.png       8 frames   →  384 × 48
pyjamas.png      6 frames   →  288 × 48
playing.png      8 frames   →  384 × 48
wobbling.png     6 frames   →  288 × 48
shivering.png    6 frames   →  288 × 48
```

Then three overlays, drawn on their own transparent strips, to be laid on top of
the current state for a moment when a cue fires. They are all "something just
went out", and the three must be tellable apart at a glance — that is the entire
reason there are three:

```
flourish-midi.png     4 frames  →  192 × 48   musical
flourish-osc.png      4 frames  →  192 × 48   data down a wire
flourish-network.png  4 frames  →  192 × 48   data through the air
```

Each burst grows and fades over its four frames and does not loop.

## And the logo

The same mark, still, is the application's icon and the logo on its website. So
design the resting pose first, as something worth putting on a page, and derive
the states from it.

```
logo.png        the resting mark, still, at 48 × 48
logo-large.png  the same design drawn properly at 512 × 512
```

`logo-large.png` is **not** an upscale of the sprite. It is the same idea drawn
at a size where it can carry a page on its own.

## What will get the work rejected

- Anti-aliased or blurry edges. True pixel art, every pixel placed.
- Any background that is not fully transparent.
- Frames not exactly 48×48, or strips with gaps or padding.
- The mark drifting position between frames of the same animation.
- Anything that looks like a pressable button.
- `pyjamas` that could be mistaken for `playing` at a glance.
- Text, logos, watermarks or drop shadows anywhere in the image.
