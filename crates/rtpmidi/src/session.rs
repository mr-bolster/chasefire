//! Keeping an RTP-MIDI session alive, on its own thread.
//!
//! The whole thing runs behind a channel. Firing a cue must never wait on a
//! network, and a session that is still shaking hands must never be the reason
//! a cue is late — so sending is a push into a queue that always succeeds, and
//! what happens on the wire is somebody else's problem.
//!
//! Two ports, always adjacent: the invitations and the clock go on the control
//! port, and the MIDI on the one after it. That is fixed by the protocol.

use crate::packet::{midi_packet, Control, Ports};
use std::net::{SocketAddr, UdpSocket};
use std::sync::atomic::{AtomicU8, Ordering};
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::Arc;
use std::time::{Duration, Instant};

/// How the session is getting on, for the window to show.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Status {
    /// Nobody on the other end yet: invited and waiting, or waiting to be
    /// invited.
    Waiting,
    /// Both ports agreed. Cues will go out.
    Joined,
    /// It was up and went away.
    Lost,
}

impl Status {
    fn code(self) -> u8 {
        match self {
            Status::Waiting => 0,
            Status::Joined => 1,
            Status::Lost => 2,
        }
    }

    fn from_code(code: u8) -> Self {
        match code {
            1 => Status::Joined,
            2 => Status::Lost,
            _ => Status::Waiting,
        }
    }
}

/// A live session. Dropping it says goodbye properly and stops the thread.
pub struct Session {
    outgoing: Sender<Vec<u8>>,
    status: Arc<AtomicU8>,
    name: String,
    listening_on: u16,
    peer: Option<SocketAddr>,
}

impl Session {
    /// Start one, listening on `port` and `port + 1`.
    ///
    /// With a `peer`, this end invites. Without, it waits to be invited, which
    /// is what a Mac or rtpMIDI does when somebody presses Connect over there.
    pub fn start(name: &str, port: u16, peer: Option<SocketAddr>) -> Result<Self, String> {
        let control =
            UdpSocket::bind(("0.0.0.0", port)).map_err(|error| format!("port {port}: {error}"))?;
        let data_port = port
            .checked_add(1)
            .ok_or("port 65535 has no room for the data port")?;
        let data = UdpSocket::bind(("0.0.0.0", data_port))
            .map_err(|error| format!("port {data_port}: {error}"))?;

        // Short timeouts rather than blocking reads: the thread has to notice
        // its own clock as well as the network.
        let tick = Duration::from_millis(200);
        control.set_read_timeout(Some(tick)).ok();
        data.set_read_timeout(Some(tick)).ok();

        let listening_on = control.local_addr().map(|at| at.port()).unwrap_or(port);
        let (outgoing, queue) = mpsc::channel();
        let status = Arc::new(AtomicU8::new(Status::Waiting.code()));

        let worker = Worker {
            control,
            data,
            queue,
            status: Arc::clone(&status),
            // The SSRC only has to be unlike anybody else's on the network.
            // Built from the clock and the port rather than from a random
            // number generator, which would be a dependency for eight bytes.
            ssrc: seed(port),
            token: seed(port).rotate_left(16),
            name: name.to_string(),
            peer: peer.and_then(Ports::from_control),
            joined_control: false,
            joined_data: false,
            remote_ssrc: None,
            remote_control: None,
            sequence: 0,
            started: Instant::now(),
            last_invite: None,
            last_sync: None,
        };
        std::thread::Builder::new()
            .name("chasefire-rtpmidi".into())
            .spawn(move || worker.run())
            .map_err(|error| error.to_string())?;

        Ok(Self {
            outgoing,
            status,
            name: name.to_string(),
            listening_on,
            peer,
        })
    }

