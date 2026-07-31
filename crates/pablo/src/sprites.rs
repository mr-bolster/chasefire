//! Pablo, drawn.
//!
//! The art is a PNG sprite sheet baked into the binary with `include_bytes!`.
//! Ten frames of him at the time of writing come to under a kilobyte, so there
//! is nothing to be gained by being clever about it — and everything to be
//! gained by letting whoever draws him use a real drawing program and hand back
//! a normal image file.
//!
//! Baked in rather than shipped alongside, deliberately: no assets folder to
//! lose, no install path to get wrong, no "it works on my machine". One
//! executable, as promised.
//!
//! Frames are laid out left to right in a single row, each one a square the
//! height of the sheet. Which frames belong to which mood is [`FRAMES`].

use std::sync::OnceLock;

/// The two skins. Same frames, same order, same rules — one drawn as a little
/// guitarist, the other as the transport symbols everybody in this trade
/// already reads without thinking.
static PABLO_BYTES: &[u8] = include_bytes!("../assets/pablo.png");
static MARKS_BYTES: &[u8] = include_bytes!("../assets/marks.png");

/// The resting mark, for the window icon and anywhere else an icon is wanted.
pub static LOGO_BYTES: &[u8] = include_bytes!("../assets/logo.png");

/// Which frames of the sheet belong to which mood, in order.
pub mod frames {
    use std::ops::Range;
    pub const ASLEEP: Range<usize> = 0..8;
    pub const PYJAMAS: Range<usize> = 8..14;
    pub const PLAYING: Range<usize> = 14..22;
    pub const WOBBLING: Range<usize> = 22..28;
    pub const SHIVERING: Range<usize> = 28..34;
    pub const FLOURISH_MIDI: Range<usize> = 34..38;
    pub const FLOURISH_OSC: Range<usize> = 38..42;
    pub const FLOURISH_NETWORK: Range<usize> = 42..46;

    /// Everything, in the order `tools/build-sprite-sheet.py` lays it out.
    /// Change one and you must change the other; the script prints the ranges
    /// it produced so there is no excuse for guessing.
    pub const ALL: [Range<usize>; 8] = [
        ASLEEP,
        PYJAMAS,
        PLAYING,
        WOBBLING,
        SHIVERING,
        FLOURISH_MIDI,
        FLOURISH_OSC,
        FLOURISH_NETWORK,
    ];

    // Two animations sharing a frame would be a copy-paste slip, and this is
    // the sort of thing a compiler can check for free: it will not build if the
    // ranges ever overlap or leave a gap.
    const _: () = {
        assert!(ASLEEP.start == 0);
        assert!(ASLEEP.end == PYJAMAS.start);
        assert!(PYJAMAS.end == PLAYING.start);
        assert!(PLAYING.end == WOBBLING.start);
        assert!(WOBBLING.end == SHIVERING.start);
        assert!(SHIVERING.end == FLOURISH_MIDI.start);
        assert!(FLOURISH_MIDI.end == FLOURISH_OSC.start);
        assert!(FLOURISH_OSC.end == FLOURISH_NETWORK.start);
    };
}

#[derive(Debug)]
pub enum SheetError {
    Decode(String),
    /// The sheet is not a whole number of square frames.
    Ragged {
        width: u32,
        height: u32,
    },
    /// The sheet has fewer frames than the moods need.
    TooFewFrames {
        found: usize,
        needed: usize,
    },
}

impl std::fmt::Display for SheetError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Decode(message) => write!(f, "the sprite sheet will not decode: {message}"),
            Self::Ragged { width, height } => write!(
                f,
                "the sprite sheet is {width}x{height}, which is not a row of squares"
            ),
            Self::TooFewFrames { found, needed } => {
                write!(
                    f,
                    "the sprite sheet has {found} frames; {needed} are needed"
                )
            }
        }
    }
}

impl std::error::Error for SheetError {}

/// The decoded sheet: straight RGBA, ready to hand to whatever draws it.
pub struct Sheet {
    pixels: Vec<u8>,
    cell: usize,
    frame_count: usize,
}

