//! Quicuts macOS system agent (sidecar).
//!
//! Deliberately isolated from the main app: this is the only Quicuts binary
//! that installs a global event tap. It receives its entire configuration
//! over stdin and reports only semantic transitions over stdout — raw
//! keystrokes never leave this process (see quicuts-proto docs).

mod activation;

#[cfg(target_os = "macos")]
mod foreground;
#[cfg(target_os = "macos")]
mod ipc;
#[cfg(target_os = "macos")]
mod state;
#[cfg(target_os = "macos")]
mod tap;

#[cfg(target_os = "macos")]
fn main() -> anyhow::Result<()> {
    tap::run()
}

#[cfg(not(target_os = "macos"))]
fn main() {
    eprintln!("quicuts-agent-mac only runs on macOS");
    std::process::exit(1);
}
