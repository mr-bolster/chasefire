// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
//
// Copyright (c) Leo Bolster.

//! A cue is a moment in a show, not a wire.
//!
//! The old model had one action per cue and one socket for all of them, so the
//! moment that starts the video *and* moves the desk could not be written down
//! at all. This proves it can now — through the runner, with real UDP sockets
//! on the other end, because a model that only works in a struct is not a
//! feature.

use show::{Event, Runner, Source};
use std::net::UdpSocket;

fn listener() -> (UdpSocket, String) {
    let socket = UdpSocket::bind("127.0.0.1:0").unwrap();
    socket
        .set_read_timeout(Some(std::time::Duration::from_millis(500)))
        .unwrap();
    let address = socket.local_addr().unwrap().to_string();
    (socket, address)
}

fn address_of(packet: &[u8]) -> String {
    let end = packet.iter().position(|byte| *byte == 0).unwrap();
    String::from_utf8_lossy(&packet[..end]).into_owned()
}

fn heard(socket: &UdpSocket) -> Option<String> {
    let mut buffer = [0u8; 512];
    socket
        .recv_from(&mut buffer)
        .ok()
        .map(|(length, _)| address_of(&buffer[..length]))
}

/// Run the show past a cue sitting at 10:00:01:00.
fn run_past_the_cue(runner: &mut Runner) -> Vec<Event> {
    let mut events = Vec::new();
    for at in [
        ltc::Timecode::new(10, 0, 0, 24),
        ltc::Timecode::new(10, 0, 1, 0),
    ] {
        events.extend(runner.accept_timecode(at, Source::Ltc));
    }
    events
}

fn cue_at_one_second(name: &str, steps: Vec<cue::Step>) -> cue::Cue {
    cue::Cue::of(1, name, ltc::Timecode::new(10, 0, 1, 0), steps)
}

fn osc(address: &str, args: Vec<cue::OscArg>) -> cue::Message {
    cue::Message::Osc {
        address: address.into(),
        args,
    }
}

#[test]
fn one_moment_reaches_the_video_server_and_the_desk() {
    let (video, video_at) = listener();
    let (desk, desk_at) = listener();

    let mut runner = Runner::new(25);
    runner.connect_osc_as("video", &video_at).unwrap();
    runner.connect_osc_as("mesa", &desk_at).unwrap();
    runner.set_cues(vec![cue_at_one_second(
        "arranque",
        vec![
            cue::Step::to(
                "video",
                osc("/composition/columns/1/connect", vec![cue::OscArg::Int(1)]),
            ),
            cue::Step::to("mesa", osc("/-action/goscene", vec![cue::OscArg::Int(12)])),
        ],
    )]);
    runner.set_armed(true);

    let fired: Vec<_> = run_past_the_cue(&mut runner)
        .into_iter()
        .filter_map(|event| match event {
            Event::Fired { sent, .. } => Some(sent),
            _ => None,
        })
        .collect();

    assert_eq!(fired.len(), 1, "one cue, fired once");
    assert!(fired[0].is_ok(), "both messages should go: {:?}", fired[0]);
    assert_eq!(
        heard(&video).as_deref(),
        Some("/composition/columns/1/connect")
    );
    assert_eq!(heard(&desk).as_deref(), Some("/-action/goscene"));
}

#[test]
fn a_dead_destination_does_not_stop_the_live_one() {
    // The rule that matters on a stage: half a cue is bad, and half a cue that
    // could have been three quarters is worse. The failure is still reported —
    // swallowing it would mean a cue that silently does nothing all night.
    let (desk, desk_at) = listener();

    let mut runner = Runner::new(25);
    runner.connect_osc_as("mesa", &desk_at).unwrap();
    runner.set_cues(vec![cue_at_one_second(
        "arranque",
        vec![
            // Nothing is called "video". This step cannot go anywhere.
            cue::Step::to("video", osc("/composition/columns/1/connect", vec![])),
            cue::Step::to("mesa", osc("/-action/goscene", vec![cue::OscArg::Int(12)])),
        ],
    )]);
    runner.set_armed(true);

    let complaint = run_past_the_cue(&mut runner)
        .into_iter()
        .find_map(|event| match event {
            Event::Fired { sent: Err(why), .. } => Some(why),
            _ => None,
        })
        .expect("the failure should be reported, not swallowed");
    assert!(complaint.contains("video"), "should name it: {complaint}");

    assert_eq!(
        heard(&desk).as_deref(),
        Some("/-action/goscene"),
        "the desk should still have got its message"
    );
}

#[test]
fn the_wing_gets_its_two_messages_in_the_order_it_needs_them() {
    // Recalling one scene on a Behringer Wing takes two messages: the index,
    // then the word GO. Sent the other way round it does nothing. This is the
    // case that could not be expressed at all before.
    let (wing, wing_at) = listener();

    let mut runner = Runner::new(25);
    runner.connect_osc_as("wing", &wing_at).unwrap();
    runner.set_cues(vec![cue_at_one_second(
        "escena 12",
        vec![
            cue::Step::to(
                "wing",
                osc("/$ctl/lib/$actionidx", vec![cue::OscArg::Int(12)]),
            ),
            cue::Step::to(
                "wing",
                osc("/$ctl/lib/$action", vec![cue::OscArg::Str("GO".into())]),
            ),
        ],
    )]);
    runner.set_armed(true);
    run_past_the_cue(&mut runner);

    assert_eq!(heard(&wing).as_deref(), Some("/$ctl/lib/$actionidx"));
    assert_eq!(heard(&wing).as_deref(), Some("/$ctl/lib/$action"));
}

#[test]
fn a_cue_naming_nowhere_keeps_going_where_it_always_did() {
    // A show with one machine should never have to learn that outputs have
    // names — and adding a second output later must not silently steal its
    // cues, which would be a very quiet way to break somebody's show file.
    let (first, first_at) = listener();
    let (second, second_at) = listener();

    let mut runner = Runner::new(25);
    runner.connect_osc_as("principal", &first_at).unwrap();
    runner.connect_osc_as("segunda", &second_at).unwrap();
    runner.set_cues(vec![cue_at_one_second(
        "sin destino",
        vec![cue::Step::anywhere(osc("/go", vec![]))],
    )]);
    runner.set_armed(true);
    run_past_the_cue(&mut runner);

    assert_eq!(heard(&first).as_deref(), Some("/go"));
    assert_eq!(
        heard(&second),
        None,
        "the second output should not have seen it"
    );
}

#[test]
fn qlab_gets_an_address_and_nothing_else() {
    // QLab wants no arguments at all on /cue/5/start. The old model always
    // carried an integer, which is one media server's habit and not a cue
    // system. An extra 1 here is not harmless.
    let (qlab, qlab_at) = listener();

    let mut runner = Runner::new(25);
    runner.connect_osc_as("qlab", &qlab_at).unwrap();
    runner.set_cues(vec![cue_at_one_second(
        "arranca la 5",
        vec![cue::Step::anywhere(osc("/cue/5/start", vec![]))],
    )]);
    runner.set_armed(true);
    run_past_the_cue(&mut runner);

    let mut buffer = [0u8; 512];
    let (length, _) = qlab.recv_from(&mut buffer).unwrap();
    // Address, then an empty type tag string. Nothing after it.
    assert_eq!(&buffer[..length], b"/cue/5/start\0\0\0\0,\0\0\0");
}
