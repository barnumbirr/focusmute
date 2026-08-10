//! FocusMute CLI — hotkey mute control for Focusrite Scarlett 4th Gen interfaces.
//!
//! Console subsystem: works normally in PowerShell, cmd, and other terminals.

use std::path::PathBuf;
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

    /// Path to a custom config file (default: platform config directory)
    #[arg(long, global = true)]
    config: Option<PathBuf>,

    #[command(subcommand)]
    command: cli::Command,
}

// ── Ctrl+C handler ──

/// SAFETY: Called by the Windows console subsystem with a valid control type.
/// Only touches a static `AtomicBool` (lock-free, no aliasing concerns).
#[cfg(windows)]
unsafe extern "system" fn ctrl_handler(_ctrl_type: u32) -> windows::core::BOOL {
    RUNNING.store(false, Ordering::SeqCst);
    windows::core::BOOL(1)
}

fn main() {
    let args = Args::parse();

    let default_level = if args.verbose { "debug" } else { "warn" };
    flexi_logger::Logger::try_with_str(default_level)
        .unwrap_or_else(|_| flexi_logger::Logger::try_with_str("warn").unwrap())
        .format(flexi_logger::opt_format)
        .start()
        .ok();

    // Install Ctrl+C handler
    #[cfg(windows)]
    // SAFETY: `ctrl_handler` has the correct `extern "system"` signature for
    // SetConsoleCtrlHandler and only writes to a static AtomicBool.
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

    if let Err(e) = cli::run(args.command, args.json, args.config.as_deref()) {
        eprintln!("Error: {e}");
        std::process::exit(1);
    }
}
