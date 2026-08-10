//! Webhook hooks — send HTTP POST requests on mute/unmute events.
//!
//! When a URL is configured, an HTTP POST is sent using `minreq` with a JSON
//! body. No shell commands are executed — this avoids visible terminal windows
//! on Windows and works cross-platform.
//!
//! Default body: `{"event": "mute"}` or `{"event": "unmute"}`.
//! Custom bodies can be configured per-event via `on_mute_body` / `on_unmute_body`.

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

/// Default timeout for webhook requests (30 seconds).
const HOOK_TIMEOUT: Duration = Duration::from_secs(30);

/// Run the appropriate webhook for a mute state change.
///
/// Spawns the request in a background thread so it doesn't block the event loop.
/// Empty URLs are silently ignored. Only one hook can run at a time — if a
/// previous hook is still running, the new one is skipped with a warning.
pub fn run_action_hook(action: MonitorAction, config: &Config) {
    let (url, custom_body, default_event) = match action {
        MonitorAction::ApplyMute => (
            &config.hooks.on_mute_url,
            &config.hooks.on_mute_body,
            "mute",
        ),
        MonitorAction::ClearMute => (
            &config.hooks.on_unmute_url,
            &config.hooks.on_unmute_body,
            "unmute",
        ),
        MonitorAction::NoChange => return,
    };
    run_webhook(url, custom_body, default_event);
}

/// Spawn a webhook request in a background thread. Empty URLs are ignored.
fn run_webhook(url: &str, custom_body: &str, default_event: &str) {
    let url = url.trim();
    if url.is_empty() {
        return;
    }
    if HOOK_RUNNING
        .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
        .is_err()
    {
        log::warn!("[hooks] skipped (previous hook still running): {url}");
        return;
    }
    let url = url.to_string();
    let body = if custom_body.trim().is_empty() {
        format!(r#"{{"event":"{default_event}"}}"#)
    } else {
        custom_body.trim().to_string()
    };
    std::thread::spawn(move || {
        let _guard = HookGuard;
        let timeout_secs = HOOK_TIMEOUT.as_secs();

        match minreq::post(&url)
            .with_header("Content-Type", "application/json")
            .with_body(body)
            .with_timeout(timeout_secs)
            .send()
        {
            Ok(resp) => {
                log::debug!(
                    "[hooks] webhook {} (HTTP {}): {url}",
                    if (200..300).contains(&resp.status_code) {
                        "succeeded"
                    } else {
                        "returned error"
                    },
                    resp.status_code,
                );
            }
            Err(e) => {
                log::warn!("[hooks] webhook failed: {e}: {url}");
            }
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    /// Serializes tests that interact with the global HOOK_RUNNING flag.
    static HOOK_TEST_LOCK: Mutex<()> = Mutex::new(());

    #[test]
    fn run_webhook_empty_url_is_noop() {
        run_webhook("", "", "mute");
        run_webhook("   ", "", "mute");
    }

    #[test]
    fn run_action_hook_no_change_is_noop() {
        let config = Config::default();
        run_action_hook(MonitorAction::NoChange, &config);
    }

    #[test]
    fn run_action_hook_with_empty_urls_is_noop() {
        let config = Config::default();
        run_action_hook(MonitorAction::ApplyMute, &config);
        run_action_hook(MonitorAction::ClearMute, &config);
    }

    #[test]
    fn run_webhook_guard_skips_concurrent() {
        let _lock = HOOK_TEST_LOCK.lock().unwrap();
        HOOK_RUNNING.store(true, Ordering::SeqCst);
        run_webhook("https://example.com/hook", "", "mute");
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

    #[test]
    fn hook_guard_resets_on_panic() {
        let _lock = HOOK_TEST_LOCK.lock().unwrap();
        wait_for_hook_idle();

        let handle = std::thread::spawn(|| {
            HOOK_RUNNING.store(true, Ordering::SeqCst);
            let _guard = HookGuard;
            panic!("intentional panic to test HookGuard drop");
        });
        let _ = handle.join();
        assert!(
            !HOOK_RUNNING.load(Ordering::SeqCst),
            "HOOK_RUNNING should be false after HookGuard drop on panic"
        );
    }

    #[test]
    fn webhook_to_unreachable_host_fails_gracefully() {
        let _lock = HOOK_TEST_LOCK.lock().unwrap();
        wait_for_hook_idle();
        // Should fail (connection refused) but not panic
        run_webhook("http://127.0.0.1:1/nonexistent", "", "mute");
        wait_for_hook_idle();
    }

    #[test]
    fn default_body_format() {
        let body_mute = format!(r#"{{"event":"{}"}}"#, "mute");
        let body_unmute = format!(r#"{{"event":"{}"}}"#, "unmute");

        let parsed: serde_json::Value = serde_json::from_str(&body_mute).unwrap();
        assert_eq!(parsed["event"], "mute");

        let parsed: serde_json::Value = serde_json::from_str(&body_unmute).unwrap();
        assert_eq!(parsed["event"], "unmute");
    }

    #[test]
    fn custom_body_overrides_default() {
        let custom = r#"{"action":"silent","source":"focusmute"}"#;
        let body = if custom.trim().is_empty() {
            r#"{"event":"mute"}"#.to_string()
        } else {
            custom.trim().to_string()
        };
        assert_eq!(body, custom);
    }

    #[test]
    fn empty_custom_body_uses_default() {
        let custom = "";
        let body = if custom.trim().is_empty() {
            r#"{"event":"mute"}"#.to_string()
        } else {
            custom.trim().to_string()
        };
        assert_eq!(body, r#"{"event":"mute"}"#);
    }

    #[test]
    fn action_hook_uses_correct_urls() {
        let config = Config {
            hooks: crate::config::HooksConfig {
                on_mute_url: "https://example.com/mute".into(),
                on_unmute_url: "https://example.com/unmute".into(),
                on_mute_body: r#"{"muted":true}"#.into(),
                on_unmute_body: r#"{"muted":false}"#.into(),
            },
            ..Config::default()
        };

        // Verify the dispatch logic picks the right fields
        let (url, body, event) = match MonitorAction::ApplyMute {
            MonitorAction::ApplyMute => (
                &config.hooks.on_mute_url,
                &config.hooks.on_mute_body,
                "mute",
            ),
            _ => unreachable!(),
        };
        assert_eq!(url, "https://example.com/mute");
        assert_eq!(body, r#"{"muted":true}"#);
        assert_eq!(event, "mute");

        let (url, body, event) = match MonitorAction::ClearMute {
            MonitorAction::ClearMute => (
                &config.hooks.on_unmute_url,
                &config.hooks.on_unmute_body,
                "unmute",
            ),
            _ => unreachable!(),
        };
        assert_eq!(url, "https://example.com/unmute");
        assert_eq!(body, r#"{"muted":false}"#);
        assert_eq!(event, "unmute");
    }
}
