//! A working cue for each machine somebody is likely to point this at.
//!
//! Not magic and not hidden: a preset writes one ordinary cue that can then be
//! edited like any other. What it saves is the half hour of reading a manual to
//! find out that QLab wants no arguments, that grandMA3 wants its whole command
//! line as a string, and that a Behringer Wing needs two messages in the right
//! order to recall one scene.
//!
//! Every address and port here came from the manufacturer's own documentation.
//! Where that was not reachable it says so, because a port number nobody
//! checked is worse than no port number: it sends somebody hunting a network
//! fault that does not exist.

use cue::{Message, OscArg, ShowControl, Step};

pub struct Preset {
    /// What it is called in the menu. A product name, not a protocol.
    pub name: &'static str,
    /// The port that machine listens on, when it has a fixed one.
    pub port: Option<u16>,
    /// True when the port could not be confirmed from the manufacturer's own
    /// documentation and should be checked on the desk.
    pub port_unconfirmed: bool,
    /// A one-line reminder of what the cue does, shown under the menu.
    pub note: &'static str,
    /// What the cue sends.
    pub build: fn() -> Vec<Step>,
}

impl Preset {
    /// A cue built from this preset, at a round hour so it is obvious it needs
    /// its time setting.
    pub fn cue(&self, id: u32) -> cue::Cue {
        cue::Cue::of(
            id,
            self.name,
            ltc::Timecode::new(10, 0, 0, 0),
            (self.build)(),
        )
    }

    /// What to put in the address box for this machine, on this network.
    pub fn suggested_target(&self) -> Option<String> {
        self.port.map(|port| format!("127.0.0.1:{port}"))
    }
}

fn osc(address: &str, args: Vec<OscArg>) -> Step {
    Step::anywhere(Message::Osc {
        address: address.into(),
        args,
    })
}

