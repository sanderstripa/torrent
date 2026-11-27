#![cfg_attr(windows, windows_subsystem = "windows")]

#[cfg(windows)]
mod installer_ui;
#[cfg(windows)]
mod installer_windows;
#[cfg(windows)]
mod windows_canvas;

#[cfg(windows)]
fn main() {
    if let Err(error) = installer_windows::run() {
        if std::env::args_os().nth(1).is_none() {
            installer_windows::message(&error.to_string(), true);
        }
        std::process::exit(1);
    }
}

#[cfg(not(windows))]
fn main() {
    eprintln!("Torrent Rust Edition Setup targets Windows.");
    std::process::exit(1);
}
