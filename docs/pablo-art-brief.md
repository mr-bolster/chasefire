# Art brief: Pablo

A commission for a pixel artist, or a prompt for an image AI. Everything under
**Hard requirements** is a constraint, not a suggestion — the art is loaded by
code that slices a sheet on an exact grid, so a sheet that is off by a pixel is
a sheet that does not work.

## What this is for

Chasefire is a small tool for live shows. It reads timecode and fires cues, and
it sits in the corner of an operator's screen for hours at a time. Pablo is the
character in that corner. He is not decoration: at three in the morning in a
dark venue nobody reads the word "locked", but anyone notices out of the corner
of an eye whether the little man is playing his guitar or fast asleep. He is a
status display for peripheral vision.

Which means every pose has to be **unmistakable at a glance and at small size**.
If two states can be confused when the window is small and the room is dark, the
drawing has failed no matter how nice it looks up close.

## The character

Pablo. A young man with **short dark hair**, who **plays the guitar**. Friendly,
a bit scruffy, entirely at home slumped over an amplifier at 4am. Chibi
proportions — big head, small body — because that is what reads at this size.
Think NES and SNES era character sprites, not modern high-resolution pixel art.

His guitar is with him in every single frame. It is his defining prop.

## Hard requirements

| | |
|---|---|
| Format | PNG with a real alpha channel |
| Frame size | **48 × 48 pixels**, exactly |
| Background | **Fully transparent**, alpha 0. Not white, not a colour key |
| Layout | One horizontal strip per animation: frames left to right, no gaps, no padding, no labels |
| Sheet width | Exactly `48 × number of frames`. Sheet height exactly 48 |
| Edges | **Hard pixel edges only.** No anti-aliasing, no soft edges, no semi-transparent pixels except where noted |
| Outline | Every solid shape gets a **1 pixel dark outline** in `#241E1B`. The window sits over an unknown desktop, so he has to hold up against any background |
| Palette | Only the colours below. No gradients, no dithering beyond simple 2-colour patterns |
| Alignment | Pablo occupies the same position in every frame of an animation. He must not drift or wander within the cell |

## Palette

Use these and nothing else.

```
#241E1B  outline / eyes        #3C2A21  hair, dark
#5A3E2B  hair, highlight       #F0C8A8  skin
#D9A47F  skin, shadow          #2E5A82  shirt
#1E3D5C  shirt, shadow         #A85C28  guitar body
#6E4624  guitar, dark / neck   #D9964B  guitar, highlight
#CED6F0  pyjamas               #9AA5C8  pyjamas, shadow
#E896A0  nightcap              #A8D6E0  snot bubble
#E8ECF2  white / zzz           #F0C83C  accent yellow
```

The snot bubble is the one place partial transparency is welcome: around 70%
alpha so it reads as a bubble rather than a blob.

## The animations

Eight strips. Each one loops seamlessly — the last frame must lead back into the
first without a jolt.

### 1. `asleep` — 8 frames

Nothing is coming in and he has given up waiting. Slumped down, sitting, guitar
across his lap, eyes closed, head drooping. His chest rises and falls slowly
across the loop.

A **snot bubble** grows from one nostril: barely there on frame 1, biggest on
frame 6, **bursts on frame 7**, gone on frame 8. This is the detail people will
watch the window for, so give it room.

Two or three `z` letters drift up and away from his head across the loop, in
`#E8ECF2`.

### 2. `pyjamas` — 6 frames

Awake, standing, guitar strapped on — **but wearing pyjamas and a nightcap**,
and not playing. He knows there is work happening and he is not doing it.

This state means *the operator has disarmed the software and nothing will fire*,
so it must be impossible to confuse with him playing. Same character, obviously
off duty. A gentle sway, and a yawn somewhere in the loop.

### 3. `playing` — 8 frames

The good state. Standing, guitar up, **strumming**, head nodding, one foot
tapping. Eyes open, pleased with himself. Energetic and unmistakably in motion —
this is what a working show looks like and it should look like fun.

### 4. `wobbling` — 6 frames

The timecode just vanished and he is carrying on from memory. Still playing, but
unsteady — leaning, glancing around, uncertain. A **`?` in `#F0C83C` floating
above his head**, appearing and fading over the loop.

### 5. `shivering` — 6 frames

Playing, but the signal is weak and about to give trouble. Same pose as
`playing`, trembling: shift him one pixel left and right between frames, and
give him a slightly worried face. Everything still works — he just does not
trust it.

### 6. `flourish-midi` — 4 frames

Not Pablo. An effect drawn **on its own transparent 48×48 strip**, to be
overlaid on top of him when a cue fires: two or three **musical notes** bursting
out and upward from where the guitar would be, growing and fading over 4 frames.

### 7. `flourish-osc` — 4 frames

Same idea, different shape: small **square data packets** flying out to the
right, as if down a wire. Growing and fading over 4 frames.

### 8. `flourish-network` — 4 frames

Same again: data packets with a small **wireless arc** behind them, going up and
to the right. Growing and fading over 4 frames.

## What will get the work rejected

- Anti-aliased or blurry edges. This must be true pixel art, every pixel placed.
- Any background that is not fully transparent.
- Frames that are not exactly 48×48, or a strip with gaps or padding between them.
- The character drifting position between frames of the same animation.
- Colours outside the palette, or gradients.
- His hair, skin tone, shirt or guitar changing between frames or between
  animations. **Consistency across the whole set is the single most important
  thing after the grid being right.** If it helps, draw one reference pose first,
  lock it down, and derive every other frame from it.
- Text, logos, watermarks or drop shadows anywhere in the image.

## Delivering

Eight PNG files, named exactly:

```
asleep.png            8 frames    →  384 × 48
pyjamas.png           6 frames    →  288 × 48
playing.png           8 frames    →  384 × 48
wobbling.png          6 frames    →  288 × 48
shivering.png         6 frames    →  288 × 48
flourish-midi.png     4 frames    →  192 × 48
flourish-osc.png      4 frames    →  192 × 48
flourish-network.png  4 frames    →  192 × 48
```

Plus, if it is easy: a single reference image of Pablo standing still, at any
size, so there is something to check the sprites against.

Total is 46 frames. At this size the whole set comes to a few kilobytes, so
there is no reason to be sparing with the animation — smooth, characterful loops
are the entire point.
