// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
//
// Copyright (c) Leo Bolster.

//! A session against something that answers back.
//!
//! The packet encoding has its own tests, but a handshake is a conversation and
//! conversations go wrong in ways no single message shows. The far end here is
//! a pair of plain sockets doing the minimum a Mac or rtpMIDI would do, which
//! is the only way to find out whether the invitation actually lands, whether
//! the data port is invited only after the control port agrees, and whether a
//! note ever reaches the wire.

use rtpmidi::{Control, Session, Status};
use std::net::{SocketAddr, UdpSocket};
use std::time::{Duration, Instant};

/// A socket that will not sit there for ever.
fn socket(port: u16) -> UdpSocket {
    let socket = UdpSocket::bind(("127.0.0.1", port)).expect("a free port");
    socket
        .set_read_timeout(Some(Duration::from_millis(200)))
        .unwrap();
    socket
}

/// Wait for a control message, or give up and say what we were waiting for.
fn expect(socket: &UdpSocket, what: &str) -> (Control, SocketAddr) {
    let mut buffer = [0u8; 1500];
    let deadline = Instant::now() + Duration::from_secs(8);
    while Instant::now() < deadline {
        if let Ok((length, from)) = socket.recv_from(&mut buffer) {
            if let Some(message) = Control::parse(&buffer[..length]) {
                return (message, from);
            }
        }
    }
    panic!("nothing that looked like {what} arrived");
}

/// Wait for something that is *not* a session command: an RTP packet.
fn expect_rtp(socket: &UdpSocket) -> Vec<u8> {
    let mut buffer = [0u8; 1500];
    let deadline = Instant::now() + Duration::from_secs(8);
    while Instant::now() < deadline {
        if let Ok((length, _)) = socket.recv_from(&mut buffer) {
            if Control::parse(&buffer[..length]).is_none() {
                return buffer[..length].to_vec();
            }
        }
    }
    panic!("no MIDI packet arrived");
}

/// Two adjacent ports nobody else on this machine is using.
///
/// Handed out from one counter shared by every test in this file, because
/// tests run at the same time and "look for a free port, let go of it, then
/// bind it" is a race two of them will eventually lose together.
fn free_pair() -> u16 {
    use std::sync::atomic::{AtomicU16, Ordering};
    static NEXT: AtomicU16 = AtomicU16::new(21000);
    loop {
        let port = NEXT.fetch_add(2, Ordering::SeqCst);
        assert!(port < 21500, "ran out of ports to try");
        // Held only long enough to know they are free; the race with our own
        // tests is what the counter removes, and nothing else on the machine
        // is likely to want this range.
        let first = UdpSocket::bind(("127.0.0.1", port));
        let second = UdpSocket::bind(("127.0.0.1", port + 1));
        if first.is_ok() && second.is_ok() {
            return port;
        }
    }
}

#[test]
fn it_invites_a_machine_and_then_sends_it_a_note() {
    let their_port = free_pair();
    let their_control = socket(their_port);
    let their_data = socket(their_port + 1);

    let our_port = free_pair();
    let session = Session::start(
        "Chasefire",
        our_port,
        Some(format!("127.0.0.1:{their_port}").parse().unwrap()),
    )
    .expect("should have started");

    // It invites the control port first.
    let (message, from) = expect(&their_control, "an invitation on the control port");
    let token = match message {
        Control::Invitation { token, name, .. } => {
            assert_eq!(name, "Chasefire", "sessions are picked by name over there");
            token
        }
        other => panic!("expected an invitation, got {other:?}"),
    };
    their_control
        .send_to(
            &Control::Accepted {
                token,
                ssrc: 0xABCD,
                name: "far end".into(),
            }
            .to_bytes(),
            from,
        )
        .unwrap();

    // Only now does the data port get invited. Inviting both at once is how
    // sessions end up half open, which looks exactly like a working one until
    // the first cue goes nowhere.
    let (message, from) = expect(&their_data, "an invitation on the data port");
    match message {
        Control::Invitation { token, .. } => {
            their_data
                .send_to(
                    &Control::Accepted {
                        token,
                        ssrc: 0xABCD,
                        name: "far end".into(),
                    }
                    .to_bytes(),
                    from,
                )
                .unwrap();
        }
        other => panic!("expected an invitation, got {other:?}"),
    }

    // The session should now say it is up.
    let deadline = Instant::now() + Duration::from_secs(5);
    while session.status() != Status::Joined && Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(50));
    }
    assert_eq!(session.status(), Status::Joined, "never came up");

    session.send(&[0x90, 60, 127]).unwrap();

    let packet = expect_rtp(&their_data);
    assert_eq!(packet[0], 0x80, "RTP version 2");
    assert_eq!(packet[1], 0xE1, "MIDI payload type");
    assert_eq!(
        &packet[packet.len() - 3..],
        &[0x90, 60, 127],
        "the note itself"
    );
}

