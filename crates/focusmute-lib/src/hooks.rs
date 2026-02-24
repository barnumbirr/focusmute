//! Mute state change hooks — run user-defined commands on mute/unmute events.

use std::io;
use std::process::ExitStatus;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use crate::config::Config;
use crate::monitor::MonitorAction;

/// Guard preventing concurrent hook execution (shared across mute/unmute hooks).
static HOOK_RUNNING: AtomicBool = AtomicBool::new(false);

/// Default timeout for hook commands (30 seconds).
const HOOK_TIMEOUT: Duration = Duration::from_secs(30);

/// Poll interval when waiting for a hook process to exit.
const POLL_INTERVAL: Duration = Duration::from_millis(100);

/// Run the appropriate hook command for a mute state change.
///
/// Spawns the command in a background thread so it doesn't block the event loop.
/// Empty commands are silently ignored. Only one hook can run at a time — if a
/// previous hook is still running, the new one is skipped with a warning.
pub fn run_action_hook(action: MonitorAction, config: &Config) {
    match action {
        MonitorAction::ApplyMute => run_hook(&config.on_mute_command),
        MonitorAction::ClearMute => run_hook(&config.on_unmute_command),
        MonitorAction::NoChange => {}
    }
}

/// Spawn a shell command in a background thread. Empty commands are ignored.
fn run_hook(command: &str) {
    let command = command.trim();
    if command.is_empty() {
        return;
    }
    if HOOK_RUNNING
        .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
        .is_err()
    {
        log::warn!("hook skipped (previous hook still running): {command}");
        return;
    }
    let command = command.to_string();
    std::thread::spawn(move || {
        let result = run_hook_with_timeout(&command, HOOK_TIMEOUT);
        HOOK_RUNNING.store(false, Ordering::SeqCst);
        match result {
            Ok(s) if !s.success() => {
                log::warn!("hook command exited with {s}: {command}");
            }
            Err(e) => {
                log::warn!("hook command failed: {e}: {command}");
            }
            _ => {}
        }
    });
}

/// Run a shell command with a timeout. Kills the process if it exceeds the deadline.
fn run_hook_with_timeout(command: &str, timeout: Duration) -> io::Result<ExitStatus> {
    let mut child = if cfg!(windows) {
        std::process::Command::new("cmd")
            .args(["/C", command])
            .spawn()?
    } else {
        std::process::Command::new("sh")
            .args(["-c", command])
            .spawn()?
    };

    let max_polls = (timeout.as_millis() / POLL_INTERVAL.as_millis()).max(1) as u64;
    for _ in 0..max_polls {
        match child.try_wait()? {
            Some(status) => return Ok(status),
            None => std::thread::sleep(POLL_INTERVAL),
        }
    }

    // Timeout — kill and reap
    log::warn!("hook command timed out after {timeout:?}, killing: {command}");
    let _ = child.kill();
    child.wait() // reap zombie
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn run_hook_empty_command_is_noop() {
        // Should not spawn any process or panic
        run_hook("");
        run_hook("   ");
    }

    #[test]
    fn run_action_hook_no_change_is_noop() {
        let config = Config::default();
        // NoChange should not run anything
        run_action_hook(MonitorAction::NoChange, &config);
    }

    #[test]
    fn run_action_hook_with_empty_commands_is_noop() {
        let config = Config::default();
        // Default config has empty commands — should be fine
        run_action_hook(MonitorAction::ApplyMute, &config);
        run_action_hook(MonitorAction::ClearMute, &config);
    }

    #[test]
    fn run_hook_with_timeout_completes() {
        // A fast command should succeed within the timeout
        let cmd = if cfg!(windows) { "echo ok" } else { "true" };
        let result = run_hook_with_timeout(cmd, Duration::from_secs(5));
        assert!(result.is_ok());
        assert!(result.unwrap().success());
    }

    #[test]
    fn run_hook_with_timeout_kills_on_timeout() {
        // A long-running command should be killed after a short timeout
        let cmd = if cfg!(windows) {
            "ping -n 60 127.0.0.1"
        } else {
            "sleep 60"
        };
        let result = run_hook_with_timeout(cmd, Duration::from_secs(1));
        // The process was killed — the exit status should indicate abnormal termination
        assert!(result.is_ok(), "should still return Ok after kill+wait");
        let status = result.unwrap();
        assert!(
            !status.success(),
            "killed process should not report success"
        );
    }

    #[test]
    fn run_hook_guard_skips_concurrent() {
        // Set the guard to simulate a running hook
        HOOK_RUNNING.store(true, Ordering::SeqCst);
        // run_hook should skip immediately (no spawn)
        run_hook("echo should-not-run");
        // Clean up
        HOOK_RUNNING.store(false, Ordering::SeqCst);
    }
}
