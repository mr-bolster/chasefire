//! A cue, fired by timecode, arriving at a machine over the network.
//!
//! The RTP-MIDI session has its own tests and so does the MIDI encoding, but
//! the thing worth proving is the whole chain: a timecode position goes in one
//! end and MIDI bytes come out of a socket at the other. Every join between
//! those is somewhere a cue can quietly go nowhere.

use show::{Runner, Source};
use std::net::{SocketAddr, UdpSocket};
use std::time::{Duration, Instant};

fn socket(port: u16) -> UdpSocket {
    let socket = UdpSocket::bind(("127.0.0.1", port)).expect("a free port");
    socket
        .set_read_timeout(Some(Duration::from_millis(200)))
        .unwrap();
    socket
}

/// A pair of adjacent ports, handed out from one counter so that tests running
/// at the same time cannot pick the same pair.
fn free_pair() -> u16 {
    use std::sync::atomic::{AtomicU16, Ordering};
    static NEXT: AtomicU16 = AtomicU16::new(21600);
    loop {
        let port = NEXT.fetch_add(2, Ordering::SeqCst);
        assert!(port < 21900, "ran out of ports to try");
        if UdpSocket::bind(("127.0.0.1", port)).is_ok()
            && UdpSocket::bind(("127.0.0.1", port + 1)).is_ok()
        {
            return port;
        }
    }
}

/// Answer invitations the way a Mac or rtpMIDI would, so the session comes up.
fn accept_the_session(control: &UdpSocket, data: &UdpSocket) {
    for socket in [control, data] {
        let mut buffer = [0u8; 1500];
        let deadline = Instant::now() + Duration::from_secs(8);
        let mut agreed = false;
        while !agreed && Instant::now() < deadline {
            let Ok((length, from)) = socket.recv_from(&mut buffer) else {
                continue;
            };
            if let Some(rtpmidi::Control::Invitation { token, .. }) =
                rtpmidi::Control::parse(&buffer[..length])
            {
                let reply = rtpmidi::Control::Accepted {
                    token,
                    ssrc: 0x1234_5678,
                    name: "far end".into(),
                }
                .to_bytes();
                let _ = socket.send_to(&reply, from);
                agreed = true;
            }
        }
        assert!(agreed, "no invitation arrived on one of the ports");
    }
}

#[test]
fn a_timecode_position_becomes_midi_on_a_socket() {
    let their_port = free_pair();
    let their_control = socket(their_port);
    let their_data = socket(their_port + 1);
    let peer: SocketAddr = format!("127.0.0.1:{their_port}").parse().unwrap();

    let mut runner = Runner::new(25);
    runner
        .connect_network_midi_as("red", free_pair(), Some(&peer.to_string()))
        .expect("the session should have started");

    accept_the_session(&their_control, &their_data);

    // A Show Control GO — what a grandMA2 takes and nothing else will do.
    runner.set_cues(vec![cue::Cue::new(
        1,
        "MA2 cue 21.5",
        ltc::Timecode::new(10, 0, 1, 0),
        cue::Message::ShowControl(cue::ShowControl {
            device: 0x7F,
            format: 0x01,
            command: cue::ShowCommand::Go,
            cue: "21.5".into(),
            list: None,
        }),
    )]);
    runner.set_armed(true);

    // Give the session a moment to finish agreeing before the cue lands. A cue
    // fired into a session that is still shaking hands is reported as a
    // failure, which is correct behaviour and not what this test is about.
    let ready = Instant::now() + Duration::from_secs(4);
    while Instant::now() < ready && runner.output_described("red").is_none() {
        std::thread::sleep(Duration::from_millis(50));
    }
    std::thread::sleep(Duration::from_millis(600));

    let mut sent = None;
    for at in [
        ltc::Timecode::new(10, 0, 0, 24),
        ltc::Timecode::new(10, 0, 1, 0),
    ] {
        for event in runner.accept_timecode(at, Source::Ltc) {
            if let show::Event::Fired { sent: outcome, .. } = event {
                sent = Some(outcome);
            }
        }
    }
    assert_eq!(
        sent,
        Some(Ok(())),
        "the cue did not go out cleanly over the session"
    );

    // And the bytes themselves, on the far end's data port.
    let mut buffer = [0u8; 1500];
    let deadline = Instant::now() + Duration::from_secs(5);
    let mut heard = None;
    while heard.is_none() && Instant::now() < deadline {
        if let Ok((length, _)) = their_data.recv_from(&mut buffer) {
            // Session traffic shares the socket; the MIDI is what is left.
            if rtpmidi::Control::parse(&buffer[..length]).is_none() {
                heard = Some(buffer[..length].to_vec());
            }
        }
    }
    let packet = heard.expect("no MIDI packet reached the far end");
    assert_eq!(
        &packet[packet.len() - 11..],
        &[0xF0, 0x7F, 0x7F, 0x02, 0x01, 0x01, b'2', b'1', b'.', b'5', 0xF7],
        "a grandMA2 would not have recognised what arrived"
    );
}