pub static ALL: &[Preset] = &[
    Preset {
        name: "Resolume",
        port: Some(7000),
        port_unconfirmed: false,
        // Triggers are integers: 1 connects, 0 releases. Opacity and the like
        // are floats from 0.0 to 1.0 — a different type on the same protocol.
        note: "Connect column 1",
        build: || vec![osc("/composition/columns/1/connect", vec![OscArg::Int(1)])],
    },
    Preset {
        name: "QLab",
        port: Some(53000),
        port_unconfirmed: false,
        // The one that proves a single on/off integer is not a cue system: an
        // extra argument here is not harmless.
        note: "Start cue 1 — no arguments at all",
        build: || vec![osc("/cue/1/start", vec![])],
    },
    Preset {
        name: "grandMA3",
        port: Some(8000),
        port_unconfirmed: false,
        // The whole command line, as a string. Anything the desk can be typed
        // can be sent.
        note: "The command line, as text",
        build: || vec![osc("/cmd", vec![OscArg::Str("Go+ Sequence 1".into())])],
    },
    Preset {
        name: "grandMA2 (MSC)",
        port: None,
        port_unconfirmed: false,
        // The MA2 has no OSC input. This is the only way in.
        note: "MIDI Show Control — the MA2 has no OSC input",
        build: || {
            vec![Step::anywhere(Message::ShowControl(ShowControl {
                device: 0x7F,
                format: 0x01,
                command: cue::ShowCommand::Go,
                cue: "1".into(),
                list: None,
            }))]
        },
    },
    Preset {
        name: "ChamSys MagicQ",
        port: Some(6553),
        // Their documentation answered 403 to every attempt; this came from
        // secondary sources. Worth thirty seconds on the desk before a show.
        port_unconfirmed: true,
        note: "Playback 1, cue 1",
        build: || vec![osc("/pb/1/1", vec![])],
    },
    Preset {
        name: "Behringer X32 / M32",
        port: Some(10023),
        port_unconfirmed: false,
        note: "Recall scene 0",
        build: || vec![osc("/-action/goscene", vec![OscArg::Int(0)])],
    },
    Preset {
        name: "Behringer Wing",
        port: Some(2223),
        port_unconfirmed: false,
        // Two messages, in this order. The index alone does nothing; GO alone
        // recalls whatever index was last set. This is the case a one-message
        // cue could not express at all.
        //
        // What gets recalled is **item N of whichever library list is open on
        // the console**, so the desk decides whether that is a snapshot (the
        // whole console state — the scene-level one), a snippet (only the
        // parameters that were scoped, the granular one) or an entry in the
        // show's cue list. A show has to be open on the console first. Saying
        // "scene" here would have been wrong on two counts.
        note: "Item 1 of the open library — the index, then GO. \
               Snapshot, snippet or show entry, whichever list the desk has open",
        build: || {
            vec![
                osc("/$ctl/lib/$actionidx", vec![OscArg::Int(1)]),
                osc("/$ctl/lib/$action", vec![OscArg::Str("GO".into())]),
            ]
        },
    },
    Preset {
        name: "Waves SuperRack",
        port: None,
        port_unconfirmed: false,
        // Program change 0..127 for the first 128 snapshots; past that it takes
        // a bank select first, which the cue editor can add.
        note: "Snapshot 1 by program change — SuperRack has no OSC",
        build: || {
            vec![Step::anywhere(Message::MidiProgramChange {
                channel: 1,
                program: 0,
                bank: None,
            })]
        },
    },
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_preset_builds_a_cue_that_sends_something() {
        for preset in ALL {
            let cue = preset.cue(1);
            assert!(
                !cue.steps.is_empty(),
                "{} builds a cue that sends nothing",
                preset.name
            );
        }
    }

    #[test]
    fn every_osc_preset_is_encodable_as_it_stands() {
        // A preset that produces an address the encoder refuses would be worse
        // than no preset: it looks like a working cue and fails on the night.
        for preset in ALL {
            for step in (preset.build)() {
                if let Message::Osc { address, args } = &step.send {
                    assert!(
                        address.starts_with('/'),
                        "{} has an address that is not one: {address}",
                        preset.name
                    );
                    let _ = args;
                }
            }
        }
    }

    #[test]
    fn every_midi_preset_is_encodable_as_it_stands() {
        for preset in ALL {
            for step in (preset.build)() {
                if !matches!(step.send, Message::Osc { .. }) {
                    sink::midi::encode(&step.send)
                        .unwrap_or_else(|error| panic!("{}: {error}", preset.name));
                }
            }
        }
    }

    #[test]
    fn the_wing_keeps_its_two_messages_in_order() {
        // The index, then GO. The other way round recalls whatever index was
        // last set, which is a wrong scene and no error.
        let wing = ALL
            .iter()
            .find(|preset| preset.name == "Behringer Wing")
            .expect("the Wing preset");
        let steps = (wing.build)();
        assert_eq!(steps.len(), 2);
        match (&steps[0].send, &steps[1].send) {
            (
                Message::Osc { address: first, .. },
                Message::Osc {
                    address: second,
                    args,
                },
            ) => {
                assert!(first.ends_with("actionidx"), "the index goes first");
                assert!(second.ends_with("action"));
                assert_eq!(args, &vec![OscArg::Str("GO".into())]);
            }
            _ => panic!("the Wing preset stopped being two OSC messages"),
        }
    }

    #[test]
    fn qlab_sends_no_arguments() {
        // The whole reason the argument list is a list.
        let qlab = ALL
            .iter()
            .find(|preset| preset.name == "QLab")
            .expect("the QLab preset");
        match &(qlab.build)()[0].send {
            Message::Osc { args, .. } => assert!(args.is_empty(), "QLab was given an argument"),
            other => panic!("QLab should be OSC, not {other:?}"),
        }
    }

    #[test]
    fn a_preset_with_no_port_is_one_that_does_not_use_the_network() {
        // MSC and program changes go down a MIDI cable. Offering an IP address
        // for them would send somebody looking for a network problem.
        for preset in ALL {
            let networked = (preset.build)()
                .iter()
                .any(|step| matches!(step.send, Message::Osc { .. }));
            assert_eq!(
                networked,
                preset.port.is_some(),
                "{} disagrees with itself about whether it uses the network",
                preset.name
            );
        }
    }
}