    pub fn status(&self) -> Status {
        Status::from_code(self.status.load(Ordering::Relaxed))
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn port(&self) -> u16 {
        self.listening_on
    }

    pub fn peer(&self) -> Option<SocketAddr> {
        self.peer
    }

    /// Queue one MIDI message. Never blocks and never fails for a reason worth
    /// stopping a show over: if the session is not up the bytes are dropped,
    /// and the window already says the session is not up.
    pub fn send(&self, midi: &[u8]) -> Result<(), String> {
        self.outgoing
            .send(midi.to_vec())
            .map_err(|_| "the session thread has stopped".to_string())
    }
}

/// Something unlike anybody else's, without pulling in a random number crate
/// for the sake of eight bytes.
fn seed(port: u16) -> u32 {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|since| since.subsec_nanos() ^ since.as_secs() as u32)
        .unwrap_or(0x5EED_5EED);
    now.rotate_left(port as u32 % 32) ^ (port as u32) << 16 ^ 0x9E37_79B9
}

struct Worker {
    control: UdpSocket,
    data: UdpSocket,
    queue: Receiver<Vec<u8>>,
    status: Arc<AtomicU8>,
    ssrc: u32,
    token: u32,
    name: String,
    /// Set when this end is the one doing the inviting.
    peer: Option<Ports>,
    joined_control: bool,
    joined_data: bool,
    remote_ssrc: Option<u32>,
    /// Where the far end's control port is, once we know — either because we
    /// were told it or because it invited us from there.
    remote_control: Option<SocketAddr>,
    sequence: u16,
    started: Instant,
    last_invite: Option<Instant>,
    last_sync: Option<Instant>,
}

impl Worker {
    /// The session clock, in the units everything here counts in: 100 µs.
    fn now(&self) -> u64 {
        (self.started.elapsed().as_micros() / 100) as u64
    }

    fn run(mut self) {
        let mut buffer = [0u8; 1500];
        loop {
            // Invite, and keep inviting. A desk that was switched on after us
            // must not need somebody to come back and press a button.
            if self.peer.is_some() && !(self.joined_control && self.joined_data) {
                let due = self
                    .last_invite
                    .map(|at| at.elapsed() > Duration::from_secs(2))
                    .unwrap_or(true);
                if due {
                    self.invite();
                    self.last_invite = Some(Instant::now());
                }
            }

            // The clock exchange is what keeps a session alive. Stop and the
            // far end drops us, usually about a minute later and always in the
            // middle of something.
            if self.joined() {
                let interval = if self.started.elapsed() < Duration::from_secs(10) {
                    Duration::from_millis(1500)
                } else {
                    Duration::from_secs(10)
                };
                let due = self
                    .last_sync
                    .map(|at| at.elapsed() > interval)
                    .unwrap_or(true);
                if due {
                    self.begin_clock_sync();
                    self.last_sync = Some(Instant::now());
                }
            }

            // Anything waiting to go out — and the same call tells us whether
            // the Session was dropped. It has to be one call: asking a second
            // time to find that out *takes a message off the queue* and throws
            // it away, which is a cue that silently never fires. Found by
            // sending one down a real socket and waiting for it.
            loop {
                match self.queue.try_recv() {
                    Ok(midi) => self.send_midi(&midi),
                    Err(mpsc::TryRecvError::Empty) => break,
                    Err(mpsc::TryRecvError::Disconnected) => {
                        self.say_goodbye();
                        return;
                    }
                }
            }

            self.read(&mut buffer, true);
            self.read(&mut buffer, false);
        }
    }

    fn joined(&self) -> bool {
        self.joined_control && self.joined_data
    }

    fn note_status(&self) {
        let status = if self.joined() {
            Status::Joined
        } else {
            Status::Waiting
        };
        self.status.store(status.code(), Ordering::Relaxed);
    }

    fn invite(&mut self) {
        let Some(ports) = self.peer else { return };
        let invitation = Control::Invitation {
            token: self.token,
            ssrc: self.ssrc,
            name: self.name.clone(),
        }
        .to_bytes();
        if !self.joined_control {
            let _ = self.control.send_to(&invitation, ports.control);
        } else if !self.joined_data {
            // The data port is only invited once the control port has agreed;
            // inviting both at once is how sessions end up half open.
            let _ = self.data.send_to(&invitation, ports.data);
        }
    }

