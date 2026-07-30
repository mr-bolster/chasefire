//! Where a fired cue actually goes out.
//!
//! One sink per protocol, all behind the same trait, so adding RTP-MIDI later
//! is a new file rather than surgery on the engine. Sinks never decide *whether*
//! to fire — that argument was settled in the `cue` crate — they only send.

use cue::{Action, OscArg};
use std::io;
use std::net::{SocketAddr, ToSocketAddrs, UdpSocket};

/// Anything that can carry a fired cue to the outside world.
pub trait Sink {
    /// A short name for logs and for the UI.
    fn describe(&self) -> String;

    /// Send it. Returns an error rather than panicking: one dead receiver must
    /// never take the show down with it.
    fn deliver(&mut self, action: &Action) -> io::Result<()>;

    /// True if this sink knows how to handle that action at all.
    fn accepts(&self, action: &Action) -> bool;
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

    fn accepts(&self, action: &Action) -> bool {
        matches!(action, Action::Osc { .. })
    }

    fn deliver(&mut self, action: &Action) -> io::Result<()> {
        match action {
            Action::Osc { address, args } => self.send(address, args),
            _ => Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "not an OSC action",
            )),
        }
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
        sink.deliver(&Action::Osc {
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
        let midi = Action::MidiNote {
            channel: 1,
            note: 60,
            velocity: 127,
        };
        assert!(!sink.accepts(&midi));
        assert!(sink.deliver(&midi).is_err());
    }
}
