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

/// The sheet itself. Replace the file and the app picks it up on rebuild;
/// [`Sheet::load`] checks it is laid out the way the code expects.
static SHEET_BYTES: &[u8] = include_bytes!("../assets/pablo.png");

/// Which frames of the sheet belong to which mood, in order.
pub mod frames {
    use std::ops::Range;
    pub const ASLEEP: Range<usize> = 0..4;
    pub const PYJAMAS: Range<usize> = 4..6;
    pub const PLAYING: Range<usize> = 6..10;

    // Two moods sharing a frame would be a copy-paste slip, and this is the
    // sort of thing a compiler can check for free. It will not build if the
    // ranges ever overlap.
    const _: () = {
        assert!(ASLEEP.end <= PYJAMAS.start);
        assert!(PYJAMAS.end <= PLAYING.start);
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
    /// Decode the baked-in sheet. Cheap enough to do at startup and then keep.
    pub fn load() -> Result<Self, SheetError> {
        let decoder = png::Decoder::new(SHEET_BYTES);
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
        let needed = frames::PLAYING.end;
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

    /// The one everybody shares. Panics only if the baked-in art is broken,
    /// which is a build-time mistake and is caught by a test.
    pub fn shared() -> &'static Sheet {
        static SHEET: OnceLock<Sheet> = OnceLock::new();
        SHEET.get_or_init(|| Sheet::load().expect("the sprite sheet baked into this binary"))
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

/// Which frames to cycle through for a mood.
///
/// Shivering and wobbling borrow the playing frames — one is that same
/// animation jittered, the other is it with a question mark over his head —
/// so neither costs the artist anything.
pub fn frames_for(mood: crate::Mood) -> std::ops::Range<usize> {
    match mood {
        crate::Mood::Asleep => frames::ASLEEP,
        crate::Mood::Pyjamas => frames::PYJAMAS,
        crate::Mood::Playing | crate::Mood::Shivering | crate::Mood::Wobbling => frames::PLAYING,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Mood;

    #[test]
    fn the_art_baked_into_this_binary_is_usable() {
        // If someone drops in a new sheet that is the wrong shape, this is
        // where they find out — at build time, not on a stage.
        let sheet = Sheet::load().expect("sheet should load");
        assert!(
            sheet.cell() >= 16,
            "frames are {}px, too small",
            sheet.cell()
        );
        assert!(sheet.frame_count() >= frames::PLAYING.end);
    }

    #[test]
    fn every_mood_points_at_frames_that_exist() {
        let sheet = Sheet::shared();
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
        // A frame of nothing means the sheet slipped a column, which looks
        // exactly like Pablo vanishing at random. Cheap to rule out.
        let sheet = Sheet::shared();
        for frame in 0..sheet.frame_count() {
            let visible = (0..sheet.cell())
                .flat_map(|y| (0..sheet.cell()).map(move |x| (x, y)))
                .filter(|(x, y)| sheet.pixel(frame, *x, *y)[3] > 0)
                .count();
            assert!(visible > 20, "frame {frame} is empty or nearly so");
        }
    }
}
