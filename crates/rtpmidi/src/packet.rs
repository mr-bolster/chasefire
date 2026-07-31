//! The bytes RTP-MIDI is made of, and nothing else.
//!
//! No sockets in this file on purpose. The wire format is fixed, small and
//! completely specified, and it is the part that has to be exactly right — a
//! session that never establishes and a session that establishes and drops
//! every packet look identical from a stage, which is to say they look like a
//! cue that did nothing.
//!
//! Two kinds of packet share the wire:
//!
//! * **Session control** — the `FF FF` messages that invite, accept, refuse,
//!   synchronise clocks and say goodbye. They travel on *both* ports.
//! * **RTP MIDI** — the actual notes, on the data port only.

use std::net::SocketAddr;

/// Everything in a session command after the two `FF FF` bytes.
pub const MARKER: [u8; 2] = [0xFF, 0xFF];

/// The version of the protocol everything in the field speaks.
pub const PROTOCOL_VERSION: u32 = 2;

/// The RTP payload type registered for MIDI, with the marker bit set.
const PAYLOAD_TYPE: u8 = 0x61;

/// A session control message.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Control {
    /// "May I join you?" — sent by whichever end goes first.
    Invitation { token: u32, ssrc: u32, name: String },
    /// "Yes."
    Accepted { token: u32, ssrc: u32, name: String },
    /// "No." Sent, among other reasons, when a session is already full.
    Refused { token: u32, ssrc: u32 },
    /// "I am going now."
    Goodbye { token: u32, ssrc: u32 },
    /// One leg of the three-way clock exchange.
    ClockSync {
        ssrc: u32,
        /// 0, 1 or 2 — which leg this is.
        count: u8,
        /// The three timestamps, in units of 100 µs. Only the first `count + 1`
        /// of them mean anything.
        times: [u64; 3],
    },
    /// "I have everything up to here", so the far end can drop its journal.
    ReceiverFeedback { ssrc: u32, sequence: u32 },
}

impl Control {
    /// Read one, or `None` if these bytes are not a session command at all.
    /// Not an error: RTP MIDI data arrives on the same socket and simply is not
    /// one of these.
    pub fn parse(bytes: &[u8]) -> Option<Self> {
        if bytes.len() < 4 || bytes[0..2] != MARKER {
            return None;
        }
        let command = &bytes[2..4];
        let rest = &bytes[4..];
        match command {
            b"IN" | b"OK" | b"NO" => {
                // version, token, ssrc, then a name that may be missing.
                if rest.len() < 12 {
                    return None;
                }
                let token = u32_at(rest, 4)?;
                let ssrc = u32_at(rest, 8)?;
                if command == b"NO" {
                    return Some(Control::Refused { token, ssrc });
                }
                let name = read_name(&rest[12..]);
                Some(if command == b"IN" {
                    Control::Invitation { token, ssrc, name }
                } else {
                    Control::Accepted { token, ssrc, name }
                })
            }
            b"BY" => {
                if rest.len() < 12 {
                    return None;
                }
                Some(Control::Goodbye {
                    token: u32_at(rest, 4)?,
                    ssrc: u32_at(rest, 8)?,
                })
            }
            b"CK" => {
                // Four for the SSRC, one for the leg, three of padding the
                // standard insists on, then three eight-byte timestamps.
                if rest.len() < 32 {
                    return None;
                }
                Some(Control::ClockSync {
                    ssrc: u32_at(rest, 0)?,
                    count: rest[4],
                    times: [u64_at(rest, 8)?, u64_at(rest, 16)?, u64_at(rest, 24)?],
                })
            }
            b"RS" => {
                if rest.len() < 8 {
                    return None;
                }
                Some(Control::ReceiverFeedback {
                    ssrc: u32_at(rest, 0)?,
                    sequence: u32_at(rest, 4)?,
                })
            }
            _ => None,
        }
    }

