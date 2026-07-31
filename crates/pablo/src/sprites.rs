//! Pablo, drawn.
//!
//! The art lives here as text, one character per pixel, on purpose. It can be
//! edited in any editor by anyone, it shows up properly in a diff, and nobody
//! needs a sprite tool to move his fringe. When someone who can actually draw
//! takes a pass at him, the shapes get replaced and not a line of logic moves.
//!
//! Current art is a placeholder by someone who cannot draw. It is deliberately
//! honest about that.
//!
//! ```text
//!   .  transparent      #  hair          o  skin
//!   e  eye              T  shirt         A  arm
//!   G  guitar body      N  guitar neck   |  string
//!   P  pyjamas          ^  nightcap      b  snot bubble
//!   z  zzz              ?  question mark
//! ```

/// One frame of Pablo: rows of pixels, top to bottom.
pub type Frame = &'static [&'static str];

/// Slumped over the guitar, out cold. The bubble swells across the loop and
/// pops at the end, which is the whole reason anyone will look at this window.
pub const ASLEEP: &[Frame] = &[
    &[
        "..........z.....",
        ".........z......",
        "................",
        "....######......",
        "...########.....",
        "...#oooooo#.....",
        "...#o.oo.o#..b..",
        "...#oooooo#.....",
        "....#oooo#......",
        "...TTTTTTTT.....",
        "..TTTTTTTTTT....",
        "..ATTTTTTTTA....",
        "...NGGGGGGN.....",
        "...GGGGGGGG.....",
        "....GGGGGG......",
        "................",
    ],
    &[
        ".........z......",
        "........z.......",
        "................",
        "....######......",
        "...########.....",
        "...#oooooo#.....",
        "...#o.oo.o#.bb..",
        "...#oooooo#.bb..",
        "....#oooo#......",
        "...TTTTTTTT.....",
        "..TTTTTTTTTT....",
        "..ATTTTTTTTA....",
        "...NGGGGGGN.....",
        "...GGGGGGGG.....",
        "....GGGGGG......",
        "................",
    ],
    &[
        "........z.......",
        ".......z........",
        "................",
        "....######......",
        "...########.....",
        "...#oooooo#.....",
        "...#o.oo.o#bbb..",
        "...#oooooo#bbb..",
        "....#oooo#.bbb..",
        "...TTTTTTTT.....",
        "..TTTTTTTTTT....",
        "..ATTTTTTTTA....",
        "...NGGGGGGN.....",
        "...GGGGGGGG.....",
        "....GGGGGG......",
        "................",
    ],
    // Pop.
    &[
        "................",
        "................",
        "................",
        "....######......",
        "...########.....",
        "...#oooooo#.....",
        "...#o.oo.o#.b.b.",
        "...#oooooo#..b..",
        "....#oooo#.b.b..",
        "...TTTTTTTT.....",
        "..TTTTTTTTTT....",
        "..ATTTTTTTTA....",
        "...NGGGGGGN.....",
        "...GGGGGGGG.....",
        "....GGGGGG......",
        "................",
    ],
];

/// Awake, guitar on, nightcap still on his head. Timecode is running and he
/// knows it — he is simply not on duty, and you can see that from the door.
pub const PYJAMAS: &[Frame] = &[
    &[
        ".....^^^........",
        "....^^^^^.......",
        "...^^^^^^^......",
        "...##oooo##.....",
        "...#oooooo#.....",
        "...#oeooeo#.....",
        "...#oooooo#.....",
        "....#oooo#......",
        "...PPPPPPPP.....",
        "..PPPPPPPPPP....",
        "..APPPPPPPPA....",
        "...NGGGGGG......",
        "..NGGGGGGGG.....",
        "...PPPPPPPP.....",
        "...PP....PP.....",
        "...oo....oo.....",
    ],
    &[
        "......^^^.......",
        ".....^^^^^......",
        "....^^^^^^^.....",
        "...##oooo##.....",
        "...#oooooo#.....",
        "...#oeooeo#.....",
        "...#oooooo#.....",
        "....#oooo#......",
        "...PPPPPPPP.....",
        "..PPPPPPPPPP....",
        "..APPPPPPPPA....",
        "...NGGGGGG......",
        "..NGGGGGGGG.....",
        "...PPPPPPPP.....",
        "...PP....PP.....",
        "...oo....oo.....",
    ],
];

