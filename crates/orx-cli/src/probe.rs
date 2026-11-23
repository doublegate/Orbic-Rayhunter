//! Network reachability probes for the device web interfaces.

use std::io::{Read, Write};
use std::net::{SocketAddr, TcpStream};
use std::time::{Duration, Instant};

/// Default address of the Orbic on its RNDIS / Wi-Fi network.
pub const DEFAULT_DEVICE_IP: &str = "192.168.1.1";
/// Port the Rayhunter daemon serves its web UI on.
pub const RAYHUNTER_PORT: u16 = 8080;
/// Port the stock Orbic web UI serves on.
pub const STOCK_UI_PORT: u16 = 80;

/// Whether a TCP connection to `addr` succeeds within `timeout`.
pub fn tcp_open(addr: SocketAddr, timeout: Duration) -> bool {
    TcpStream::connect_timeout(&addr, timeout).is_ok()
}

/// Parse `ip` and `port` into a socket address.
pub fn socket(ip: &str, port: u16) -> Result<SocketAddr, String> {
    format!("{ip}:{port}")
        .parse()
        .map_err(|e| format!("invalid device address {ip:?}: {e}"))
}

/// Minimal read-only HTTP/1.0 GET, std-only (orx-cli carries no dependencies).
/// Returns the response body on a `2xx` status. The device UIs are plain
/// HTTP/1.0 on a local link, so there is no TLS, redirect or chunked-encoding
/// handling here; a non-2xx status or any I/O error is an `Err`. The whole
/// exchange is bounded by `timeout`, and the body is capped so a hostile or
/// broken peer cannot stream unboundedly into memory.
pub fn http_get(addr: SocketAddr, path: &str, timeout: Duration) -> Result<String, String> {
    const MAX_BODY: usize = 256 * 1024;
    // A single total deadline bounds the whole exchange. A per-read timeout alone
    // lets a slow-drip peer hold the connection open forever (every read resets
    // the clock), so each read's timeout is the time REMAINING to the deadline.
    let deadline = Instant::now() + timeout;
    let mut stream =
        TcpStream::connect_timeout(&addr, timeout).map_err(|e| format!("connect: {e}"))?;
    stream.set_write_timeout(Some(timeout)).ok();
    let req =
        format!("GET {path} HTTP/1.0\r\nHost: {addr}\r\nConnection: close\r\nAccept: */*\r\n\r\n");
    stream
        .write_all(req.as_bytes())
        .map_err(|e| format!("write: {e}"))?;
    let mut raw = Vec::new();
    let mut buf = [0u8; 8192];
    loop {
        let remaining = deadline.saturating_duration_since(Instant::now());
        if remaining.is_zero() {
            return Err("read: deadline exceeded".to_string());
        }
        stream.set_read_timeout(Some(remaining)).ok();
        match stream.read(&mut buf) {
            Ok(0) => break,
            Ok(n) => {
                raw.extend_from_slice(&buf[..n]);
                if raw.len() > MAX_BODY {
                    raw.truncate(MAX_BODY);
                    break;
                }
            }
            Err(e) => return Err(format!("read: {e}")),
        }
    }
    let text = String::from_utf8_lossy(&raw);
    let (head, body) = text
        .split_once("\r\n\r\n")
        .ok_or_else(|| "malformed HTTP response (no header/body split)".to_string())?;
    let status = head
        .lines()
        .next()
        .and_then(|l| l.split_whitespace().nth(1))
        .and_then(|c| c.parse::<u16>().ok())
        .ok_or_else(|| "malformed HTTP status line".to_string())?;
    if !(200..300).contains(&status) {
        return Err(format!("HTTP {status}"));
    }
    Ok(body.to_string())
}

