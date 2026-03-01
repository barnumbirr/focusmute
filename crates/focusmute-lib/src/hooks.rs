//! Mute state change hooks — run user-defined commands on mute/unmute events.

use std::io;
use std::process::ExitStatus;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use crate::config::Config;
use crate::monitor::MonitorAction;

/// Guard preventing concurrent hook execution (shared across mute/unmute hooks).
static HOOK_RUNNING: AtomicBool = AtomicBool::new(false);

/// RAII guard that resets `HOOK_RUNNING` on drop. Ensures the flag is cleared
/// even if the hook thread panics.
struct HookGuard;

impl Drop for HookGuard {
    fn drop(&mut self) {
        HOOK_RUNNING.store(false, Ordering::SeqCst);
    }
}

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
        let _guard = HookGuard;
        let result = run_hook_with_timeout(&command, HOOK_TIMEOUT);
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
    use std::sync::Mutex;

    /// Serializes tests that interact with the global HOOK_RUNNING flag.
    static HOOK_TEST_LOCK: Mutex<()> = Mutex::new(());

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
        let _lock = HOOK_TEST_LOCK.lock().unwrap();
        // Set the guard to simulate a running hook
        HOOK_RUNNING.store(true, Ordering::SeqCst);
        // run_hook should skip immediately (no spawn)
        run_hook("echo should-not-run");
        // Clean up
        HOOK_RUNNING.store(false, Ordering::SeqCst);
    }

    /// Wait for HOOK_RUNNING to become false (up to 5 seconds).
    fn wait_for_hook_idle() {
        let deadline = std::time::Instant::now() + Duration::from_secs(5);
        while HOOK_RUNNING.load(Ordering::SeqCst) {
            if std::time::Instant::now() > deadline {
                panic!("timed out waiting for HOOK_RUNNING to become false");
            }
            std::thread::sleep(Duration::from_millis(50));
        }
    }

    /// Wait for a file to appear (up to 5 seconds).
    fn wait_for_file(path: &std::path::Path) {
        let deadline = std::time::Instant::now() + Duration::from_secs(5);
        while !path.exists() {
            if std::time::Instant::now() > deadline {
                panic!("timed out waiting for file: {}", path.display());
            }
            std::thread::sleep(Duration::from_millis(50));
        }
    }

    #[test]
    fn run_action_hook_dispatches_commands() {
        let _lock = HOOK_TEST_LOCK.lock().unwrap();
        wait_for_hook_idle();

        let dir = tempfile::tempdir().unwrap();

        // Test mute command dispatch
        let mute_marker = dir.path().join("muted.txt");
        let mute_cmd = format!("echo muted > {}", mute_marker.display());
        let config = Config {
            on_mute_command: mute_cmd,
            ..Config::default()
        };
        run_action_hook(MonitorAction::ApplyMute, &config);
        wait_for_file(&mute_marker);
        wait_for_hook_idle();

        let content = std::fs::read_to_string(&mute_marker).unwrap();
        assert!(
            content.trim() == "muted",
            "mute marker should contain 'muted', got: {content:?}"
        );

        // Test unmute command dispatch (runs after mute hook completes)
        let unmute_marker = dir.path().join("unmuted.txt");
        let unmute_cmd = format!("echo unmuted > {}", unmute_marker.display());
        let config = Config {
            on_unmute_command: unmute_cmd,
            ..Config::default()
        };
        run_action_hook(MonitorAction::ClearMute, &config);
        wait_for_file(&unmute_marker);
        wait_for_hook_idle();

        let content = std::fs::read_to_string(&unmute_marker).unwrap();
        assert!(
            content.trim() == "unmuted",
            "unmute marker should contain 'unmuted', got: {content:?}"
        );
    }

    #[test]
    fn hook_guard_resets_on_panic() {
        let _lock = HOOK_TEST_LOCK.lock().unwrap();
        wait_for_hook_idle();

        // Spawn a thread that sets HOOK_RUNNING, creates a HookGuard, then panics.
        // The Drop impl should reset the flag even after panic.
        let handle = std::thread::spawn(|| {
            HOOK_RUNNING.store(true, Ordering::SeqCst);
            let _guard = HookGuard;
            panic!("intentional panic to test HookGuard drop");
        });
        // Join the thread — it will have panicked
        let _ = handle.join();
        // The guard's Drop should have reset HOOK_RUNNING to false
        assert!(
            !HOOK_RUNNING.load(Ordering::SeqCst),
            "HOOK_RUNNING should be false after HookGuard drop on panic"
        );
    }
}
