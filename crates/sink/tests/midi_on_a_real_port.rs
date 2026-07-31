//! MIDI, out of a real port and into a real listener.
//!
//! The encoder has its own tests, but bytes that are right in a `Vec` and never
//! reach a cable are worth nothing on a stage. This opens a virtual port,
//! listens on it, and checks what actually arrives.
//!
//! Virtual ports are an ALSA/CoreMIDI feature, so on Windows there is nothing
//! to bind to and the test skips itself rather than failing — an honest skip
//! beats a red CI nobody trusts.

#[cfg(unix)]
mod unix_only {
    use midir::os::unix::VirtualInput;
    use std::sync::mpsc;
    use std::time::Duration;

    /// Open a virtual port and something to listen to it with. `None` when this
    /// machine has no MIDI stack, which is a skip and not a failure.
    fn loopback(
        name: &str,
    ) -> Option<(
        sink::MidiSink,
        mpsc::Receiver<Vec<u8>>,
        midir::MidiInputConnection<()>,
    )> {
        let input = midir::MidiInput::new("chasefire-test-in").ok()?;
        let (sender, receiver) = mpsc::channel();
        let connection = input
            .create_virtual(
                name,
                move |_stamp, bytes, _| {
                    let _ = sender.send(bytes.to_vec());
                },
                (),
            )
            .ok()?;
        // Give the sequencer a moment to publish the port before looking for it.
        std::thread::sleep(Duration::from_millis(200));
        let out = sink::MidiSink::open(name).ok()?;
        Some((out, receiver, connection))
    }

    /// Everything that arrives before it goes quiet. A port may deliver one
    /// packet or several; either is correct, so the bytes are what matter.
    fn everything_heard(receiver: &mpsc::Receiver<Vec<u8>>) -> Vec<u8> {
        let mut all = Vec::new();
        while let Ok(chunk) = receiver.recv_timeout(Duration::from_millis(500)) {
            all.extend(chunk);
        }
        all
    }

    #[test]
    fn show_control_arrives_the_way_a_desk_expects_it() {
        let Some((mut out, receiver, _keep)) = loopback("chasefire-msc") else {
            eprintln!("no MIDI stack here; skipping");
            return;
        };

        sink::Sink::deliver(
            &mut out,
            &cue::Message::ShowControl(cue::ShowControl {
                device: 3,
                format: 0x01,
                command: cue::ShowCommand::Go,
                cue: "21.5".into(),
                list: None,
            }),
        )
        .expect("should have sent");

        assert_eq!(
            everything_heard(&receiver),
            vec![0xF0, 0x7F, 3, 0x02, 0x01, 0x01, b'2', b'1', b'.', b'5', 0xF7],
            "what left the port is not what a desk is listening for"
        );
    }

    #[test]
    fn a_program_change_with_a_bank_arrives_in_the_right_order() {
        let Some((mut out, receiver, _keep)) = loopback("chasefire-bank") else {
            eprintln!("no MIDI stack here; skipping");
            return;
        };

        // How SuperRack reaches a snapshot past 128.
        sink::Sink::deliver(
            &mut out,
            &cue::Message::MidiProgramChange {
                channel: 1,
                program: 5,
                bank: Some((0, 2)),
            },
        )
        .expect("should have sent");

        assert_eq!(
            everything_heard(&receiver),
            vec![0xB0, 0x00, 0, 0xB0, 0x20, 2, 0xC0, 5],
            "the bank must arrive before the program it selects"
        );
    }
}