impl Sheet {
    /// Decode one of the baked-in sheets. Cheap enough to do at startup.
    pub fn load(presentation: crate::Presentation) -> Result<Self, SheetError> {
        let bytes = match presentation {
            crate::Presentation::Pablo => PABLO_BYTES,
            crate::Presentation::Plain => MARKS_BYTES,
        };
        let decoder = png::Decoder::new(bytes);
        let mut reader = decoder
            .read_info()
            .map_err(|error| SheetError::Decode(error.to_string()))?;
        let mut buffer = vec![0; reader.output_buffer_size()];
        let info = reader
            .next_frame(&mut buffer)
            .map_err(|error| SheetError::Decode(error.to_string()))?;

        let (width, height) = (info.width, info.height);
        if height == 0 || width % height != 0 {
            return Err(SheetError::Ragged { width, height });
        }

        // Normalise whatever the artist's tool produced into plain RGBA, so the
        // rest of the program never has to care about colour types.
        let pixels = to_rgba(
            &buffer[..info.buffer_size()],
            info.color_type,
            info.bit_depth,
        )
        .ok_or_else(|| {
            SheetError::Decode(format!(
                "unsupported {:?} at {:?} bits",
                info.color_type, info.bit_depth
            ))
        })?;

        let cell = height as usize;
        let frame_count = (width / height) as usize;
        let needed = frames::FLOURISH_NETWORK.end;
        if frame_count < needed {
            return Err(SheetError::TooFewFrames {
                found: frame_count,
                needed,
            });
        }

        Ok(Self {
            pixels,
            cell,
            frame_count,
        })
    }

    /// The shared copy of a skin. Panics only if the baked-in art is broken,
    /// which is a build-time mistake and is caught by a test.
    pub fn shared(presentation: crate::Presentation) -> &'static Sheet {
        static PABLO: OnceLock<Sheet> = OnceLock::new();
        static MARKS: OnceLock<Sheet> = OnceLock::new();
        let slot = match presentation {
            crate::Presentation::Pablo => &PABLO,
            crate::Presentation::Plain => &MARKS,
        };
        slot.get_or_init(|| Sheet::load(presentation).expect("the art baked into this binary"))
    }

    /// Size of one frame, in pixels, both ways.
    pub fn cell(&self) -> usize {
        self.cell
    }

    pub fn frame_count(&self) -> usize {
        self.frame_count
    }

    /// Colour at a pixel of a frame, as red/green/blue/alpha.
    pub fn pixel(&self, frame: usize, x: usize, y: usize) -> [u8; 4] {
        if frame >= self.frame_count || x >= self.cell || y >= self.cell {
            return [0, 0, 0, 0];
        }
        let column = frame * self.cell + x;
        let offset = (y * self.cell * self.frame_count + column) * 4;
        [
            self.pixels[offset],
            self.pixels[offset + 1],
            self.pixels[offset + 2],
            self.pixels[offset + 3],
        ]
    }

    /// One frame as RGBA rows, for handing straight to a texture.
    pub fn frame_rgba(&self, frame: usize) -> Vec<u8> {
        let mut out = Vec::with_capacity(self.cell * self.cell * 4);
        for y in 0..self.cell {
            for x in 0..self.cell {
                out.extend_from_slice(&self.pixel(frame, x, y));
            }
        }
        out
    }
}

/// Widen whatever the PNG happened to be into RGBA.
fn to_rgba(data: &[u8], colour: png::ColorType, depth: png::BitDepth) -> Option<Vec<u8>> {
    if depth != png::BitDepth::Eight {
        return None;
    }
    Some(match colour {
        png::ColorType::Rgba => data.to_vec(),
        png::ColorType::Rgb => data
            .chunks_exact(3)
            .flat_map(|pixel| [pixel[0], pixel[1], pixel[2], 255])
            .collect(),
        png::ColorType::GrayscaleAlpha => data
            .chunks_exact(2)
            .flat_map(|pixel| [pixel[0], pixel[0], pixel[0], pixel[1]])
            .collect(),
        png::ColorType::Grayscale => data
            .iter()
            .flat_map(|value| [*value, *value, *value, 255])
            .collect(),
        // Indexed is expanded by the decoder before it reaches us.
        png::ColorType::Indexed => return None,
    })
}

/// Which frames to cycle through for a mood. Every mood has its own now.
pub fn frames_for(mood: crate::Mood) -> std::ops::Range<usize> {
    match mood {
        crate::Mood::Asleep => frames::ASLEEP,
        crate::Mood::Pyjamas => frames::PYJAMAS,
        crate::Mood::Playing => frames::PLAYING,
        crate::Mood::Wobbling => frames::WOBBLING,
        crate::Mood::Shivering => frames::SHIVERING,
    }
}

