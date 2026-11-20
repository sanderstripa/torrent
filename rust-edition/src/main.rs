#![cfg_attr(windows, windows_subsystem = "windows")]

#[cfg(windows)]
mod windows_app;

#[cfg(windows)]
fn main() {
    if let Err(error) = windows_app::run() {
        if std::env::args_os()
            .any(|argument| argument == "--smoke-test" || argument == "--interaction-test")
        {
            eprintln!("{error}");
        } else {
            windows_app::show_error(&error);
        }
        std::process::exit(1);
    }
}

#[cfg(not(windows))]
fn main() {
    eprintln!("Torrent Rust Edition targets Windows.");
}
