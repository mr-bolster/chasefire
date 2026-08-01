// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
//
// Copyright (c) Leo Bolster.

//! A show file written this morning has to open this afternoon.
//!
//! The cue model grew from "one message" to "a list of messages, each with a
//! destination" because a single on/off integer is not a cue system — QLab
//! wants no arguments at all, grandMA3 wants a string, and a Behringer Wing
//! needs two messages in order to recall one scene. None of that is a licence
//! to break somebody's cue list.

use cue::{Cue, Message, OscArg, Step};

/// Exactly what version 0.1 wrote. Do not tidy this up: it is evidence.
const WRITTEN_BY_THE_OLD_VERSION: &str = r#"[
  {
    "id": 1,
    "name": "arranque",
    "at": { "hours": 10, "minutes": 0, "seconds": 0, "frames": 0, "drop_frame": false },
    "enabled": true,
    "action": { "Osc": { "address": "/composition/columns/1/connect", "args": [ { "Int": 1 } ] } }
  },
  {
    "id": 2,
    "name": "negro",
    "at": { "hours": 10, "minutes": 5, "seconds": 0, "frames": 0, "drop_frame": false },
    "enabled": false,
    "action": { "Osc": { "address": "/composition/disconnectall", "args": [] } }
  }
]"#;

#[test]
fn a_cue_list_from_the_old_version_opens_unchanged() {
    let cues: Vec<Cue> = serde_json::from_str(WRITTEN_BY_THE_OLD_VERSION).expect("should open");

    assert_eq!(cues.len(), 2);
    assert_eq!(cues[0].name, "arranque");
    assert_eq!(cues[0].steps.len(), 1, "one message became one step");
    assert!(
        cues[0].steps[0].to.is_none(),
        "and it goes where it always did"
    );
    assert_eq!(
        cues[0].steps[0].send,
        Message::Osc {
            address: "/composition/columns/1/connect".into(),
            args: vec![OscArg::Int(1)],
        }
    );

    // The one that was switched off must still be switched off. Loading a cue
    // list and having a disabled cue quietly come back to life is the kind of
    // thing that fires a blackout in the middle of a song.
    assert!(cues[0].enabled);
    assert!(!cues[1].enabled, "a disabled cue came back armed");
    assert_eq!(
        cues[1].steps[0].send,
        Message::Osc {
            address: "/composition/disconnectall".into(),
            args: vec![],
        }
    );
}

#[test]
fn what_it_writes_is_what_it_reads() {
    let wing = Cue::of(
        7,
        "escena 12 en la Wing",
        ltc::Timecode::new(10, 30, 0, 0),
        vec![
            // The Wing takes two messages to recall one scene: the index, then
            // the word GO. This is the case the old model could not express.
            Step::to(
                "wing",
                Message::Osc {
                    address: "/$ctl/lib/$actionidx".into(),
                    args: vec![OscArg::Int(12)],
                },
            ),
            Step::to(
                "wing",
                Message::Osc {
                    address: "/$ctl/lib/$action".into(),
                    args: vec![OscArg::Str("GO".into())],
                },
            ),
        ],
    );

    let text = serde_json::to_string_pretty(&[&wing]).unwrap();
    let back: Vec<Cue> = serde_json::from_str(&text).unwrap();

    assert_eq!(back[0], wing);
    assert_eq!(back[0].steps.len(), 2, "the order and the count survive");
    assert_eq!(back[0].steps[1].to.as_deref(), Some("wing"));
}

#[test]
fn a_cue_that_sends_nothing_is_refused_rather_than_loaded_empty() {
    // A cue with no messages would sit in the list looking armed and do
    // nothing at all when its moment came. Better to say so while the file is
    // being opened, when somebody is looking at the screen.
    let broken = r#"[{
        "id": 1, "name": "vacia",
        "at": { "hours": 1, "minutes": 0, "seconds": 0, "frames": 0, "drop_frame": false },
        "enabled": true, "steps": []
    }]"#;
    let outcome: Result<Vec<Cue>, _> = serde_json::from_str(broken);
    let error = outcome.expect_err("should have refused");
    assert!(
        error.to_string().contains("vacia"),
        "the complaint should name the cue: {error}"
    );
}

#[test]
fn an_old_file_missing_enabled_altogether_loads_armed() {
    // Some hand-written lists leave it out. The safe reading is the one the UI
    // shows by default, which is on.
    let terse = r#"[{
        "id": 3, "name": "a mano",
        "at": { "hours": 2, "minutes": 0, "seconds": 0, "frames": 0, "drop_frame": false },
        "action": { "Osc": { "address": "/go", "args": [] } }
    }]"#;
    let cues: Vec<Cue> = serde_json::from_str(terse).expect("should open");
    assert!(cues[0].enabled);
    assert_eq!(
        cues[0].steps[0].send,
        Message::Osc {
            address: "/go".into(),
            args: vec![]
        }
    );
}
