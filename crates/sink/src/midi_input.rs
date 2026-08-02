// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
//
// Copyright (c) Leo Bolster.

//! A MIDI port that is listened to rather than sent down.
//!
//! Everything here happens on midir's own callback thread, which must never be
//! made to wait: it is as close to a real-time context as MIDI gets. So the
//! callback decodes and pushes, and whoever wants the positions collects them
//! whenever it suits — the same shape as the audio side.

use crate::mtc::Rate;
use crate::mtc_in::{Heard, Reader};
use ltc::Timecode;
use std::sync::mpsc::{self, Receiver};

/// A timecode position that arrived over MIDI.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Position {
    pub at: Timecode,
    pub rate: Rate,
    pub reverse: bool,
}

/// An open MIDI input, reading timecode. Dropping it closes the port.
pub struct MtcInput {
    positions: Receiver<Position>,
    port: String,
    /// Held because dropping it closes the connection.
    _connection: midir::MidiInputConnection<()>,
}

impl MtcInput {
    /// Everything this machine can listen to.
    pub fn ports() -> Result<Vec<String>, String> {
        let midi = midir::MidiInput::new("Chasefire").map_err(|error| error.to_string())?;
        Ok(midi
            .ports()
            .iter()
            .filter_map(|port| midi.port_name(port).ok())
            .collect())
    }

    /// Open one by name, loosely matched — port names carry client numbers
    /// that change between reboots, and nobody should have to re-pick their
    /// desk because ALSA renumbered it.
    pub fn open(wanted: &str) -> Result<Self, String> {
        let mut midi = midir::MidiInput::new("Chasefire").map_err(|error| error.to_string())?;
        // Timecode is system-common traffic. Without this, midir filters it out
        // and the port sits there looking open and silent.
        midi.ignore(midir::Ignore::None);

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
            .ok_or_else(|| format!("there is no MIDI input called '{wanted}'"))?;
        let port = midi.port_name(found).unwrap_or_else(|_| wanted.to_string());

        let (sender, positions) = mpsc::channel();
        let mut reader = Reader::new();
        let connection = midi
            .connect(
                found,
                "chasefire-mtc",
                move |_stamp, bytes, _| {
                    if let Heard::At(at, rate, reverse) = reader.take(bytes) {
                        // If nobody is collecting any more the show is over;
                        // there is nothing useful to do about it here.
                        let _ = sender.send(Position { at, rate, reverse });
                    }
                },
                (),
            )
            .map_err(|error| error.to_string())?;

        Ok(Self {
            positions,
            port,
            _connection: connection,
        })
    }

    pub fn port(&self) -> &str {
        &self.port
    }

    /// Every position that has arrived since last asked. Never blocks.
    pub fn drain(&self) -> Vec<Position> {
        self.positions.try_iter().collect()
    }
}