/// Extract a JSON string value for a top-level `"key":"value"` pair without a
/// JSON parser (orx-cli has no dependencies). Returns the first match's value
/// with standard escapes decoded; `None` if the key is absent or not a string.
/// This is deliberately small: it reads one known field (a version) out of a
/// trusted local response, not a general JSON decoder.
pub fn json_str_field(body: &str, key: &str) -> Option<String> {
    let needle = format!("\"{key}\"");
    // Find an occurrence of `"key"` that is actually a KEY -- i.e. followed by
    // optional whitespace and `:` -- not a string value that merely equals the
    // key text. Try each occurrence until one qualifies.
    for (pos, _) in body.match_indices(&needle) {
        let after = body[pos + needle.len()..].trim_start();
        let Some(after) = after.strip_prefix(':') else {
            continue;
        };
        let after = after.trim_start();
        let Some(after) = after.strip_prefix('"') else {
            continue; // value is not a string (number/null/object)
        };
        let mut chars = after.chars();
        let mut out = String::new();
        while let Some(c) = chars.next() {
            match c {
                '"' => return Some(out),
                '\\' => match chars.next()? {
                    'n' => out.push('\n'),
                    't' => out.push('\t'),
                    'r' => out.push('\r'),
                    // A \uXXXX escape is not decoded; refuse rather than emit a
                    // bogus literal `uXXXX`.
                    'u' => return None,
                    other => out.push(other), // \" \\ \/ \b \f and the rest
                },
                other => out.push(other),
            }
        }
        return None; // unterminated string
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::TcpListener;

    #[test]
    fn socket_rejects_garbage() {
        assert!(socket("not-an-ip", 80).is_err());
        assert_eq!(socket("192.168.1.1", 8080).unwrap().port(), 8080);
    }

    #[test]
    fn tcp_open_detects_listener_and_closed_port() {
        let l = TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = l.local_addr().unwrap();
        assert!(tcp_open(addr, Duration::from_millis(500)));
        drop(l);
        assert!(!tcp_open(addr, Duration::from_millis(500)));
    }

    /// Serve one canned HTTP response on a loopback port, then return its addr.
    fn serve_once(response: &'static str) -> SocketAddr {
        let l = TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = l.local_addr().unwrap();
        std::thread::spawn(move || {
            if let Ok((mut s, _)) = l.accept() {
                let mut discard = [0u8; 1024];
                let _ = s.read(&mut discard);
                let _ = s.write_all(response.as_bytes());
            }
        });
        addr
    }

    #[test]
    fn http_get_returns_body_on_200() {
        let addr = serve_once(
            "HTTP/1.0 200 OK\r\nContent-Type: application/json\r\n\r\n{\"rayhunter_version\":\"0.13.0\"}",
        );
        let body = http_get(addr, "/api/system-stats", Duration::from_secs(2)).unwrap();
        assert_eq!(body, "{\"rayhunter_version\":\"0.13.0\"}");
        assert_eq!(
            json_str_field(&body, "rayhunter_version").as_deref(),
            Some("0.13.0")
        );
    }

    #[test]
    fn http_get_errors_on_non_2xx() {
        let addr = serve_once("HTTP/1.0 404 Not Found\r\n\r\nnope");
        assert!(
            http_get(addr, "/missing", Duration::from_secs(2))
                .unwrap_err()
                .contains("404")
        );
    }

    #[test]
    fn json_str_field_matches_key_not_value_and_rejects_unicode_escape() {
        // A value equal to the key text must not be mistaken for the key.
        assert_eq!(
            json_str_field(
                "{\"x\":\"rayhunter_version\",\"rayhunter_version\":\"0.13.0\"}",
                "rayhunter_version"
            )
            .as_deref(),
            Some("0.13.0")
        );
        // \uXXXX is refused rather than decoded to a bogus literal.
        assert_eq!(json_str_field("{\"a\":\"\\u0041\"}", "a"), None);
    }

    #[test]
    fn json_str_field_handles_absent_escapes_and_nonstring() {
        assert_eq!(json_str_field("{\"a\":\"b\"}", "a").as_deref(), Some("b"));
        assert_eq!(
            json_str_field("{\"a\": \"x\\ny\"}", "a").as_deref(),
            Some("x\ny")
        );
        assert_eq!(
            json_str_field("{\"a\":\"quo\\\"te\"}", "a").as_deref(),
            Some("quo\"te")
        );
        assert_eq!(json_str_field("{\"a\":\"b\"}", "missing"), None);
        assert_eq!(json_str_field("{\"a\":123}", "a"), None); // not a string
        assert_eq!(json_str_field("{\"a\":\"unterminated", "a"), None);
    }
}
