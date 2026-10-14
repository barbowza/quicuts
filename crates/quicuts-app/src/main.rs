// GUI subsystem in debug builds too: the dev deploy is a debug build, and
// autostart at login would otherwise open a console window beside it. Logs
// still reach the log file (`just log`) via tauri-plugin-log.
#![windows_subsystem = "windows"]

fn main() {
    quicuts_app::run()
}
