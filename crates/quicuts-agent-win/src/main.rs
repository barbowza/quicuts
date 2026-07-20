//! Quicuts Windows system agent (sidecar).
//!
//! Deliberately isolated from the main app: this is the only Quicuts binary
//! that installs global hooks. It receives its entire configuration over
//! stdin and reports only semantic transitions over stdout — raw keystrokes
//! never leave this process (see quicuts-proto docs).
#![cfg_attr(all(windows, not(debug_assertions)), windows_subsystem = "windows")]

#[cfg(windows)]
mod foreground;
#[cfg(windows)]
mod hook;
#[cfg(windows)]
mod ipc;
#[cfg(windows)]
mod state;
#[cfg(windows)]
mod taskbar;

#[cfg(windows)]
fn main() -> anyhow::Result<()> {
    hook::run()
}

#[cfg(not(windows))]
fn main() {
    eprintln!("quicuts-agent-win only runs on Windows");
    std::process::exit(1);
}
