//! `orx alert`: surface and exercise the Rayhunter daemon's BUILT-IN alerting
//! over the `:8080` API. The daemon already sends a `Warning` notification on
//! analyzer detections (and `LowBattery`) to its configured `ntfy_url`, with
//! per-type debounce and exponential-backoff retry -- so "alerting" is a matter
//! of pointing `ntfy_url` at a sink, not re-implementing it on the host.
//!
//! `status` (read-only) shows the current `ntfy_url` and enabled notification
//! types. `test` asks the daemon to POST a test notification to that URL.
//!
//! Setting `ntfy_url` from the host (`orx alert set`) is a follow-up (T-OR-046):
//! the config write restarts the daemon and the GET/POST `/api/config`
//! round-trip redacts `wifi_password`, so it must be done as a careful
//! read-modify-write validated on-device. For now, set `ntfy_url` in the
//! Rayhunter web UI, or ship it in `device/config/config.orbic.toml` at install.

use crate::json::Json;
use crate::pull::{self, PullError};
use std::net::SocketAddr;
use std::time::Duration;

const MAX_CONFIG: usize = 256 * 1024;

pub struct AlertStatus {
    /// The configured ntfy/webhook URL, or `None` if unset (no alerts delivered).
    pub ntfy_url: Option<String>,
    /// Notification types the daemon will deliver (e.g. `Warning`, `LowBattery`).
    pub enabled: Vec<String>,
}

/// Read the alerting configuration from the device.
pub fn status(ip: &str, timeout: Duration) -> Result<AlertStatus, String> {
    let addr = crate::probe::socket(ip, crate::probe::RAYHUNTER_PORT)?;
    status_at(addr, timeout)
}

/// As [`status`] but against an explicit address (tests use a loopback server).
pub fn status_at(addr: SocketAddr, timeout: Duration) -> Result<AlertStatus, String> {
    let body = pull::http_get_string(addr, "/api/config", MAX_CONFIG, timeout)
        .map_err(|e| format!("fetch config: {e}"))?;
    let v = crate::json::parse(&body).map_err(|e| format!("config JSON: {e}"))?;
    if !matches!(v, Json::Obj(_)) {
        return Err("unexpected config response (not a JSON object)".to_string());
    }
    let ntfy_url = v
        .get("ntfy_url")
        .and_then(Json::as_str)
        .filter(|s| !s.is_empty())
        .map(str::to_string);
    let enabled = v
        .get("enabled_notifications")
        .and_then(Json::as_array)
        .map(|a| {
            a.iter()
                .filter_map(|x| x.as_str().map(str::to_string))
                .collect()
        })
        .unwrap_or_default();
    Ok(AlertStatus { ntfy_url, enabled })
}

/// Ask the daemon to send a test notification to its configured `ntfy_url`.
/// This is outward-facing: it causes the device to POST to that external URL.
pub fn test(ip: &str, timeout: Duration) -> Result<(), String> {
    let addr = crate::probe::socket(ip, crate::probe::RAYHUNTER_PORT)?;
    test_at(addr, timeout)
}

/// As [`test`] but against an explicit address.
pub fn test_at(addr: SocketAddr, timeout: Duration) -> Result<(), String> {
    match pull::http_post(addr, "/api/test-notification", timeout) {
        Ok(_) => Ok(()),
        Err(PullError::Http(400)) => Err(
            "no ntfy_url is configured on the device; set one in the Rayhunter web UI (or in \
             device/config/config.orbic.toml at install) first"
                .to_string(),
        ),
        Err(e) => Err(format!("test notification failed: {e}")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{Read, Write};
    use std::net::TcpListener;
    use std::sync::{Arc, Mutex};

    /// Loopback daemon: serves a canned /api/config and records POST paths,
    /// answering /api/test-notification with `test_status`.
    fn serve(config: &'static str, test_status: u16) -> (SocketAddr, Arc<Mutex<Vec<String>>>) {
        let l = TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = l.local_addr().unwrap();
        let posts = Arc::new(Mutex::new(Vec::new()));
        let p2 = Arc::clone(&posts);
        std::thread::spawn(move || {
            while let Ok((mut s, _)) = l.accept() {
                let mut req = [0u8; 2048];
                let n = s.read(&mut req).unwrap_or(0);
                let line = String::from_utf8_lossy(&req[..n]);
                let mut parts = line.split_whitespace();
                let method = parts.next().unwrap_or("");
                let path = parts.next().unwrap_or("");
                let resp = if method == "POST" {
                    p2.lock().unwrap().push(path.to_string());
                    let reason = if test_status == 200 {
                        "OK"
                    } else {
                        "Bad Request"
                    };
                    format!("HTTP/1.1 {test_status} {reason}\r\nContent-Length: 2\r\n\r\nok")
                } else {
                    format!(
                        "HTTP/1.1 200 OK\r\nContent-Length: {}\r\n\r\n{config}",
                        config.len()
                    )
                };
                let _ = s.write_all(resp.as_bytes());
            }
        });
        (addr, posts)
    }

    #[test]
    fn status_reads_ntfy_url_and_enabled() {
        let (addr, _) = serve(
            r#"{"ntfy_url":"https://ntfy.sh/foo","enabled_notifications":["Warning","LowBattery"]}"#,
            200,
        );
        let s = status_at(addr, Duration::from_secs(2)).unwrap();
        assert_eq!(s.ntfy_url.as_deref(), Some("https://ntfy.sh/foo"));
        assert_eq!(
            s.enabled,
            vec!["Warning".to_string(), "LowBattery".to_string()]
        );
    }

    #[test]
    fn status_treats_null_or_empty_ntfy_url_as_unset() {
        let (addr, _) = serve(r#"{"ntfy_url":null,"enabled_notifications":[]}"#, 200);
        let s = status_at(addr, Duration::from_secs(2)).unwrap();
        assert!(s.ntfy_url.is_none());
        assert!(s.enabled.is_empty());
    }

    #[test]
    fn test_posts_to_test_notification() {
        let (addr, posts) = serve(r#"{"ntfy_url":"https://ntfy.sh/foo"}"#, 200);
        assert!(test_at(addr, Duration::from_secs(2)).is_ok());
        assert_eq!(posts.lock().unwrap().as_slice(), ["/api/test-notification"]);
    }

    #[test]
    fn test_reports_missing_url_on_400() {
        let (addr, _) = serve(r#"{"ntfy_url":null}"#, 400);
        let err = test_at(addr, Duration::from_secs(2)).unwrap_err();
        assert!(err.contains("no ntfy_url"), "got: {err}");
    }
}
