//! MIDI over a network, as an output like any other.
//!
//! The session lives in the `rtpmidi` crate; this is the thin piece that turns
//! a cue message into bytes and hands them over. The encoding is shared with
//! the local MIDI port, which is the point: the same Program Change reaches
//! SuperRack down a cable or across a network, and only the last step differs.

use cue::{Carrier, Message};
use std::io;
use std::net::SocketAddr;

use crate::midi;
use crate::Sink;

pub struct NetworkMidiSink {
    session: rtpmidi::Session,
}

impl NetworkMidiSink {
    /// Start a session. With a `peer` this end invites; without, it waits to be
    /// invited, which is what happens when somebody presses Connect on a Mac.
    pub fn start(name: &str, port: u16, peer: Option<SocketAddr>) -> Result<Self, String> {
        Ok(Self {
            session: rtpmidi::Session::start(name, port, peer)?,
        })
    }

    pub fn status(&self) -> rtpmidi::Status {
        self.session.status()
    }
}

impl Sink for NetworkMidiSink {
    fn describe(&self) -> String {
        let where_to = match self.session.peer() {
            Some(peer) => format!("to {peer}"),
            None => format!("waiting on {}", self.session.port()),
        };
        format!("RTP-MIDI {where_to}")
    }

    fn accepts(&self, message: &Message) -> bool {
        !matches!(message, Message::Osc { .. })
    }

    fn carrier(&self) -> Carrier {
        Carrier::Midi
    }

    fn deliver(&mut self, message: &Message) -> io::Result<()> {
        // A session that is not up yet is a thing to say plainly. Quietly
        // dropping the cue would look identical to a cue that fired.
        if self.session.status() != rtpmidi::Status::Joined {
            return Err(io::Error::new(
                io::ErrorKind::NotConnected,
                "the RTP-MIDI session is not up",
            ));
        }
        for bytes in midi::encode(message)? {
            self.session.send(&bytes).map_err(io::Error::other)?;
        }
        Ok(())
    }
}
