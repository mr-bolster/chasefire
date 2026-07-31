//! Pablo.
//!
//! Dark hair, short, plays guitar, lives in the corner of your screen for six
//! hours at a stretch. He is not decoration: at three in the morning in a dark
//! FOH nobody reads the word "locked", but everyone notices out of the corner
//! of an eye whether the little man is playing or asleep. He is a status
//! display for peripheral vision, which is the only kind of attention an
//! operator has spare.
//!
//! Which means the one rule that matters: **Pablo must never lie.** If he is
//! playing, timecode is genuinely locked and cues will genuinely fire. The day
//! he plays while nothing would fire, he stops being useful and becomes a
//! dangerous joke. Every mood below maps onto real state, and the tests at the
//! bottom exist to keep it that way.
//!
//! The trap worth naming: *timecode running, everything looks fine, but
//! disarmed*. That is the state that ruins a show — the operator glances over,
//! sees the numbers rolling, relaxes, and nothing ever goes out. So that is not
//! "playing". That is [`Mood::Pyjamas`]: awake, guitar on, still in his
//! nightcap. Alive, visibly off duty.

pub mod sprites;

/// Everything Pablo needs to know about the outside world.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Situation {
    /// Cues would actually go out if one came due.
    pub armed: bool,
    /// Timecode is arriving and being believed.
    pub locked: bool,
    /// The signal has gone but we are still counting, for now.
    pub freewheeling: bool,
    /// Input level in dBFS, if there is any signal at all to measure.
    pub level_dbfs: Option<f32>,
}

impl Situation {
    /// Below this the decoder starts letting corrupted frames through. Not a
    /// guess: measured on a real analogue loop, sound card out through a mic
    /// preamp and back in. Above it, no bad frames at all.
    pub const WEAK_DBFS: f32 = -50.0;
}

/// What Pablo is doing, and therefore what is actually happening.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mood {
    /// Nothing coming in. Slumped over the guitar, zzz, snot bubble swelling
    /// and popping.
    Asleep,
    /// Timecode is running and he is awake — but disarmed, so still in his
    /// pyjamas and nightcap. Nothing will go out and you can see it.
    Pyjamas,
    /// Locked, armed, signal healthy. Playing, nodding, tapping his foot.
    Playing,
    /// Playing, but the level is close to where frames start coming back
    /// wrong. Shivering, sprite flickering.
    Shivering,
    /// The signal just died and we are coasting on our own count. Still
    /// playing, but looking around with a question mark over his head.
    Wobbling,
}

impl Mood {
    /// Read the situation. This is the only place a mood is ever decided.
    pub fn read(situation: Situation) -> Self {
        // Order matters, and it is deliberate. Freewheeling is checked before
        // anything about levels, because "the signal just vanished" is more
        // urgent than "the signal is quiet". And disarmed beats everything
        // except being asleep: the whole point is that it cannot be mistaken
        // for a healthy show.
        if !situation.locked && !situation.freewheeling {
            return Mood::Asleep;
        }
        if !situation.armed {
            return Mood::Pyjamas;
        }
        if situation.freewheeling {
            return Mood::Wobbling;
        }
        match situation.level_dbfs {
            Some(level) if level < Situation::WEAK_DBFS => Mood::Shivering,
            _ => Mood::Playing,
        }
    }

    /// True only when a cue coming due right now would really go out.
    ///
    /// This is the promise the animation makes to whoever is watching, and
    /// [`Mood::read`] is tested against it.
    pub fn implies_cues_will_fire(self) -> bool {
        matches!(self, Mood::Playing | Mood::Shivering | Mood::Wobbling)
    }

    /// Frames per second for this mood's loop. Sleeping is slow and breathy;
    /// playing is brisk. Nothing here needs to be smooth — it needs to be
    /// readable at a glance from two metres away.
    pub fn animation_fps(self) -> f32 {
        match self {
            Mood::Asleep => 4.0,
            Mood::Pyjamas => 6.0,
            Mood::Playing => 12.0,
            Mood::Shivering => 12.0,
            Mood::Wobbling => 10.0,
        }
    }

    /// A word for the tooltip, and for the log.
    pub fn describe(self) -> &'static str {
        match self {
            Mood::Asleep => "no timecode",
            Mood::Pyjamas => "disarmed — nothing will fire",
            Mood::Playing => "locked and armed",
            Mood::Shivering => "locked, but the signal is weak",
            Mood::Wobbling => "signal lost, freewheeling",
        }
    }
}

