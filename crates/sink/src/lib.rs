// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
//
// Copyright (c) Leo Bolster.

//! Where a fired cue actually goes out.
//!
//! One sink per protocol, all behind the same trait, so adding RTP-MIDI later
//! is a new file rather than surgery on the engine. Sinks never decide *whether*
//! to fire — that argument was settled in the `cue` crate — they only send.

pub mod midi;
pub mod mtc;
pub mod mtc_clock;
pub mod network;

pub use midi::MidiSink;
pub use mtc_clock::MtcClock;
pub use network::NetworkMidiSink;

use cue::{Carrier, Message, OscArg, Step};
use std::io;
use std::net::{SocketAddr, ToSocketAddrs, UdpSocket};

/// Anything that can carry a fired cue to the outside world.
pub trait Sink {
    /// A short name for logs and for the UI.
    fn describe(&self) -> String;

    /// Send it. Returns an error rather than panicking: one dead receiver must
    /// never take the show down with it.
    fn deliver(&mut self, message: &Message) -> io::Result<()>;

    /// True if this sink knows how to carry that message at all.
    fn accepts(&self, message: &Message) -> bool;

    /// Which sort of messages this one carries.
    fn carrier(&self) -> Carrier;
}

/// Open Sound Control over UDP: Resolume, grandMA3, TouchDesigner, VST hosts.
pub struct OscSink {
    socket: UdpSocket,
    target: SocketAddr,
}

impl OscSink {
    pub fn connect(target: impl ToSocketAddrs) -> io::Result<Self> {
        let target = target
            .to_socket_addrs()?
            .next()
            .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "no such address"))?;
        // Port 0: the operating system picks our source port. We only ever send.
        let socket = UdpSocket::bind("0.0.0.0:0")?;
        Ok(Self { socket, target })
    }

    pub fn target(&self) -> SocketAddr {
        self.target
    }

    pub fn send(&self, address: &str, args: &[OscArg]) -> io::Result<()> {
        let packet = encode_message(address, args)?;
        self.socket.send_to(&packet, self.target)?;
        Ok(())
    }
}

impl Sink for OscSink {
    fn describe(&self) -> String {
        format!("OSC to {}", self.target)
    }

    fn accepts(&self, message: &Message) -> bool {
        matches!(message, Message::Osc { .. })
    }

    fn carrier(&self) -> Carrier {
        Carrier::Osc
    }

    fn deliver(&mut self, message: &Message) -> io::Result<()> {
        match message {
            Message::Osc { address, args } => self.send(address, args),
            _ => Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "not an OSC message",
            )),
        }
    }
}

/// Everywhere this program can send, by name.
///
/// A show is not one machine. The video server, the desk and the lighting
/// console are three addresses, and the cue that starts the song talks to two
/// of them. So a step names where it goes — and a step that names nowhere goes
/// to the first output that can carry it, which is what a one-destination show
/// wants and never has to think about.
#[derive(Default)]
pub struct Outputs {
    named: Vec<(String, Box<dyn Sink>)>,
}

impl Outputs {
    pub fn new() -> Self {
        Self::default()
    }

    /// Add one, replacing any output already going by that name.
    pub fn put(&mut self, name: impl Into<String>, sink: Box<dyn Sink>) {
        let name = name.into();
        self.named.retain(|(existing, _)| existing != &name);
        self.named.push((name, sink));
    }

    pub fn remove(&mut self, name: &str) -> bool {
        let before = self.named.len();
        self.named.retain(|(existing, _)| existing != name);
        self.named.len() != before
    }

    pub fn is_empty(&self) -> bool {
        self.named.is_empty()
    }

    /// The names, in the order they were added.
    pub fn names(&self) -> impl Iterator<Item = &str> {
        self.named.iter().map(|(name, _)| name.as_str())
    }

    pub fn describe(&self, name: &str) -> Option<String> {
        self.named
            .iter()
            .find(|(existing, _)| existing == name)
            .map(|(_, sink)| sink.describe())
    }

