#![cfg_attr(windows, windows_subsystem = "windows")]

#[cfg(windows)]
mod windows_app;
#[cfg(windows)]
mod windows_paint;
#[cfg(all(windows, feature = "rqbit", feature = "vpn"))]
mod windows_vpn;

#[cfg(windows)]
fn main() {
    if std::env::args_os().any(|argument| argument == "--interaction-test") {
        std::panic::set_hook(Box::new(|info| {
            let detail = format!("{info}\n{}", std::backtrace::Backtrace::force_capture());
            let _ = std::fs::write("re-ui-panic.txt", detail);
        }));
    }
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
