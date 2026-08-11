//! Localhost HTTP server for browser extension mute sync.
//!
//! Listens on `127.0.0.1:<port>` for simple HTTP POST requests with JSON bodies.
//! No dependencies beyond `std` and `serde_json` (already in the dep tree).
//!
//! Understands three JSON message types:
//! - `{"type": "mute_state", "platform": "...", "muted": bool}` → `Msg::BrowserMute`
//! - `{"type": "ping"}` → replies `{"type": "pong"}`
//! - `{"type": "poll_actions"}` → replies `{"type": "pending_action", "action":
//!   "mute"|"unmute"|null}` — reverse sync: the extension polls for
//!   user-initiated mute changes to mirror into the meeting page
//!
//! Firefox forces TLS on WebSocket connections from extension background scripts,
//! but allows plain `http://127.0.0.1` via `fetch()` (localhost is exempt from
//! mixed content restrictions for HTTP requests).

use std::io::{BufRead, BufReader, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::atomic::Ordering;
use std::sync::{Arc, Mutex, mpsc};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use serde::Deserialize;

use super::state::Msg;
use crate::RUNNING;

/// A pending reverse-sync action older than this is stale: delivering it
/// (e.g. after the extension was closed for a while) could undo a mute
/// change the user has since made in the meeting itself.
const MAX_ACTION_AGE: Duration = Duration::from_secs(3);

/// Single-slot mailbox for the latest user-initiated mute transition.
///
/// Last-wins by design: mute intents supersede each other, so a queue would
/// only replay stale toggles. The slot is consumed by the extension's next
/// `poll_actions` request.
pub struct ReverseSlot(Mutex<Option<(bool, Instant)>>);

impl ReverseSlot {
    pub fn new() -> Self {
        Self(Mutex::new(None))
    }

    /// Record a user-initiated transition to `muted`.
    pub fn set(&self, muted: bool) {
        *self.0.lock().unwrap() = Some((muted, Instant::now()));
    }

    /// Take the pending action if one is present and no older than `max_age`.
    /// Consumes the slot either way.
    pub fn take_fresh(&self, max_age: Duration) -> Option<bool> {
        self.0
            .lock()
            .unwrap()
            .take()
            .filter(|(_, at)| at.elapsed() <= max_age)
            .map(|(muted, _)| muted)
    }
}

/// Maximum size for the request line (method + path + version).
const MAX_REQUEST_LINE: usize = 8192;

/// Maximum total bytes across all headers.
const MAX_HEADER_BYTES: usize = 8192;

/// Maximum POST body size.
const MAX_BODY_SIZE: usize = 4096;

/// JSON messages from the browser extension.
#[derive(Debug, Deserialize)]
#[serde(tag = "type")]
enum BrowserMessage {
    #[serde(rename = "mute_state")]
    MuteState {
        #[serde(default)]
        platform: String,
        muted: bool,
    },
    #[serde(rename = "ping")]
    Ping,
    #[serde(rename = "poll_actions")]
    PollActions,
}

/// Spawn a background thread that listens for HTTP requests from the
/// browser extension and forwards mute state changes to the tray event loop.
pub fn spawn_sync_thread(
    port: u16,
    tx: mpsc::Sender<Msg>,
    reverse: Arc<ReverseSlot>,
) -> JoinHandle<()> {
    std::thread::Builder::new()
        .name("sync-listener".into())
        .spawn(move || run_listener(port, &tx, &reverse))
        .expect("failed to spawn browser sync listener thread")
}

fn run_listener(port: u16, tx: &mpsc::Sender<Msg>, reverse: &ReverseSlot) {
    let addr = format!("127.0.0.1:{port}");
    let listener = match TcpListener::bind(&addr) {
        Ok(l) => {
            log::info!("[sync] listening on {addr}");
            l
        }
        Err(e) => {
            log::error!("[sync] failed to bind {addr}: {e}");
            return;
        }
    };

    // Non-blocking accept so we can check the RUNNING flag.
    listener
        .set_nonblocking(true)
        .expect("failed to set TcpListener to non-blocking");

    while RUNNING.load(Ordering::SeqCst) {
        match listener.accept() {
            Ok((stream, peer)) => {
                log::debug!("[sync] connection from {peer}");
                stream.set_nonblocking(false).ok();
                stream.set_read_timeout(Some(Duration::from_secs(5))).ok();
                handle_request(stream, tx, reverse);
            }
            Err(ref e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                std::thread::sleep(Duration::from_millis(100));
            }
            Err(e) => {
                log::warn!("[sync] accept error: {e}");
                std::thread::sleep(Duration::from_millis(100));
            }
        }
    }
    log::info!("[sync] listener shutting down");
}

// ── HTTP response helpers ──

/// CORS headers shared by all responses.
const CORS_HEADERS: &str = "\
Access-Control-Allow-Origin: *\r\n\
Access-Control-Allow-Methods: POST, OPTIONS\r\n\
Access-Control-Allow-Headers: Content-Type\r\n\
Connection: close\r\n";

fn send_response(stream: &mut TcpStream, status: &str, body: &str) {
    let response = format!(
        "HTTP/1.1 {status}\r\n{CORS_HEADERS}Content-Type: application/json\r\nContent-Length: {}\r\n\r\n{body}",
        body.len()
    );
    stream.write_all(response.as_bytes()).ok();
}

fn send_no_content(stream: &mut TcpStream) {
    let response = format!("HTTP/1.1 204 No Content\r\n{CORS_HEADERS}\r\n");
    stream.write_all(response.as_bytes()).ok();
}

// ── Origin checking ──

/// Check whether a request origin is allowed.
///
/// Allowed: absent (no header), or browser extension origins
/// (`moz-extension://...`, `chrome-extension://...`).
/// Blocked: any other origin (e.g. `https://evil.com`).
fn is_origin_allowed(origin: Option<&str>) -> bool {
    match origin {
        None => true,
        Some(o) => o.starts_with("moz-extension://") || o.starts_with("chrome-extension://"),
    }
}

// ── Request handling ──

fn handle_request(mut stream: TcpStream, tx: &mpsc::Sender<Msg>, reverse: &ReverseSlot) {
    let mut reader = BufReader::new(&stream);

    // Read request line
    let mut request_line = String::new();
    if reader.read_line(&mut request_line).is_err() {
        return;
    }
    if request_line.len() > MAX_REQUEST_LINE {
        send_response(&mut stream, "414 URI Too Long", "Request line too long");
        return;
    }
    let request_line = request_line.trim_end();
    log::debug!("[sync] {request_line}");

    // Read headers — extract Content-Length and Origin (case-insensitive).
    let mut content_length: usize = 0;
    let mut origin: Option<String> = None;
    let mut total_header_bytes: usize = 0;
    loop {
        let mut line = String::new();
        if reader.read_line(&mut line).is_err() {
            return;
        }
        total_header_bytes += line.len();
        if total_header_bytes > MAX_HEADER_BYTES {
            send_response(
                &mut stream,
                "431 Request Header Fields Too Large",
                "Headers too large",
            );
            return;
        }
        let trimmed = line.trim();
        if trimmed.is_empty() {
            break; // End of headers
        }
        let lower = trimmed.to_ascii_lowercase();
        if let Some(val) = lower.strip_prefix("content-length:") {
            content_length = val.trim().parse().unwrap_or(0);
        } else if let Some(val) = lower.strip_prefix("origin:") {
            origin = Some(val.trim().to_string());
        }
    }

    // Check origin — block requests from web pages.
    if !is_origin_allowed(origin.as_deref()) {
        log::debug!(
            "[sync] rejected request from origin: {}",
            origin.as_deref().unwrap_or("(none)")
        );
        send_response(&mut stream, "403 Forbidden", "Origin not allowed");
        return;
    }

    // Handle CORS preflight
    if request_line.starts_with("OPTIONS ") {
        send_no_content(&mut stream);
        return;
    }

    // Only accept POST
    if !request_line.starts_with("POST ") {
        send_response(&mut stream, "405 Method Not Allowed", "Method not allowed");
        return;
    }

    // Read body
    if content_length == 0 || content_length > MAX_BODY_SIZE {
        send_response(&mut stream, "400 Bad Request", "Bad request");
        return;
    }

    let mut body_buf = vec![0u8; content_length];
    if std::io::Read::read_exact(&mut reader, &mut body_buf).is_err() {
        return;
    }
    let body = String::from_utf8_lossy(&body_buf);
    log::debug!("[sync] body: {body}");

    // Parse and handle
    let response_body = match serde_json::from_str::<BrowserMessage>(&body) {
        Ok(BrowserMessage::MuteState { platform, muted }) => {
            log::info!(
                "[sync] browser mute: {} (platform={})",
                if muted { "muted" } else { "unmuted" },
                platform
            );
            tx.send(Msg::BrowserMute(muted)).ok();
            "{\"type\":\"ack\"}"
        }
        Ok(BrowserMessage::Ping) => "{\"type\":\"pong\"}",
        Ok(BrowserMessage::PollActions) => match reverse.take_fresh(MAX_ACTION_AGE) {
            Some(true) => "{\"type\":\"pending_action\",\"action\":\"mute\"}",
            Some(false) => "{\"type\":\"pending_action\",\"action\":\"unmute\"}",
            None => "{\"type\":\"pending_action\",\"action\":null}",
        },
        Err(e) => {
            log::debug!("[sync] unknown message: {e}");
            "{\"type\":\"error\"}"
        }
    };

    send_response(&mut stream, "200 OK", response_body);
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Read;
    use std::net::TcpStream;

    // ── JSON parsing tests ──

    #[test]
    fn parse_mute_state_message() {
        let json = r#"{"type": "mute_state", "platform": "meet", "muted": true}"#;
        let msg: BrowserMessage = serde_json::from_str(json).unwrap();
        assert!(matches!(
            msg,
            BrowserMessage::MuteState {
                muted: true,
                ref platform,
                ..
            } if platform == "meet"
        ));
    }

    #[test]
    fn parse_mute_state_unmuted() {
        let json = r#"{"type": "mute_state", "platform": "teams", "muted": false}"#;
        let msg: BrowserMessage = serde_json::from_str(json).unwrap();
        assert!(matches!(
            msg,
            BrowserMessage::MuteState { muted: false, .. }
        ));
    }

    #[test]
    fn parse_ping_message() {
        let json = r#"{"type": "ping"}"#;
        let msg: BrowserMessage = serde_json::from_str(json).unwrap();
        assert!(matches!(msg, BrowserMessage::Ping));
    }

    #[test]
    fn parse_unknown_type_fails() {
        let json = r#"{"type": "unknown"}"#;
        let result = serde_json::from_str::<BrowserMessage>(json);
        assert!(result.is_err());
    }

    #[test]
    fn parse_malformed_json_fails() {
        let result = serde_json::from_str::<BrowserMessage>("not json at all");
        assert!(result.is_err());
    }

    #[test]
    fn parse_mute_state_missing_platform_defaults() {
        let json = r#"{"type": "mute_state", "muted": true}"#;
        let msg: BrowserMessage = serde_json::from_str(json).unwrap();
        assert!(matches!(
            msg,
            BrowserMessage::MuteState {
                muted: true,
                ref platform,
                ..
            } if platform.is_empty()
        ));
    }

    // ── Origin checking tests ──

    #[test]
    fn origin_absent_is_allowed() {
        assert!(is_origin_allowed(None));
    }

    #[test]
    fn origin_firefox_extension_is_allowed() {
        assert!(is_origin_allowed(Some(
            "moz-extension://ab12cd34-ef56-7890-abcd-ef1234567890"
        )));
    }

    #[test]
    fn origin_chrome_extension_is_allowed() {
        assert!(is_origin_allowed(Some(
            "chrome-extension://abcdefghijklmnop"
        )));
    }

    #[test]
    fn origin_web_page_is_blocked() {
        assert!(!is_origin_allowed(Some("https://evil.com")));
        assert!(!is_origin_allowed(Some("http://localhost:3000")));
        assert!(!is_origin_allowed(Some("null")));
    }

    // ── HTTP integration tests ──

    /// Send a raw HTTP request to the given address and return the full response.
    fn send_http(
        addr: std::net::SocketAddr,
        method: &str,
        headers: &[(&str, &str)],
        body: Option<&str>,
    ) -> String {
        let mut stream = TcpStream::connect(addr).unwrap();
        stream.set_read_timeout(Some(Duration::from_secs(5))).ok();

        let body_bytes = body.unwrap_or("");
        let mut request = format!("{method} / HTTP/1.1\r\nHost: 127.0.0.1\r\n");
        for (key, val) in headers {
            request.push_str(&format!("{key}: {val}\r\n"));
        }
        if body.is_some() {
            request.push_str(&format!("Content-Length: {}\r\n", body_bytes.len()));
        }
        request.push_str("\r\n");
        request.push_str(body_bytes);

        stream.write_all(request.as_bytes()).unwrap();
        stream.shutdown(std::net::Shutdown::Write).ok();

        let mut response = String::new();
        stream.read_to_string(&mut response).ok();
        response
    }

    /// Spawn a listener on an OS-assigned port, accept one connection, handle it,
    /// and return the port. The `tx` channel receives any `Msg::BrowserMute` sent.
    fn serve_one_request(tx: mpsc::Sender<Msg>) -> std::net::SocketAddr {
        serve_one_request_with_slot(tx, Arc::new(ReverseSlot::new()))
    }

    /// Like [`serve_one_request`], with a caller-provided reverse slot.
    fn serve_one_request_with_slot(
        tx: mpsc::Sender<Msg>,
        slot: Arc<ReverseSlot>,
    ) -> std::net::SocketAddr {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        std::thread::spawn(move || {
            let (stream, _) = listener.accept().unwrap();
            stream.set_nonblocking(false).ok();
            stream.set_read_timeout(Some(Duration::from_secs(5))).ok();
            handle_request(stream, &tx, &slot);
        });
        addr
    }

    #[test]
    fn http_post_mute_state_returns_ack() {
        let (tx, rx) = mpsc::channel();
        let addr = serve_one_request(tx);
        let body = r#"{"type":"mute_state","platform":"meet","muted":true}"#;
        let response = send_http(addr, "POST", &[], Some(body));
        assert!(response.contains("200 OK"), "got: {response}");
        assert!(response.contains(r#"{"type":"ack"}"#), "got: {response}");
        // Verify the message was sent to the channel
        let msg = rx.recv_timeout(Duration::from_secs(1)).unwrap();
        assert!(matches!(msg, Msg::BrowserMute(true)));
    }

    #[test]
    fn http_post_ping_returns_pong() {
        let (tx, _rx) = mpsc::channel();
        let addr = serve_one_request(tx);
        let body = r#"{"type":"ping"}"#;
        let response = send_http(addr, "POST", &[], Some(body));
        assert!(response.contains("200 OK"), "got: {response}");
        assert!(response.contains(r#"{"type":"pong"}"#), "got: {response}");
    }

    #[test]
    fn http_options_returns_204() {
        let (tx, _rx) = mpsc::channel();
        let addr = serve_one_request(tx);
        let response = send_http(addr, "OPTIONS", &[], None);
        assert!(response.contains("204 No Content"), "got: {response}");
        assert!(
            response.contains("Access-Control-Allow-Methods"),
            "got: {response}"
        );
    }

    #[test]
    fn http_get_returns_405() {
        let (tx, _rx) = mpsc::channel();
        let addr = serve_one_request(tx);
        let response = send_http(addr, "GET", &[], None);
        assert!(
            response.contains("405 Method Not Allowed"),
            "got: {response}"
        );
    }

    #[test]
    fn http_post_empty_body_returns_400() {
        let (tx, _rx) = mpsc::channel();
        let addr = serve_one_request(tx);
        // Send POST with no Content-Length (defaults to 0)
        let response = send_http(addr, "POST", &[], None);
        assert!(response.contains("400 Bad Request"), "got: {response}");
    }

    #[test]
    fn http_post_oversized_body_returns_400() {
        let (tx, _rx) = mpsc::channel();
        let addr = serve_one_request(tx);
        let big_body = "x".repeat(MAX_BODY_SIZE + 1);
        let response = send_http(addr, "POST", &[], Some(&big_body));
        assert!(response.contains("400 Bad Request"), "got: {response}");
    }

    #[test]
    fn http_post_unknown_json_returns_error() {
        let (tx, _rx) = mpsc::channel();
        let addr = serve_one_request(tx);
        let body = r#"{"type":"unknown_thing"}"#;
        let response = send_http(addr, "POST", &[], Some(body));
        assert!(response.contains("200 OK"), "got: {response}");
        assert!(response.contains(r#"{"type":"error"}"#), "got: {response}");
    }

    #[test]
    fn http_post_with_web_origin_returns_403() {
        let (tx, _rx) = mpsc::channel();
        let addr = serve_one_request(tx);
        let body = r#"{"type":"ping"}"#;
        let response = send_http(addr, "POST", &[("Origin", "https://evil.com")], Some(body));
        assert!(response.contains("403 Forbidden"), "got: {response}");
    }

    #[test]
    fn http_post_with_extension_origin_allowed() {
        let (tx, _rx) = mpsc::channel();
        let addr = serve_one_request(tx);
        let body = r#"{"type":"ping"}"#;
        let response = send_http(
            addr,
            "POST",
            &[("Origin", "moz-extension://abc-123")],
            Some(body),
        );
        assert!(response.contains("200 OK"), "got: {response}");
    }

    #[test]
    fn http_post_without_origin_allowed() {
        let (tx, _rx) = mpsc::channel();
        let addr = serve_one_request(tx);
        let body = r#"{"type":"ping"}"#;
        let response = send_http(addr, "POST", &[], Some(body));
        assert!(response.contains("200 OK"), "got: {response}");
    }

    // ── Reverse sync (poll_actions) tests ──

    #[test]
    fn reverse_slot_is_last_wins_and_consumed_on_take() {
        let slot = ReverseSlot::new();
        assert_eq!(slot.take_fresh(MAX_ACTION_AGE), None);
        slot.set(true);
        slot.set(false); // supersedes
        assert_eq!(slot.take_fresh(MAX_ACTION_AGE), Some(false));
        assert_eq!(slot.take_fresh(MAX_ACTION_AGE), None); // consumed
    }

    #[test]
    fn reverse_slot_discards_stale_actions() {
        let slot = ReverseSlot::new();
        slot.set(true);
        // A zero max-age makes any recorded action stale.
        assert_eq!(slot.take_fresh(Duration::ZERO), None);
        // Stale take still consumes the slot.
        assert_eq!(slot.take_fresh(MAX_ACTION_AGE), None);
    }

    #[test]
    fn http_poll_actions_empty_returns_null() {
        let (tx, _rx) = mpsc::channel();
        let addr = serve_one_request(tx);
        let response = send_http(addr, "POST", &[], Some(r#"{"type":"poll_actions"}"#));
        assert!(response.contains("200 OK"), "got: {response}");
        assert!(response.contains(r#""action":null"#), "got: {response}");
    }

    #[test]
    fn http_poll_actions_delivers_pending_mute() {
        let (tx, _rx) = mpsc::channel();
        let slot = Arc::new(ReverseSlot::new());
        slot.set(true);
        let addr = serve_one_request_with_slot(tx, Arc::clone(&slot));
        let response = send_http(addr, "POST", &[], Some(r#"{"type":"poll_actions"}"#));
        assert!(response.contains(r#""action":"mute""#), "got: {response}");
        // Delivered exactly once.
        assert_eq!(slot.take_fresh(MAX_ACTION_AGE), None);
    }

    #[test]
    fn http_poll_actions_delivers_pending_unmute() {
        let (tx, _rx) = mpsc::channel();
        let slot = Arc::new(ReverseSlot::new());
        slot.set(false);
        let addr = serve_one_request_with_slot(tx, Arc::clone(&slot));
        let response = send_http(addr, "POST", &[], Some(r#"{"type":"poll_actions"}"#));
        assert!(response.contains(r#""action":"unmute""#), "got: {response}");
    }

    #[test]
    fn http_response_includes_connection_close() {
        let (tx, _rx) = mpsc::channel();
        let addr = serve_one_request(tx);
        let body = r#"{"type":"ping"}"#;
        let response = send_http(addr, "POST", &[], Some(body));
        assert!(response.contains("Connection: close"), "got: {response}");
    }
}