    pub fn to_bytes(&self) -> Vec<u8> {
        let mut bytes = Vec::with_capacity(48);
        bytes.extend_from_slice(&MARKER);
        match self {
            Control::Invitation { token, ssrc, name } => {
                bytes.extend_from_slice(b"IN");
                push_session(&mut bytes, *token, *ssrc);
                push_name(&mut bytes, name);
            }
            Control::Accepted { token, ssrc, name } => {
                bytes.extend_from_slice(b"OK");
                push_session(&mut bytes, *token, *ssrc);
                push_name(&mut bytes, name);
            }
            Control::Refused { token, ssrc } => {
                bytes.extend_from_slice(b"NO");
                push_session(&mut bytes, *token, *ssrc);
            }
            Control::Goodbye { token, ssrc } => {
                bytes.extend_from_slice(b"BY");
                push_session(&mut bytes, *token, *ssrc);
            }
            Control::ClockSync { ssrc, count, times } => {
                bytes.extend_from_slice(b"CK");
                bytes.extend_from_slice(&ssrc.to_be_bytes());
                bytes.push(*count);
                // Three bytes of padding the standard insists on.
                bytes.extend_from_slice(&[0, 0, 0]);
                for time in times {
                    bytes.extend_from_slice(&time.to_be_bytes());
                }
            }
            Control::ReceiverFeedback { ssrc, sequence } => {
                bytes.extend_from_slice(b"RS");
                bytes.extend_from_slice(&ssrc.to_be_bytes());
                bytes.extend_from_slice(&sequence.to_be_bytes());
            }
        }
        bytes
    }
}

fn push_session(bytes: &mut Vec<u8>, token: u32, ssrc: u32) {
    bytes.extend_from_slice(&PROTOCOL_VERSION.to_be_bytes());
    bytes.extend_from_slice(&token.to_be_bytes());
    bytes.extend_from_slice(&ssrc.to_be_bytes());
}

fn push_name(bytes: &mut Vec<u8>, name: &str) {
    bytes.extend_from_slice(name.as_bytes());
    bytes.push(0);
}

fn read_name(bytes: &[u8]) -> String {
    let end = bytes
        .iter()
        .position(|byte| *byte == 0)
        .unwrap_or(bytes.len());
    String::from_utf8_lossy(&bytes[..end]).into_owned()
}

fn u32_at(bytes: &[u8], at: usize) -> Option<u32> {
    let slice = bytes.get(at..at + 4)?;
    Some(u32::from_be_bytes(slice.try_into().ok()?))
}

fn u64_at(bytes: &[u8], at: usize) -> Option<u64> {
    let slice = bytes.get(at..at + 8)?;
    Some(u64::from_be_bytes(slice.try_into().ok()?))
}

/// One RTP packet carrying MIDI.
///
/// The recovery journal is deliberately not used. It exists to rebuild state
/// after a lost packet on a lossy link, and it is a large amount of machinery
/// for a case that a wired local network does not have — and *this* program
/// only ever sends cues, where a late reconstruction of a note that should have
/// fired thirty seconds ago is worse than the silence.
pub fn midi_packet(ssrc: u32, sequence: u16, timestamp: u32, midi: &[u8]) -> Option<Vec<u8>> {
    // The short form carries fifteen bytes. Every cue message this program
    // sends fits: the longest is a Show Control sysex with a cue number.
    if midi.is_empty() {
        return None;
    }

    let mut packet = Vec::with_capacity(16 + midi.len());
    // Version 2, no padding, no extension, no contributing sources.
    packet.push(0x80);
    // The marker bit is set on the first packet of a "media burst"; every cue
    // we send is its own burst of one.
    packet.push(0x80 | PAYLOAD_TYPE);
    packet.extend_from_slice(&sequence.to_be_bytes());
    packet.extend_from_slice(&timestamp.to_be_bytes());
    packet.extend_from_slice(&ssrc.to_be_bytes());

    if midi.len() < 16 {
        // B=0 (short length), J=0 (no journal), Z=0 (no leading delta time),
        // P=0. The low four bits are the length.
        packet.push(midi.len() as u8);
    } else {
        // B=1: twelve bits of length across two bytes.
        if midi.len() > 0x0FFF {
            return None;
        }
        packet.push(0x80 | ((midi.len() >> 8) as u8 & 0x0F));
        packet.push((midi.len() & 0xFF) as u8);
    }
    packet.extend_from_slice(midi);
    Some(packet)
}

/// Where a session's two ports are. The data port is always the one after the
/// control port; that is not a convention we chose and cannot be changed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Ports {
    pub control: SocketAddr,
    pub data: SocketAddr,
}