/// What Pablo throws out of the guitar when a cue fires. Different shapes for
/// different destinations, so you can tell from across the room whether that
/// was the lights or the video.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Flourish {
    /// Musical notes.
    Midi,
    /// Little packets flying off down a wire.
    Osc,
    /// Packets, but with an aerial.
    NetworkMidi,
}

/// A cue firing, on screen for a moment and then gone.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Strum {
    pub flourish: Flourish,
    /// Seconds left before it fades. Counted down by the window.
    pub remaining: f32,
}

impl Strum {
    /// Long enough to register out of the corner of an eye, short enough that
    /// a run of quick cues does not turn into a smear.
    pub const DURATION: f32 = 0.45;

    pub fn new(flourish: Flourish) -> Self {
        Self {
            flourish,
            remaining: Self::DURATION,
        }
    }

    /// Advance by `delta` seconds. Returns false once it is over.
    pub fn tick(&mut self, delta: f32) -> bool {
        self.remaining -= delta;
        self.remaining > 0.0
    }

    /// 1.0 at the moment of firing, falling to 0.0 as it fades.
    pub fn intensity(&self) -> f32 {
        (self.remaining / Self::DURATION).clamp(0.0, 1.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn situation(armed: bool, locked: bool, freewheeling: bool, level: f32) -> Situation {
        Situation {
            armed,
            locked,
            freewheeling,
            level_dbfs: Some(level),
        }
    }

    #[test]
    fn pablo_never_lies() {
        // The rule, brute-forced over every combination there is: whenever
        // Pablo looks like a working show, a cue really would fire. And
        // whenever one really would, he does not look asleep or off duty.
        for armed in [false, true] {
            for locked in [false, true] {
                for freewheeling in [false, true] {
                    for level in [-10.0, -45.0, -60.0] {
                        let situation = situation(armed, locked, freewheeling, level);
                        let mood = Mood::read(situation);
                        let would_fire = armed && (locked || freewheeling);

                        assert_eq!(
                            mood.implies_cues_will_fire(),
                            would_fire,
                            "Pablo showing {mood:?} for {situation:?} — he is lying"
                        );
                    }
                }
            }
        }
    }

    #[test]
    fn timecode_running_while_disarmed_is_never_mistaken_for_a_healthy_show() {
        // The dangerous state, called out on its own because it is the one
        // that ruins shows: numbers rolling, everything looking fine, and
        // nothing going out.
        let mood = Mood::read(situation(false, true, false, -20.0));
        assert_eq!(mood, Mood::Pyjamas);
        assert!(!mood.implies_cues_will_fire());
        assert_ne!(mood, Mood::Playing, "disarmed must not look like playing");
    }

    #[test]
    fn nothing_coming_in_means_asleep_whether_armed_or_not() {
        assert_eq!(
            Mood::read(situation(true, false, false, -90.0)),
            Mood::Asleep
        );
        assert_eq!(
            Mood::read(situation(false, false, false, -90.0)),
            Mood::Asleep
        );
    }

    #[test]
    fn a_weak_signal_shows_before_anything_actually_breaks() {
        // Above the measured threshold he plays; below it he shivers, while
        // still being honest that cues are firing.
        let healthy = Mood::read(situation(true, true, false, -20.0));
        let weak = Mood::read(situation(true, true, false, -58.0));
        assert_eq!(healthy, Mood::Playing);
        assert_eq!(weak, Mood::Shivering);
        assert!(weak.implies_cues_will_fire());
    }

    #[test]
    fn losing_the_signal_is_more_urgent_than_it_being_quiet() {
        // Freewheeling with a weak level: the freewheel is what matters.
        let mood = Mood::read(situation(true, false, true, -58.0));
        assert_eq!(mood, Mood::Wobbling);
    }

    #[test]
    fn a_strum_fades_and_then_stops() {
        let mut strum = Strum::new(Flourish::Osc);
        assert_eq!(strum.intensity(), 1.0);
        assert!(strum.tick(Strum::DURATION / 2.0));
        assert!((strum.intensity() - 0.5).abs() < 0.01);
        assert!(!strum.tick(Strum::DURATION));
    }
}