/// The frames of the burst that goes over him when a cue fires.
pub fn frames_for_flourish(flourish: crate::Flourish) -> std::ops::Range<usize> {
    match flourish {
        crate::Flourish::Midi => frames::FLOURISH_MIDI,
        crate::Flourish::Osc => frames::FLOURISH_OSC,
        crate::Flourish::NetworkMidi => frames::FLOURISH_NETWORK,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Mood, Presentation};

    #[test]
    fn both_skins_baked_into_this_binary_are_usable() {
        // If someone drops in a new sheet that is the wrong shape, this is
        // where they find out — at build time, not on a stage. Both skins get
        // checked, because the one nobody is looking at is the one that rots.
        for presentation in [Presentation::Pablo, Presentation::Plain] {
            let sheet = Sheet::load(presentation).expect("sheet should load");
            assert!(
                sheet.cell() >= 16,
                "{presentation:?} frames are {}px, too small",
                sheet.cell()
            );
            assert_eq!(
                sheet.frame_count(),
                frames::FLOURISH_NETWORK.end,
                "{presentation:?} has the wrong number of frames"
            );
        }
    }

    #[test]
    fn the_two_skins_are_actually_different_drawings() {
        // Same layout, same meaning, different art. If someone rebuilds one
        // sheet from the other's strips by mistake, this catches it.
        let pablo = Sheet::shared(Presentation::Pablo);
        let marks = Sheet::shared(Presentation::Plain);
        let different = (0..pablo.frame_count()).any(|frame| {
            (0..pablo.cell()).any(|y| {
                (0..pablo.cell()).any(|x| pablo.pixel(frame, x, y) != marks.pixel(frame, x, y))
            })
        });
        assert!(different, "both skins are the same picture");
    }

    #[test]
    fn every_mood_points_at_frames_that_exist() {
        let sheet = Sheet::shared(Presentation::Pablo);
        for mood in [
            Mood::Asleep,
            Mood::Pyjamas,
            Mood::Playing,
            Mood::Shivering,
            Mood::Wobbling,
        ] {
            let range = frames_for(mood);
            assert!(!range.is_empty(), "{mood:?} has no frames");
            assert!(
                range.end <= sheet.frame_count(),
                "{mood:?} wants frame {} of {}",
                range.end,
                sheet.frame_count()
            );
        }
    }

    #[test]
    fn no_frame_is_blank() {
        // A frame of nothing means the sheet slipped a column, which on screen
        // looks exactly like Pablo vanishing at random. Cheap to rule out.
        //
        // The bursts get a gentler floor on purpose: their first frame is meant
        // to be a couple of specks, and demanding a whole character's worth of
        // pixels there would be demanding the wrong drawing.
        let sheet = Sheet::shared(Presentation::Pablo);
        for frame in 0..sheet.frame_count() {
            let visible = (0..sheet.cell())
                .flat_map(|y| (0..sheet.cell()).map(move |x| (x, y)))
                .filter(|(x, y)| sheet.pixel(frame, *x, *y)[3] > 0)
                .count();
            let floor = if frame >= frames::FLOURISH_MIDI.start {
                1
            } else {
                200
            };
            assert!(
                visible >= floor,
                "frame {frame} has {visible} visible pixels, expected at least {floor}"
            );
        }
    }

    #[test]
    fn the_sheet_holds_exactly_what_the_ranges_claim() {
        // The script that builds the sheet and the ranges here have to agree.
        // They are edited in different files, in different languages, so the
        // only thing keeping them honest is this.
        let sheet = Sheet::shared(Presentation::Pablo);
        let claimed: usize = frames::ALL.iter().map(|range| range.len()).sum();
        assert_eq!(
            claimed,
            sheet.frame_count(),
            "the ranges account for {claimed} frames but the sheet holds {}",
            sheet.frame_count()
        );
    }

    #[test]
    fn every_burst_has_its_own_frames() {
        use crate::Flourish;
        let bursts = [Flourish::Midi, Flourish::Osc, Flourish::NetworkMidi];
        for (index, first) in bursts.iter().enumerate() {
            for second in &bursts[index + 1..] {
                assert_ne!(
                    frames_for_flourish(*first),
                    frames_for_flourish(*second),
                    "{first:?} and {second:?} draw the same thing"
                );
            }
        }
    }
}