impl Ports {
    pub fn from_control(control: SocketAddr) -> Option<Self> {
        let data_port = control.port().checked_add(1)?;
        let mut data = control;
        data.set_port(data_port);
        Some(Self { control, data })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_invitation_survives_the_round_trip() {
        let invitation = Control::Invitation {
            token: 0xDEADBEEF,
            ssrc: 0x0BADF00D,
            name: "Chasefire".into(),
        };
        let bytes = invitation.to_bytes();
        assert_eq!(&bytes[0..4], &[0xFF, 0xFF, b'I', b'N']);
        // Version, then token, then SSRC — in that order, and the order is not
        // ours to choose.
        assert_eq!(&bytes[4..8], &2u32.to_be_bytes());
        assert_eq!(&bytes[8..12], &0xDEADBEEFu32.to_be_bytes());
        assert_eq!(&bytes[12..16], &0x0BADF00Du32.to_be_bytes());
        assert_eq!(Control::parse(&bytes), Some(invitation));
    }

    #[test]
    fn a_name_with_no_terminator_still_reads() {
        // Some implementations leave the null off the end. Refusing to parse
        // would mean refusing to talk to them, which helps nobody.
        let mut bytes = vec![0xFF, 0xFF, b'O', b'K'];
        bytes.extend_from_slice(&2u32.to_be_bytes());
        bytes.extend_from_slice(&1u32.to_be_bytes());
        bytes.extend_from_slice(&2u32.to_be_bytes());
        bytes.extend_from_slice(b"rtpMIDI");
        match Control::parse(&bytes) {
            Some(Control::Accepted { name, .. }) => assert_eq!(name, "rtpMIDI"),
            other => panic!("should have read an OK, got {other:?}"),
        }
    }

    #[test]
    fn clock_sync_carries_three_timestamps_and_a_leg_number() {
        let sync = Control::ClockSync {
            ssrc: 7,
            count: 1,
            times: [100, 200, 0],
        };
        let bytes = sync.to_bytes();
        // SSRC, count, three bytes of padding, then 3 x 8 bytes.
        assert_eq!(bytes.len(), 4 + 4 + 1 + 3 + 24);
        assert_eq!(bytes[8], 1, "the leg number");
        assert_eq!(Control::parse(&bytes), Some(sync));
    }

    #[test]
    fn midi_data_is_not_mistaken_for_a_session_command() {
        // Both arrive on the same socket. An RTP packet starts 0x80, not 0xFF,
        // and must simply not parse rather than parse into nonsense.
        let packet = midi_packet(1, 0, 0, &[0x90, 60, 127]).unwrap();
        assert_eq!(Control::parse(&packet), None);
    }

    #[test]
    fn a_short_midi_packet_has_the_length_in_its_header_byte() {
        let packet = midi_packet(0x11223344, 0x0102, 0x0A0B0C0D, &[0x90, 60, 127]).unwrap();
        assert_eq!(packet[0], 0x80, "version 2");
        assert_eq!(packet[1], 0xE1, "marker bit and payload type 0x61");
        assert_eq!(&packet[2..4], &[0x01, 0x02], "sequence number");
        assert_eq!(&packet[4..8], &[0x0A, 0x0B, 0x0C, 0x0D], "timestamp");
        assert_eq!(&packet[8..12], &[0x11, 0x22, 0x33, 0x44], "SSRC");
        assert_eq!(packet[12], 0x03, "no journal, no delta, three bytes");
        assert_eq!(&packet[13..], &[0x90, 60, 127]);
    }

    #[test]
    fn a_long_midi_packet_spreads_its_length_over_two_bytes() {
        // A Show Control sysex with a long cue number gets here.
        let midi: Vec<u8> = std::iter::repeat_n(0x7Fu8, 20).collect();
        let packet = midi_packet(1, 0, 0, &midi).unwrap();
        assert_eq!(packet[12] & 0x80, 0x80, "the long-length flag");
        assert_eq!(packet[12] & 0x0F, 0, "high bits of 20");
        assert_eq!(packet[13], 20, "low bits of 20");
        assert_eq!(&packet[14..], &midi[..]);
    }

    #[test]
    fn an_empty_midi_list_is_not_a_packet() {
        assert_eq!(midi_packet(1, 0, 0, &[]), None);
    }

    #[test]
    fn the_data_port_is_the_one_after_the_control_port() {
        let ports = Ports::from_control("192.168.1.50:5004".parse().unwrap()).unwrap();
        assert_eq!(ports.data.port(), 5005);
        assert_eq!(ports.data.ip(), ports.control.ip());

        // And the last port in the range has no room for a data port.
        assert!(Ports::from_control("192.168.1.50:65535".parse().unwrap()).is_none());
    }

    #[test]
    fn rubbish_parses_as_nothing_rather_than_as_something() {
        for rubbish in [
            &b""[..],
            &b"\xFF"[..],
            &b"\xFF\xFF"[..],
            &b"\xFF\xFFZZ"[..],
            // Right marker, right command, truncated body.
            &b"\xFF\xFFIN\x00\x00"[..],
        ] {
            assert_eq!(Control::parse(rubbish), None, "{rubbish:?}");
        }
    }
}