#[test]
fn it_accepts_an_invitation_from_the_other_end() {
    // Which end goes first is not ours to choose: a Mac invites, rtpMIDI
    // invites. Refusing to be invited would mean not working with either.
    let our_port = free_pair();
    let session = Session::start("Chasefire", our_port, None).expect("should have started");

    let ours: SocketAddr = format!("127.0.0.1:{our_port}").parse().unwrap();
    let their = socket(free_pair());
    their
        .send_to(
            &Control::Invitation {
                token: 0x1234,
                ssrc: 0xFEED,
                name: "a Mac".into(),
            }
            .to_bytes(),
            ours,
        )
        .unwrap();

    let (message, _) = expect(&their, "an acceptance");
    match message {
        Control::Accepted { token, name, .. } => {
            assert_eq!(token, 0x1234, "the token has to come back unchanged");
            assert_eq!(name, "Chasefire");
        }
        other => panic!("expected an acceptance, got {other:?}"),
    }
    let _ = session;
}

#[test]
fn it_answers_the_clock_because_silence_gets_a_session_dropped() {
    // A far end that gets no reply to its clock sync drops the session, and it
    // does it a minute later — which is to say, in the middle of something.
    let our_port = free_pair();
    let session = Session::start("Chasefire", our_port, None).expect("should have started");
    let ours: SocketAddr = format!("127.0.0.1:{our_port}").parse().unwrap();

    let their = socket(free_pair());
    their
        .send_to(
            &Control::ClockSync {
                ssrc: 0xFEED,
                count: 0,
                times: [1234, 0, 0],
            }
            .to_bytes(),
            ours,
        )
        .unwrap();

    let (message, _) = expect(&their, "the second leg of the clock exchange");
    match message {
        Control::ClockSync { count, times, .. } => {
            assert_eq!(count, 1, "the reply is leg one");
            assert_eq!(times[0], 1234, "our timestamp has to come back untouched");
        }
        other => panic!("expected a clock sync, got {other:?}"),
    }
    let _ = session;
}

/// The far end vanishes without saying goodbye — cable out, machine off.
///
/// Ignored by default because it has to wait out the real timeout, and half a
/// minute on every commit is a tax nobody should pay. Run it by hand:
/// `cargo test -p rtpmidi -- --ignored --nocapture`
#[test]
#[ignore]
fn a_session_that_goes_quiet_stops_claiming_to_be_up() {
    let their_port = free_pair();
    let their_control = socket(their_port);
    let their_data = socket(their_port + 1);

    let our_port = free_pair();
    let session = Session::start(
        "Chasefire",
        our_port,
        Some(format!("127.0.0.1:{their_port}").parse().unwrap()),
    )
    .unwrap();

    for what in ["control", "data"] {
        let socket = if what == "control" {
            &their_control
        } else {
            &their_data
        };
        let (message, from) = expect(socket, what);
        if let Control::Invitation { token, .. } = message {
            socket
                .send_to(
                    &Control::Accepted {
                        token,
                        ssrc: 0xABCD,
                        name: "far end".into(),
                    }
                    .to_bytes(),
                    from,
                )
                .unwrap();
        }
    }
    let deadline = Instant::now() + Duration::from_secs(5);
    while session.status() != Status::Joined && Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(50));
    }
    assert_eq!(session.status(), Status::Joined, "never came up");

    // Gone. No goodbye, no reply to the clock, nothing.
    drop(their_control);
    drop(their_data);

    let deadline = Instant::now() + rtpmidi::session::SILENCE_MEANS_GONE + Duration::from_secs(10);
    while session.status() == Status::Joined && Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(500));
    }
    assert_eq!(
        session.status(),
        Status::Lost,
        "still claiming to be up after the far end went away"
    );
}