    /// Send one step wherever it is addressed.
    ///
    /// The errors are deliberately the ones an operator can act on. "There is
    /// no output called wing" is a thing somebody can go and fix between songs;
    /// a swallowed failure is a cue that silently does nothing all night.
    pub fn deliver(&mut self, step: &Step) -> io::Result<()> {
        let wanted = step.send.carried_by();
        let index = match &step.to {
            Some(name) => {
                let found = self
                    .named
                    .iter()
                    .position(|(existing, _)| existing == name)
                    .ok_or_else(|| {
                        io::Error::new(
                            io::ErrorKind::NotFound,
                            format!("there is no output called '{name}'"),
                        )
                    })?;
                if !self.named[found].1.accepts(&step.send) {
                    return Err(io::Error::new(
                        io::ErrorKind::InvalidInput,
                        format!("'{name}' cannot carry that kind of message"),
                    ));
                }
                found
            }
            // Nowhere named: the first output that can carry it.
            None => self
                .named
                .iter()
                .position(|(_, sink)| sink.carrier() == wanted)
                .ok_or_else(|| {
                    io::Error::new(
                        io::ErrorKind::NotConnected,
                        match wanted {
                            Carrier::Osc => "no OSC output is connected",
                            Carrier::Midi => "no MIDI output is open",
                        },
                    )
                })?,
        };
        self.named[index].1.deliver(&step.send)
    }
}

/// Encode one OSC message by hand.
///
/// The wire format is small and completely specified, and doing it here keeps a
/// dependency out of the one code path that has to work on show night.
fn encode_message(address: &str, args: &[OscArg]) -> io::Result<Vec<u8>> {
    if !address.starts_with('/') {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "an OSC address must start with '/'",
        ));
    }

    let mut packet = Vec::with_capacity(64);
    push_osc_string(&mut packet, address);

    // Type tag string: a comma, then one character per argument.
    let mut tags = String::from(",");
    for arg in args {
        tags.push(match arg {
            OscArg::Int(_) => 'i',
            OscArg::Float(_) => 'f',
            OscArg::Str(_) => 's',
            // Booleans are carried by the tag alone; they have no payload.
            OscArg::Bool(true) => 'T',
            OscArg::Bool(false) => 'F',
        });
    }
    push_osc_string(&mut packet, &tags);

    for arg in args {
        match arg {
            OscArg::Int(value) => packet.extend_from_slice(&value.to_be_bytes()),
            OscArg::Float(value) => packet.extend_from_slice(&value.to_be_bytes()),
            OscArg::Str(value) => push_osc_string(&mut packet, value),
            OscArg::Bool(_) => {}
        }
    }

    Ok(packet)
}

