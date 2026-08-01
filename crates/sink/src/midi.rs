// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
//
// Copyright (c) Leo Bolster.

//! MIDI out: a local port, and the bytes that go down it.
//!
//! This is the half of the trade that does not speak OSC at all. A grandMA2
//! has no OSC input — it takes MIDI Show Control or nothing. Waves SuperRack
//! recalls its snapshots on Program Change. Both were in the first sentence of
//! what this program is for, and neither could be reached until now.
//!
//! The encoding is done here, by hand and with tests, because it is small and
//! completely specified and because a wrong byte on a stage is silent: the desk
//! simply does not move, and there is nothing to read.

use cue::{Carrier, Message, ShowControl};
use std::io;

use crate::Sink;

/// Build the wire messages for one cue message — **a list of them**, because a
/// program change with a bank is three separate MIDI messages and a port will
/// not take three in one call. Sent as one buffer, ALSA parsed the first and
/// dropped the rest: the bank arrived and the program change never did, so the
/// desk sat on whatever snapshot it already had. Caught by sending it down a
/// real port and listening, which is the only way that shows up.
///
/// Public and pure, so the wire format can be checked without a MIDI port in
/// the machine.
pub fn encode(message: &Message) -> io::Result<Vec<Vec<u8>>> {
    match message {
        Message::MidiNote {
            channel,
            note,
            velocity,
        } => Ok(vec![vec![
            0x90 | channel_nibble(*channel)?,
            seven(*note)?,
            seven(*velocity)?,
        ]]),
        Message::MidiProgramChange {
            channel,
            program,
            bank,
        } => {
            let mut messages = Vec::with_capacity(3);
            // Bank first when there is one: the desk latches the bank and the
            // program change that follows selects within it. Sent the other way
            // round it recalls from whichever bank was already selected, which
            // is how somebody ends up with the wrong snapshot and no error.
            if let Some((msb, lsb)) = bank {
                let status = 0xB0 | channel_nibble(*channel)?;
                messages.push(vec![status, 0x00, seven(*msb)?]);
                messages.push(vec![status, 0x20, seven(*lsb)?]);
            }
            messages.push(vec![0xC0 | channel_nibble(*channel)?, seven(*program)?]);
            Ok(messages)
        }
        Message::MidiControlChange {
            channel,
            controller,
            value,
        } => Ok(vec![vec![
            0xB0 | channel_nibble(*channel)?,
            seven(*controller)?,
            seven(*value)?,
        ]]),
        Message::ShowControl(msc) => Ok(vec![encode_show_control(msc)?]),
        Message::Osc { .. } => Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "that is an OSC message, not a MIDI one",
        )),
    }
}

/// `F0 7F <device> 02 <format> <command> <cue> [00 <list>] F7`
///
/// The cue number travels as **text**, digit by digit: cue 21.5 goes out as
/// `32 31 2E 35`. That is not a quirk to tidy up — 21.5 and 21.50 are different
/// cues on a desk, and only the text tells them apart.
pub fn encode_show_control(msc: &ShowControl) -> io::Result<Vec<u8>> {
    let mut bytes = vec![
        0xF0,
        0x7F,
        seven(msc.device)?,
        0x02,
        seven(msc.format)?,
        msc.command.byte(),
    ];
    push_ascii(&mut bytes, msc.cue.trim(), "cue number")?;
    if let Some(list) = &msc.list {
        let list = list.trim();
        if !list.is_empty() {
            bytes.push(0x00);
            push_ascii(&mut bytes, list, "cue list")?;
        }
    }
    bytes.push(0xF7);
    Ok(bytes)
}

/// Numbers only, as ASCII. Anything else would be a byte the desk reads as part
/// of the message structure, and a stray one turns a GO into nothing at all.
fn push_ascii(bytes: &mut Vec<u8>, text: &str, what: &str) -> io::Result<()> {
    if text.is_empty() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            format!("the {what} is empty"),
        ));
    }
    for character in text.chars() {
        if !character.is_ascii_digit() && character != '.' {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                format!("a {what} can only be digits and dots, not '{character}'"),
            ));
        }
        bytes.push(character as u8);
    }
    Ok(())
}

/// MIDI channels are 1..=16 to a human and 0..=15 on the wire. Getting this
/// backwards sends everything one channel out, which looks like a dead cable.
fn channel_nibble(channel: u8) -> io::Result<u8> {
    if !(1..=16).contains(&channel) {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            format!("MIDI channel {channel} does not exist; they run 1 to 16"),
        ));
    }
    Ok(channel - 1)
}

fn seven(value: u8) -> io::Result<u8> {
    if value > 127 {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            format!("{value} does not fit in a MIDI byte; they run 0 to 127"),
        ));
    }
    Ok(value)
}

/// A local MIDI port.
pub struct MidiSink {
    connection: midir::MidiOutputConnection,
    port: String,
}

impl MidiSink {
    /// Everything this machine can send MIDI to, by name.
    pub fn ports() -> Result<Vec<String>, String> {
        let midi = midir::MidiOutput::new("Chasefire").map_err(|error| error.to_string())?;
        Ok(midi
            .ports()
            .iter()
            .filter_map(|port| midi.port_name(port).ok())
            .collect())
    }

