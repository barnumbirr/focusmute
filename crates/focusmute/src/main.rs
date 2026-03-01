//! FocusMute — hotkey mute control for Focusrite Scarlett 4th Gen interfaces.
//!
//! GUI subsystem: double-click from Explorer launches the system tray.
//! If run from a terminal with arguments, redirects the user to focusmute-cli.

#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

#[cfg(any(windows, target_os = "linux"))]
mod icon;
#[cfg(any(windows, target_os = "linux"))]
mod settings_dialog;
#[cfg(any(windows, target_os = "linux"))]
mod sound;
#[cfg(any(windows, target_os = "linux"))]
mod tray;

#[cfg(any(windows, target_os = "linux"))]
use std::sync::atomic::AtomicBool;

/// Shared shutdown flag — set by tray quit.
#[cfg(any(windows, target_os = "linux"))]
pub static RUNNING: AtomicBool = AtomicBool::new(true);

/// Check if we were launched from an interactive console (PowerShell, cmd, etc.).
#[cfg(windows)]
fn has_parent_console() -> bool {
    use windows::Win32::System::Console::{ATTACH_PARENT_PROCESS, AttachConsole, FreeConsole};

    unsafe {
        if AttachConsole(ATTACH_PARENT_PROCESS).is_ok() {
            // We successfully attached — there's a parent console.
            // Detach immediately since this is the tray binary.
            let _ = FreeConsole();
            true
        } else {
            false
        }
    }
}

/// Initialize the tray app logger, directing output to a log file.
///
/// Falls back to stderr if the log file can't be opened.
fn init_tray_logger() {
    use focusmute_lib::config::Config;

    let mut builder =
        env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info"));
    builder.format_target(false);

    if let Some(log_path) = Config::log_path() {
        if let Some(dir) = log_path.parent() {
            let _ = std::fs::create_dir_all(dir);
        }
        if let Ok(file) = std::fs::File::create(&log_path) {
            builder.target(env_logger::Target::Pipe(Box::new(file)));
        }
    }

    builder.init();
}

fn main() {
    init_tray_logger();

    #[cfg(not(any(windows, target_os = "linux")))]
    {
        eprintln!("The tray app is only available on Windows and Linux.");
        eprintln!("Use focusmute-cli for command-line usage.");
        std::process::exit(1);
    }

    #[cfg(windows)]
    {
        let args: Vec<String> = std::env::args().collect();

        // If launched with CLI arguments from a terminal, redirect to focusmute-cli
        if args.len() > 1 && has_parent_console() {
            eprintln!("Hint: Use focusmute-cli.exe for command-line usage.");
            eprintln!("  Example: focusmute-cli.exe {}", args[1..].join(" "));
            return;
        }
    }

    #[cfg(any(windows, target_os = "linux"))]
    {
        if let Err(e) = tray::run() {
            let msg = format!("Error: {e}");
            eprintln!("{msg}");
            show_fatal_error(&msg);
            std::process::exit(1);
        }
    }
}

/// Show a fatal error to the user. On Windows, displays a MessageBox since the
/// tray binary has no console. On other platforms, the eprintln above suffices.
#[cfg(windows)]
fn show_fatal_error(msg: &str) {
    use windows::Win32::UI::WindowsAndMessaging::{MB_ICONERROR, MB_OK, MessageBoxW};
    use windows::core::PCWSTR;

    let wide_msg: Vec<u16> = msg.encode_utf16().chain(std::iter::once(0)).collect();
    let title: Vec<u16> = "FocusMute"
        .encode_utf16()
        .chain(std::iter::once(0))
        .collect();
    unsafe {
        let _ = MessageBoxW(
            None,
            PCWSTR(wide_msg.as_ptr()),
            PCWSTR(title.as_ptr()),
            MB_ICONERROR | MB_OK,
        );
    }
}

#[cfg(not(windows))]
fn show_fatal_error(_msg: &str) {
    // On non-Windows platforms, eprintln above is visible from the terminal.
}
