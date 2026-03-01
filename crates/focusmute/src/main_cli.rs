//! Focusmute CLI — hotkey mute control for Focusrite Scarlett 4th Gen interfaces.
//!
//! Console subsystem: works normally in PowerShell, cmd, and other terminals.

use std::sync::atomic::{AtomicBool, Ordering};

use clap::Parser;

mod cli;

/// Shared shutdown flag — set by Ctrl+C handler.
pub static RUNNING: AtomicBool = AtomicBool::new(true);

#[derive(Parser)]
#[command(
    name = "focusmute-cli",
    version,
    about = "Hotkey mute control for Focusrite Scarlett 4th Gen interfaces"
)]
struct Args {
    /// Output as JSON (for status, config, devices, predict)
    #[arg(long, global = true)]
    json: bool,

    /// Enable verbose (debug-level) logging
    #[arg(long, short = 'v', global = true)]
    verbose: bool,

    #[command(subcommand)]
    command: cli::Command,
}

// ── Ctrl+C handler ──

#[cfg(windows)]
unsafe extern "system" fn ctrl_handler(_ctrl_type: u32) -> windows::core::BOOL {
    RUNNING.store(false, Ordering::SeqCst);
    windows::core::BOOL(1)
}

fn main() {
    let args = Args::parse();

    let default_level = if args.verbose { "debug" } else { "warn" };
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or(default_level))
        .format_timestamp(None)
        .format_target(false)
        .init();

    // Install Ctrl+C handler
    #[cfg(windows)]
    unsafe {
        let _ = windows::Win32::System::Console::SetConsoleCtrlHandler(Some(ctrl_handler), true);
    }

    #[cfg(not(windows))]
    {
        ctrlc::set_handler(move || {
            RUNNING.store(false, Ordering::SeqCst);
        })
        .ok();
    }

    if let Err(e) = cli::run(args.command, args.json) {
        eprintln!("Error: {e}");
        std::process::exit(1);
    }
}