    fn begin_clock_sync(&mut self) {
        let Some(ports) = self.peer.or_else(|| self.learned_ports()) else {
            return;
        };
        let sync = Control::ClockSync {
            ssrc: self.ssrc,
            count: 0,
            times: [self.now(), 0, 0],
        }
        .to_bytes();
        let _ = self.data.send_to(&sync, ports.data);
    }

    /// Where the far end is, when it invited us rather than the other way round.
    fn learned_ports(&self) -> Option<Ports> {
        self.remote_control.and_then(Ports::from_control)
    }

    fn send_midi(&mut self, midi: &[u8]) {
        if !self.joined() {
            return;
        }
        let Some(ports) = self.peer.or_else(|| self.learned_ports()) else {
            return;
        };
        self.sequence = self.sequence.wrapping_add(1);
        if let Some(packet) = midi_packet(self.ssrc, self.sequence, self.now() as u32, midi) {
            let _ = self.data.send_to(&packet, ports.data);
        }
    }

    fn say_goodbye(&mut self) {
        let bye = Control::Goodbye {
            token: self.token,
            ssrc: self.ssrc,
        }
        .to_bytes();
        if let Some(ports) = self.peer.or_else(|| self.learned_ports()) {
            let _ = self.control.send_to(&bye, ports.control);
            let _ = self.data.send_to(&bye, ports.data);
        }
    }

    fn read(&mut self, buffer: &mut [u8], on_control: bool) {
        let socket = if on_control {
            &self.control
        } else {
            &self.data
        };
        let Ok((length, from)) = socket.recv_from(buffer) else {
            return;
        };
        let Some(message) = Control::parse(&buffer[..length]) else {
            return;
        };
        self.handle(message, from, on_control);
    }

    fn handle(&mut self, message: Control, from: SocketAddr, on_control: bool) {
        match message {
            // Somebody wants to join us. Which end goes first is not up to us:
            // a Mac invites, rtpMIDI invites, Companion can do either.
            Control::Invitation { token, ssrc, .. } => {
                let reply = Control::Accepted {
                    token,
                    ssrc: self.ssrc,
                    name: self.name.clone(),
                }
                .to_bytes();
                let socket = if on_control {
                    &self.control
                } else {
                    &self.data
                };
                let _ = socket.send_to(&reply, from);
                self.remote_ssrc = Some(ssrc);
                if on_control {
                    self.joined_control = true;
                    self.remote_control = Some(from);
                } else {
                    self.joined_data = true;
                }
                self.note_status();
            }
            // Our invitation was taken.
            Control::Accepted { ssrc, .. } => {
                self.remote_ssrc = Some(ssrc);
                if on_control {
                    self.joined_control = true;
                    self.remote_control = Some(from);
                    // Now, and only now, invite the data port.
                    self.last_invite = None;
                } else {
                    self.joined_data = true;
                }
                self.note_status();
            }
            Control::Refused { .. } => {
                self.joined_control = false;
                self.joined_data = false;
                self.note_status();
            }
            Control::Goodbye { .. } => {
                self.joined_control = false;
                self.joined_data = false;
                self.status.store(Status::Lost.code(), Ordering::Relaxed);
            }
            // Leg 0 from them: answer with leg 1 and our own clock. Leg 2 needs
            // nothing back. Answering is not optional — a far end that gets no
            // reply drops the session.
            Control::ClockSync { count, times, .. } => {
                if count == 0 {
                    let reply = Control::ClockSync {
                        ssrc: self.ssrc,
                        count: 1,
                        times: [times[0], self.now(), 0],
                    }
                    .to_bytes();
                    let socket = if on_control {
                        &self.control
                    } else {
                        &self.data
                    };
                    let _ = socket.send_to(&reply, from);
                } else if count == 1 {
                    let reply = Control::ClockSync {
                        ssrc: self.ssrc,
                        count: 2,
                        times: [times[0], times[1], self.now()],
                    }
                    .to_bytes();
                    let socket = if on_control {
                        &self.control
                    } else {
                        &self.data
                    };
                    let _ = socket.send_to(&reply, from);
                }
            }
            // We keep no journal, so there is nothing to drop.
            Control::ReceiverFeedback { .. } => {}
        }
    }
}