/// OSC strings are null terminated and padded out to a multiple of four bytes.
fn push_osc_string(packet: &mut Vec<u8>, value: &str) {
    packet.extend_from_slice(value.as_bytes());
    packet.push(0);
    while packet.len() % 4 != 0 {
        packet.push(0);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn encodes_an_address_with_no_arguments() {
        let packet = encode_message("/cue/1", &[]).unwrap();
        assert_eq!(&packet, b"/cue/1\0\0,\0\0\0");
        assert_eq!(packet.len() % 4, 0, "OSC packets are 4-byte aligned");
    }

    #[test]
    fn encodes_the_mixed_arguments_a_media_server_expects() {
        let packet = encode_message(
            "/composition/layers/1/clips/3/connect",
            &[OscArg::Int(1), OscArg::Float(0.5)],
        )
        .unwrap();

        assert!(packet.starts_with(b"/composition/layers/1/clips/3/connect\0"));
        assert!(
            packet.windows(4).any(|window| window == b",if\0"),
            "type tags missing"
        );
        assert!(packet.ends_with(&0.5f32.to_be_bytes()));
        assert_eq!(packet.len() % 4, 0);
    }

    #[test]
    fn booleans_travel_in_the_type_tag_alone() {
        let packet = encode_message("/go", &[OscArg::Bool(true), OscArg::Bool(false)]).unwrap();
        assert_eq!(&packet, b"/go\0,TF\0");
    }

    #[test]
    fn a_string_argument_is_padded_like_the_address() {
        let packet = encode_message("/gma3/cmd", &[OscArg::Str("Go+ Sequence 1".into())]).unwrap();
        assert_eq!(packet.len() % 4, 0);
        assert!(packet.ends_with(b"Go+ Sequence 1\0\0"));
    }

    #[test]
    fn refuses_an_address_that_is_not_an_address() {
        assert!(encode_message("cue/1", &[]).is_err());
    }

    #[test]
    fn actually_puts_the_bytes_on_the_wire() {
        // Bind a receiver on a port the OS picks, then fire at it.
        let receiver = UdpSocket::bind("127.0.0.1:0").unwrap();
        receiver
            .set_read_timeout(Some(std::time::Duration::from_secs(2)))
            .unwrap();
        let address = receiver.local_addr().unwrap();

        let mut sink = OscSink::connect(address).unwrap();
        sink.deliver(&Message::Osc {
            address: "/cue/7".into(),
            args: vec![OscArg::Int(7)],
        })
        .unwrap();

        let mut buffer = [0u8; 256];
        let (length, _) = receiver.recv_from(&mut buffer).unwrap();
        assert_eq!(
            &buffer[..length],
            &encode_message("/cue/7", &[OscArg::Int(7)]).unwrap()
        );
    }

    #[test]
    fn refuses_to_deliver_what_it_does_not_speak() {
        let mut sink = OscSink::connect("127.0.0.1:9999").unwrap();
        let midi = Message::MidiNote {
            channel: 1,
            note: 60,
            velocity: 127,
        };
        assert!(!sink.accepts(&midi));
        assert!(sink.deliver(&midi).is_err());
    }
}

#[cfg(test)]
mod outputs_tests {
    use super::*;

    fn listener() -> (UdpSocket, SocketAddr) {
        let socket = UdpSocket::bind("127.0.0.1:0").unwrap();
        socket
            .set_read_timeout(Some(std::time::Duration::from_secs(2)))
            .unwrap();
        let address = socket.local_addr().unwrap();
        (socket, address)
    }

    #[test]
    fn one_cue_reaches_two_different_machines() {
        // The case the old model could not express at all: a moment in the show
        // that starts the video and moves the desk.
        let (video, video_at) = listener();
        let (desk, desk_at) = listener();

        let mut outputs = Outputs::new();
        outputs.put("video", Box::new(OscSink::connect(video_at).unwrap()));
        outputs.put("mesa", Box::new(OscSink::connect(desk_at).unwrap()));

        outputs
            .deliver(&Step::to(
                "video",
                Message::Osc {
                    address: "/composition/columns/1/connect".into(),
                    args: vec![OscArg::Int(1)],
                },
            ))
            .unwrap();
        outputs
            .deliver(&Step::to(
                "mesa",
                Message::Osc {
                    address: "/-action/goscene".into(),
                    args: vec![OscArg::Int(12)],
                },
            ))
            .unwrap();

        let mut buffer = [0u8; 256];
        let (length, _) = video.recv_from(&mut buffer).unwrap();
        assert!(buffer[..length].starts_with(b"/composition/columns/1/connect\0"));
        let (length, _) = desk.recv_from(&mut buffer).unwrap();
        assert!(buffer[..length].starts_with(b"/-action/goscene\0"));
    }

    #[test]
    fn a_step_that_names_nowhere_takes_the_only_road_there_is() {
        // A show with one destination should never have to name it.
        let (receiver, address) = listener();
        let mut outputs = Outputs::new();
        outputs.put("main", Box::new(OscSink::connect(address).unwrap()));

        outputs
            .deliver(&Step::anywhere(Message::Osc {
                address: "/go".into(),
                args: vec![],
            }))
            .unwrap();

        let mut buffer = [0u8; 64];
        let (length, _) = receiver.recv_from(&mut buffer).unwrap();
        assert_eq!(&buffer[..length], b"/go\0,\0\0\0");
    }

    #[test]
    fn naming_an_output_that_is_not_there_says_so_by_name() {
        // Between songs somebody can fix "there is no output called wing".
        // Nobody can fix a cue that quietly did nothing.
        let mut outputs = Outputs::new();
        outputs.put(
            "video",
            Box::new(OscSink::connect("127.0.0.1:9999").unwrap()),
        );

        let error = outputs
            .deliver(&Step::to(
                "wing",
                Message::Osc {
                    address: "/go".into(),
                    args: vec![],
                },
            ))
            .expect_err("should have complained");
        assert!(error.to_string().contains("wing"), "{error}");
    }

    #[test]
    fn with_nothing_connected_it_says_that_rather_than_pretending() {
        let mut outputs = Outputs::new();
        let error = outputs
            .deliver(&Step::anywhere(Message::Osc {
                address: "/go".into(),
                args: vec![],
            }))
            .expect_err("should have complained");
        assert!(error.to_string().contains("OSC"), "{error}");
    }

    #[test]
    fn adding_the_same_name_twice_replaces_rather_than_duplicates() {
        let (_first, first_at) = listener();
        let (second, second_at) = listener();

        let mut outputs = Outputs::new();
        outputs.put("video", Box::new(OscSink::connect(first_at).unwrap()));
        outputs.put("video", Box::new(OscSink::connect(second_at).unwrap()));
        assert_eq!(outputs.names().count(), 1, "two outputs with one name");

        outputs
            .deliver(&Step::to(
                "video",
                Message::Osc {
                    address: "/go".into(),
                    args: vec![],
                },
            ))
            .unwrap();

        // It went to the new address, not the one it replaced.
        let mut buffer = [0u8; 64];
        let (length, _) = second.recv_from(&mut buffer).unwrap();
        assert_eq!(&buffer[..length], b"/go\0,\0\0\0");
    }
}
