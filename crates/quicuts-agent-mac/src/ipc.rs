//! NDJSON stdin/stdout codec + writer channel.

use std::io::{BufRead, Write};
use std::sync::mpsc::{Receiver, Sender};

use quicuts_proto::{AgentCommand, AgentEvent};

/// Handle used everywhere to emit events. Cheap to clone.
#[derive(Clone)]
pub struct EventSink(Sender<AgentEvent>);

impl EventSink {
    pub fn send(&self, ev: AgentEvent) {
        let _ = self.0.send(ev);
    }
}

/// Spawn the stdout writer thread. Returns a sink; events are flushed one per
/// line as they arrive.
pub fn spawn_writer() -> EventSink {
    let (tx, rx): (Sender<AgentEvent>, Receiver<AgentEvent>) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let stdout = std::io::stdout();
        for ev in rx {
            let mut lock = stdout.lock();
            if let Ok(line) = quicuts_proto::to_line(&ev) {
                let _ = writeln!(lock, "{line}");
                let _ = lock.flush();
            }
        }
    });
    EventSink(tx)
}

/// Spawn the stdin reader thread; parsed commands are delivered on the
/// returned channel. Sends `None`-equivalent by closing on EOF (parent gone).
pub fn spawn_reader() -> Receiver<AgentCommand> {
    let (tx, rx) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let stdin = std::io::stdin();
        for line in stdin.lock().lines() {
            let Ok(line) = line else { break };
            if line.trim().is_empty() {
                continue;
            }
            match quicuts_proto::from_line::<AgentCommand>(&line) {
                Ok(cmd) => {
                    if tx.send(cmd).is_err() {
                        break;
                    }
                }
                Err(e) => eprintln!("agent: bad command line: {e}"),
            }
        }
        // stdin closed => the app exited; drop sender so the pump can exit.
    });
    rx
}