/// Locked, armed, playing. The head nods and the strumming arm goes round; the
/// window drives the nod from the timecode itself, one beat a second, so a
/// glance tells you not just that it is running but that it is running right.
pub const PLAYING: &[Frame] = &[
    &[
        "................",
        "....######......",
        "...########.....",
        "...#oooooo#....N",
        "...#oeooeo#...N.",
        "...#oooooo#..N..",
        "....#oooo#..N...",
        "...TTTTTTTT.....",
        "..TTTTTTTTTT....",
        "..ATTTTTTTTA....",
        "...GGGGGGGG.....",
        "..GGGGGGGGGG....",
        "...GGGGGGGG.....",
        "...TT....TT.....",
        "...TT....TT.....",
        "...oo....oo.....",
    ],
    &[
        "....######......",
        "...########.....",
        "...#oooooo#....N",
        "...#oeooeo#...N.",
        "...#oooooo#..N..",
        "....#oooo#..N...",
        "...TTTTTTTT.....",
        "..TTTTTTTTTT....",
        "..ATTTTTTTTA....",
        "...GGGGGGGG.....",
        "..GGGGGGGGGG....",
        "...GGGGGGGG.....",
        "...TT....TT.....",
        "...TT....TT.....",
        "..oo......oo....",
        "................",
    ],
    &[
        "................",
        "....######......",
        "...########.....",
        "...#oooooo#....N",
        "...#oeooeo#...N.",
        "...#oooooo#..N..",
        "....#oooo#..N...",
        "...TTTTTTTT.....",
        "..TTTTTTTTTT....",
        "...TTTTTTTTA....",
        "..AGGGGGGGG.....",
        "..GGGGGGGGGG....",
        "...GGGGGGGG.....",
        "...TT....TT.....",
        "...TT....TT.....",
        "...oo....oo.....",
    ],
    &[
        "....######......",
        "...########.....",
        "...#oooooo#....N",
        "...#oeooeo#...N.",
        "...#oooooo#..N..",
        "....#oooo#..N...",
        "...TTTTTTTT.....",
        "..TTTTTTTTTT....",
        "..ATTTTTTTTA....",
        "...GGGGGGGG.....",
        "..GGGGGGGGGG....",
        "...GGGGGGGG.....",
        "...TT....TT.....",
        "..TT......TT....",
        "..oo......oo....",
        "................",
    ],
];

/// Still playing, but the timecode just vanished and we are counting on our
/// own. Same body, question mark over his head — he is carrying on, and he
/// wants you to know he is doing it from memory.
pub const WOBBLING_MARK: &[&str] = &["......?.........", "................"];

/// Pick the frames for a mood. Shivering borrows the playing frames and is
/// jittered by the window instead of costing more art.
pub fn frames_for(mood: crate::Mood) -> &'static [Frame] {
    match mood {
        crate::Mood::Asleep => ASLEEP,
        crate::Mood::Pyjamas => PYJAMAS,
        crate::Mood::Playing | crate::Mood::Shivering | crate::Mood::Wobbling => PLAYING,
    }
}

/// Every frame is this many pixels across and down. Checked by a test, because
/// a ragged sprite sheet is the kind of thing nobody notices until it is drawn
/// on screen at four in the morning.
pub const WIDTH: usize = 16;
pub const HEIGHT: usize = 16;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Mood;

    #[test]
    fn every_frame_is_the_same_size() {
        let sets = [
            ("asleep", ASLEEP),
            ("pyjamas", PYJAMAS),
            ("playing", PLAYING),
        ];
        for (name, frames) in sets {
            for (index, frame) in frames.iter().enumerate() {
                assert_eq!(
                    frame.len(),
                    HEIGHT,
                    "{name} frame {index} is the wrong height"
                );
                for (row, line) in frame.iter().enumerate() {
                    assert_eq!(
                        line.chars().count(),
                        WIDTH,
                        "{name} frame {index} row {row} is the wrong width"
                    );
                }
            }
        }
    }

    #[test]
    fn every_mood_has_something_to_show() {
        for mood in [
            Mood::Asleep,
            Mood::Pyjamas,
            Mood::Playing,
            Mood::Shivering,
            Mood::Wobbling,
        ] {
            assert!(!frames_for(mood).is_empty(), "{mood:?} has no frames");
        }
    }

    #[test]
    fn only_known_characters_are_used() {
        // A stray character would silently draw nothing, which is exactly the
        // sort of bug that only shows up on the night.
        const LEGEND: &str = ".#oeTAGN|P^bz?";
        for frames in [ASLEEP, PYJAMAS, PLAYING] {
            for frame in frames {
                for line in frame.iter() {
                    for character in line.chars() {
                        assert!(
                            LEGEND.contains(character),
                            "'{character}' is not in the legend"
                        );
                    }
                }
            }
        }
    }
}
