//! RTP-MIDI (AppleMIDI): MIDI over a network, with nothing to install.
//!
//! This is the one that needs no driver and no virtual cable. A Mac has it in
//! the Audio MIDI Setup window, an iPad has it, Companion speaks it, and on
//! Windows it is what rtpMIDI provides. The point of doing it ourselves rather
//! than leaning on a driver is that there is then nothing to be broken by
//! somebody else's update the week of a show.
//!
//! # What is here and what is not
//!
//! Both halves of establishing a session — this can **invite** a machine, and
//! it can **accept** an invitation from one, because which end goes first is
//! not up to us: a Mac invites, rtpMIDI on Windows invites, and Companion can
//! do either.
//!
//! The **recovery journal is not used**. It exists to rebuild lost state on a
//! lossy link, and it is a great deal of machinery for a case a wired local
//! network does not have. It is also the wrong medicine here: this program
//! sends cues, and a note reconstructed thirty seconds late is worse than the
//! one that went missing. Sessions are told so honestly — the journal bit is
//! zero, which is a legal thing for a sender to say.
//!
//! Nothing is received but session traffic. Chasefire fires cues; it does not
//! read MIDI in.

pub mod packet;
pub mod session;

pub use packet::{Control, Ports};
pub use session::{Session, Status};