    /// Open one by name. Matched loosely on purpose: port names carry client
    /// numbers that change between reboots, and an operator should not have to
    /// re-pick their desk because ALSA renumbered it.
    pub fn open(wanted: &str) -> Result<Self, String> {
        let midi = midir::MidiOutput::new("Chasefire").map_err(|error| error.to_string())?;
        let ports = midi.ports();
        let found = ports
            .iter()
            .find(|port| midi.port_name(port).as_deref() == Ok(wanted))
            .or_else(|| {
                ports.iter().find(|port| {
                    midi.port_name(port)
                        .map(|name| name.contains(wanted))
                        .unwrap_or(false)
                })
            })
            .ok_or_else(|| format!("there is no MIDI port called '{wanted}'"))?;
        let port = midi.port_name(found).unwrap_or_else(|_| wanted.to_string());
        let connection = midi
            .connect(found, "chasefire")
            .map_err(|error| error.to_string())?;
        Ok(Self { connection, port })
    }

    pub fn port(&self) -> &str {
        &self.port
    }

    /// Put bytes on the port as they are.
    ///
    /// For the things that are MIDI but are not a cue: quarter-frames, and the
    /// full-frame position that goes with them. They have no place in the cue
    /// model — nobody programmes a quarter-frame — but they still have to go
    /// down the same wire.
    pub fn send_raw(&mut self, bytes: &[u8]) -> io::Result<()> {
        self.connection
            .send(bytes)
            .map_err(|error| io::Error::other(error.to_string()))
    }
}

impl Sink for MidiSink {
    fn describe(&self) -> String {
        format!("MIDI to {}", self.port)
    }

    fn accepts(&self, message: &Message) -> bool {
        !matches!(message, Message::Osc { .. })
    }

    fn carrier(&self) -> Carrier {
        Carrier::Midi
    }

    fn deliver(&mut self, message: &Message) -> io::Result<()> {
        for bytes in encode(message)? {
            self.connection
                .send(&bytes)
                .map_err(|error| io::Error::other(error.to_string()))?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use cue::{ShowCommand, ShowControl};

    #[test]
    fn a_note_goes_out_on_the_channel_a_human_named() {
        // Channel 1 to a person is channel 0 on the wire. Getting this backwards
        // sends everything one channel out, which looks exactly like a dead
        // cable and is the first thing anybody would waste an hour on.
        let bytes = encode(&Message::MidiNote {
            channel: 1,
            note: 60,
            velocity: 127,
        })
        .unwrap();
        assert_eq!(bytes, [vec![0x90, 60, 127]]);

        let bytes = encode(&Message::MidiNote {
            channel: 16,
            note: 60,
            velocity: 1,
        })
        .unwrap();
        assert_eq!(bytes, [vec![0x9F, 60, 1]]);
    }

    #[test]
    fn a_bank_is_sent_before_the_program_it_selects() {
        // This is how SuperRack reaches snapshots past 128. The other way round
        // recalls from whatever bank happened to be selected — the wrong
        // snapshot, and no error anywhere.
        let bytes = encode(&Message::MidiProgramChange {
            channel: 1,
            program: 5,
            bank: Some((0, 2)),
        })
        .unwrap();
        assert_eq!(
            bytes,
            [vec![0xB0, 0x00, 0], vec![0xB0, 0x20, 2], vec![0xC0, 5]],
            "three separate messages: bank MSB, bank LSB, then the program change"
        );
    }

    #[test]
    fn show_control_carries_the_cue_number_as_written() {
        // Cue 21.5 goes out as the characters '2' '1' '.' '5'. It is not a
        // number on the wire, and 21.5 is not 21.50 to a desk.
        let bytes = encode_show_control(&ShowControl {
            device: 0x7F,
            format: 0x01,
            command: ShowCommand::Go,
            cue: "21.5".into(),
            list: None,
        })
        .unwrap();
        assert_eq!(
            bytes,
            [0xF0, 0x7F, 0x7F, 0x02, 0x01, 0x01, b'2', b'1', b'.', b'5', 0xF7]
        );
    }

    #[test]
    fn a_cue_list_is_separated_by_a_null() {
        let bytes = encode_show_control(&ShowControl {
            device: 3,
            format: 0x01,
            command: ShowCommand::Go,
            cue: "7".into(),
            list: Some("2".into()),
        })
        .unwrap();
        assert_eq!(
            bytes,
            [0xF0, 0x7F, 3, 0x02, 0x01, 0x01, b'7', 0x00, b'2', 0xF7]
        );
    }

    #[test]
    fn the_commands_have_the_numbers_the_standard_gives_them() {
        // Worth pinning: they are not consecutive — GO OFF is 0x0B, not 0x04.
        assert_eq!(ShowCommand::Go.byte(), 0x01);
        assert_eq!(ShowCommand::Stop.byte(), 0x02);
        assert_eq!(ShowCommand::Resume.byte(), 0x03);
        assert_eq!(ShowCommand::GoOff.byte(), 0x0B);
    }

    #[test]
    fn a_cue_number_that_is_not_a_number_is_refused() {
        // Anything but digits and dots would be a byte the desk reads as part
        // of the message structure. Better to complain in the settings window
        // than to send a GO that does nothing on the night.
        let mut msc = ShowControl {
            cue: "encore".into(),
            ..Default::default()
        };
        let error = encode_show_control(&msc).expect_err("should have refused");
        assert!(error.to_string().contains('e'), "{error}");

        msc.cue = String::new();
        assert!(encode_show_control(&msc).is_err(), "an empty cue number");
    }

    #[test]
    fn out_of_range_numbers_say_which_and_why() {
        let error = encode(&Message::MidiNote {
            channel: 17,
            note: 60,
            velocity: 100,
        })
        .expect_err("channel 17 does not exist");
        assert!(error.to_string().contains("1 to 16"), "{error}");

        let error = encode(&Message::MidiNote {
            channel: 1,
            note: 200,
            velocity: 100,
        })
        .expect_err("note 200 does not exist");
        assert!(error.to_string().contains("0 to 127"), "{error}");
    }
}
