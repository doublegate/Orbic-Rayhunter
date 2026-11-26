//! `orx alert`: surface and exercise the Rayhunter daemon's BUILT-IN alerting
//! over the `:8080` API. The daemon already sends a `Warning` notification on
//! analyzer detections (and `LowBattery`) to its configured `ntfy_url`, with
//! per-type debounce and exponential-backoff retry -- so "alerting" is a matter
//! of pointing `ntfy_url` at a sink, not re-implementing it on the host.
//!
//! `status` (read-only) shows the current `ntfy_url` and enabled notification
//! types. `test` asks the daemon to POST a test notification to that URL.
//! `set` (T-OR-107) writes `ntfy_url` from the host via a read-modify-write of
//! `/api/config`.
//!
//! Safety of `set`: `POST /api/config` takes the FULL config and triggers a
//! daemon restart, and `GET /api/config` redacts `wifi_password` to `null`.
//! Rather than reconstruct a config, `set` round-trips the exact object the GET
//! returned and replaces only the `ntfy_url` key -- which is byte-for-byte what
//! the upstream web UI's config form POSTs when you change the notification URL
//! (it, too, sends `wifi_password: null`). The daemon's `set_config` strips the
//! Wi-Fi fields before writing `config.toml` (they live in `wpa_sta.conf`), so
//! the write cannot clobber a password that config.toml never held. The one
//! residual side effect is that the restart re-applies `wpa_sta.conf`; `set`
//! warns when Wi-Fi client mode is active so the operator can verify the link.
//!
//! Backup gate: like `orx purge` (whose verified local pull copy is the undo),
//! `set --apply` is gated on an operation-appropriate restore path, not the
//! firmware-grade full-device backup that `install`/`uninstall` require -- a
//! config write cannot brick the device and is reversible. Before it POSTs, `set`
//! writes the exact prior `/api/config` body to a local snapshot and verifies it
//! reads back byte-identical; if that fails it aborts WITHOUT touching the device,
//! and re-POSTing the snapshot restores the previous config.

use crate::json::Json;
use crate::pull::{self, PullError};
use std::net::SocketAddr;
use std::path::{Path, PathBuf};
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
        Err(PullError::Http(400)) => Err("no ntfy_url is configured on the device; set one with \
             `orx alert set --ntfy-url <url> --apply` (or in the Rayhunter web UI) first"
            .to_string()),
        Err(e) => Err(format!("test notification failed: {e}")),
    }
}

/// What [`set`] should write to the device's `ntfy_url`.
pub enum NtfyTarget {
    /// Point alerts at this URL (validated as http/https).
    Url(String),
    /// Remove any configured URL (disables delivery).
    Clear,
}

/// Outcome of a [`set`] call (a dry-run preview or an applied write).
#[derive(Debug)]
pub struct SetOutcome {
    /// The `ntfy_url` before the change (`None` if it was unset).
    pub previous: Option<String>,
    /// The `ntfy_url` after the change (`None` when cleared).
    pub applied: Option<String>,
    /// True once the new config was POSTed to the device (false on a dry run).
    pub written: bool,
    /// True when the device is in Wi-Fi client mode (`wifi_enabled` + an SSID),
    /// so the caller can warn that the daemon restart re-applies `wpa_sta.conf`.
    pub wifi_client_active: bool,
    /// Where the prior config was snapshotted before the write (the undo), on a
    /// successful `--apply`; `None` on a dry run.
    pub backup_path: Option<PathBuf>,
    /// SHA-256 of the snapshot at [`Self::backup_path`] (so the operator can
    /// later confirm the restore file's integrity); `None` on a dry run.
    pub backup_sha256: Option<String>,
}

/// Validate a user-supplied ntfy/webhook URL before it reaches the device. A
/// bad URL would silently mean "no alerts delivered", so reject obvious
/// mistakes: it must be an http(s) URL with a host and no control/space chars.
pub fn validate_ntfy_url(url: &str) -> Result<(), String> {
    if url.len() > 2048 {
        return Err("ntfy_url is too long (max 2048 chars)".to_string());
    }
    if url.chars().any(|c| c.is_control() || c == ' ') {
        return Err("ntfy_url must not contain spaces or control characters".to_string());
    }
    let rest = url
        .strip_prefix("https://")
        .or_else(|| url.strip_prefix("http://"))
        .ok_or_else(|| "ntfy_url must start with http:// or https://".to_string())?;
    // The authority is everything before the first '/', '?' or '#'.
    let authority = rest.split(['/', '?', '#']).next().unwrap_or("");
    // Reject embedded userinfo (user:pass@host): credentials belong in the
    // config, not the URL, and a userinfo URL would be printed verbatim by
    // `orx alert set`, leaking the secret to stdout/logs.
    if authority.contains('@') {
        return Err(
            "ntfy_url must not contain embedded credentials (user:pass@); configure auth separately"
                .to_string(),
        );
    }
    // Split host[:port]. A bracketed IPv6 literal ([::1][:port]) keeps its colons.
    let (host, port) = if let Some(after_open) = authority.strip_prefix('[') {
        let close = after_open
            .find(']')
            .ok_or_else(|| "ntfy_url has an unterminated IPv6 host".to_string())?;
        let host = &after_open[..close];
        let tail = &after_open[close + 1..];
        let port = match tail {
            "" => None,
            _ => Some(
                tail.strip_prefix(':')
                    .ok_or_else(|| "ntfy_url has a malformed IPv6 authority".to_string())?,
            ),
        };
        (host, port)
    } else {
        // split_once (not rsplit_once): a second ':' falls into the port and is
        // rejected below, so "a:b:8080" and unbracketed IPv6 like "::1" fail.
        match authority.split_once(':') {
            Some((h, p)) => (h, Some(p)),
            None => (authority, None),
        }
    };
    if host.is_empty() {
        return Err("ntfy_url has no host".to_string());
    }
    // A port, if present, must parse as a non-zero u16 (rejects empty, non-digit,
    // out-of-range, a stray second ':', and port 0).
    if let Some(port) = port
        && !matches!(port.parse::<u16>(), Ok(p) if p > 0)
    {
        return Err("ntfy_url has an invalid port".to_string());
    }
    Ok(())
}

/// Write (or clear) the device's `ntfy_url`. With `apply == false` this performs
/// only the read and returns a preview, writing nothing. On `--apply` the prior
/// config is snapshotted into `backup_dir` (the undo) before the device is
/// touched. See the module docs for why a faithful round-trip of `GET
/// /api/config` plus a local snapshot is the safe, proportionate gate here.
pub fn set(
    ip: &str,
    target: NtfyTarget,
    apply: bool,
    backup_dir: &Path,
    timeout: Duration,
) -> Result<SetOutcome, String> {
    let addr = crate::probe::socket(ip, crate::probe::RAYHUNTER_PORT)?;
    set_at(addr, target, apply, backup_dir, timeout)
}

/// As [`set`] but against an explicit address (tests use a loopback server).
pub fn set_at(
    addr: SocketAddr,
    target: NtfyTarget,
    apply: bool,
    backup_dir: &Path,
    timeout: Duration,
) -> Result<SetOutcome, String> {
    if let NtfyTarget::Url(u) = &target {
        validate_ntfy_url(u)?;
    }

    let body = pull::http_get_string(addr, "/api/config", MAX_CONFIG, timeout)
        .map_err(|e| format!("fetch config: {e}"))?;
    let mut v = crate::json::parse(&body).map_err(|e| format!("config JSON: {e}"))?;
    if !matches!(v, Json::Obj(_)) {
        return Err("unexpected config response (not a JSON object)".to_string());
    }
    // Only write back a body that actually looks like Rayhunter's config, so a
    // wrong endpoint or a captive-portal 200 cannot make us POST garbage that
    // the daemon would then try to adopt (and restart into).
    for key in ["qmdl_store_path", "port", "enabled_notifications"] {
        if v.get(key).is_none() {
            return Err(format!(
                "config response is missing the expected key {key:?}; refusing to write back a \
                 body that does not look like Rayhunter's config"
            ));
        }
    }

    let previous = v
        .get("ntfy_url")
        .and_then(Json::as_str)
        .filter(|s| !s.is_empty())
        .map(str::to_string);
    let wifi_client_active = v
        .get("wifi_enabled")
        .and_then(Json::as_bool)
        .unwrap_or(false)
        && v.get("wifi_ssid")
            .and_then(Json::as_str)
            .map(|s| !s.is_empty())
            .unwrap_or(false);

    let applied = match &target {
        NtfyTarget::Url(u) => {
            v.set("ntfy_url", Json::Str(u.clone()));
            Some(u.clone())
        }
        NtfyTarget::Clear => {
            v.set("ntfy_url", Json::Null);
            None
        }
    };

    if !apply {
        return Ok(SetOutcome {
            previous,
            applied,
            written: false,
            wifi_client_active,
            backup_path: None,
            backup_sha256: None,
        });
    }

    // Device-write backup gate: snapshot the EXACT prior config and verify it
    // reads back byte-identical BEFORE we POST. A config write is reversible, so
    // this local snapshot (re-POST it to restore) is the operation-appropriate
    // restore path, mirroring `orx purge`'s verified pull copy -- not the
    // firmware-grade device backup `install`/`uninstall` require. Fail closed:
    // if the snapshot cannot be written and verified, the device is untouched.
    let (backup_path, backup_sha256) = write_config_snapshot(backup_dir, addr, body.as_bytes())?;

    let out = v.to_json();
    match pull::http_post_json(addr, "/api/config", &out, timeout) {
        Ok(_) => Ok(SetOutcome {
            previous,
            applied,
            written: true,
            wifi_client_active,
            backup_path: Some(backup_path),
            backup_sha256: Some(backup_sha256),
        }),
        Err(PullError::Http(422)) => Err(
            "the device rejected the config body (HTTP 422): the JSON did not deserialize into \
             Rayhunter's config schema"
                .to_string(),
        ),
        Err(e) => Err(format!("write config: {e}")),
    }
}

/// Snapshot the prior config `body` into `dir`, verify it reads back
/// byte-identical, and return the snapshot path plus its SHA-256. This is the
/// verified, checksummed undo for a `set --apply`; the write is gated on it.
fn write_config_snapshot(
    dir: &Path,
    addr: SocketAddr,
    body: &[u8],
) -> Result<(PathBuf, String), String> {
    std::fs::create_dir_all(dir)
        .map_err(|e| format!("create backup dir {}: {e}", dir.display()))?;
    let stamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    let label: String = addr
        .to_string()
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '-' })
        .collect();
    let path = dir.join(format!("orx-config-before-{label}-{stamp}.json"));
    pull::write_atomic(&path, body)?;
    let back =
        std::fs::read(&path).map_err(|e| format!("re-read backup {}: {e}", path.display()))?;
    if back != body {
        return Err(format!(
            "backup verification failed for {} (contents differ)",
            path.display()
        ));
    }
    let sha = pull::file_sha256(&path)?;
    Ok((path, sha))
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

    /// A representative config body: integers, nested objects, a null
    /// `ntfy_url` and the redacted-to-null `wifi_password`, as `GET /api/config`
    /// returns it. Used to prove `set` round-trips everything but `ntfy_url`.
    const SAMPLE_CONFIG: &str = r#"{"qmdl_store_path":"/data/rayhunter/qmdl","port":8080,"ntfy_url":null,"enabled_notifications":["Warning","LowBattery"],"min_space_to_start_recording_mb":64,"wifi_enabled":false,"wifi_ssid":null,"wifi_password":null,"analyzers":{"imsi_requested":true},"gps_fixed_latitude":null}"#;

    /// Captured POSTs: a list of `(path, body)` the loopback server received.
    type CapturedPosts = Arc<Mutex<Vec<(String, String)>>>;

    /// Loopback daemon for `set`: serves `config` on GET and captures the full
    /// body of any POST, answering `/api/config` POSTs with `post_status`.
    fn serve_set(config: &'static str, post_status: u16) -> (SocketAddr, CapturedPosts) {
        let l = TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = l.local_addr().unwrap();
        let posts = Arc::new(Mutex::new(Vec::new()));
        let p2 = Arc::clone(&posts);
        std::thread::spawn(move || {
            while let Ok((mut s, _)) = l.accept() {
                let mut data = Vec::new();
                let mut buf = [0u8; 4096];
                let hdr_end = loop {
                    match s.read(&mut buf) {
                        Ok(0) => break 0usize,
                        Ok(n) => {
                            data.extend_from_slice(&buf[..n]);
                            if let Some(p) = data.windows(4).position(|w| w == b"\r\n\r\n") {
                                break p + 4;
                            }
                            if data.len() > 512 * 1024 {
                                break 0usize;
                            }
                        }
                        Err(_) => break 0usize,
                    }
                };
                if hdr_end == 0 {
                    continue;
                }
                let head = String::from_utf8_lossy(&data[..hdr_end]).to_string();
                let mut parts = head.lines().next().unwrap_or("").split_whitespace();
                let method = parts.next().unwrap_or("");
                let path = parts.next().unwrap_or("").to_string();
                let clen = head
                    .lines()
                    .find_map(|l| {
                        l.to_ascii_lowercase()
                            .strip_prefix("content-length:")
                            .map(|v| v.trim().parse::<usize>().unwrap_or(0))
                    })
                    .unwrap_or(0);
                let resp = if method == "POST" {
                    let mut body = data[hdr_end..].to_vec();
                    while body.len() < clen {
                        match s.read(&mut buf) {
                            Ok(0) => break,
                            Ok(n) => body.extend_from_slice(&buf[..n]),
                            Err(_) => break,
                        }
                    }
                    body.truncate(clen);
                    p2.lock()
                        .unwrap()
                        .push((path.clone(), String::from_utf8_lossy(&body).to_string()));
                    let reason = if post_status == 202 {
                        "Accepted"
                    } else {
                        "Unprocessable Entity"
                    };
                    format!("HTTP/1.1 {post_status} {reason}\r\nContent-Length: 2\r\n\r\nok")
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

    /// A fresh, unique temp directory for a test's config snapshot.
    fn tmpdir() -> PathBuf {
        use std::sync::atomic::{AtomicU32, Ordering};
        static N: AtomicU32 = AtomicU32::new(0);
        let n = N.fetch_add(1, Ordering::Relaxed);
        let d = std::env::temp_dir().join(format!("orx-alert-test-{}-{n}", std::process::id()));
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    #[test]
    fn validate_ntfy_url_accepts_http_and_https() {
        assert!(validate_ntfy_url("https://ntfy.sh/my-topic").is_ok());
        assert!(validate_ntfy_url("http://192.168.1.5:8080/hook").is_ok());
    }

    #[test]
    fn validate_ntfy_url_rejects_bad_input() {
        assert!(validate_ntfy_url("ftp://ntfy.sh/x").is_err());
        assert!(validate_ntfy_url("ntfy.sh/x").is_err());
        assert!(validate_ntfy_url("https://").is_err());
        assert!(validate_ntfy_url("https://ntfy.sh/ has space").is_err());
        assert!(validate_ntfy_url("https://ntfy.sh/\n").is_err());
        // No host before the path.
        assert!(validate_ntfy_url("https://:8080/topic").is_err());
        // Embedded credentials (would leak when printed) are rejected.
        assert!(validate_ntfy_url("https://user:pass@ntfy.sh/topic").is_err());
        assert!(validate_ntfy_url("https://user:pass@/topic").is_err());
        // Invalid / out-of-range / zero port.
        assert!(validate_ntfy_url("https://ntfy.sh:abc/topic").is_err());
        assert!(validate_ntfy_url("https://ntfy.sh:99999/topic").is_err());
        assert!(validate_ntfy_url("https://ntfy.sh:0/topic").is_err());
        // A second ':' in a non-bracketed authority (and unbracketed IPv6).
        assert!(validate_ntfy_url("https://a:b:8080/topic").is_err());
        assert!(validate_ntfy_url("http://::1/topic").is_err());
    }

    #[test]
    fn validate_ntfy_url_accepts_host_port_and_ipv6() {
        assert!(validate_ntfy_url("https://ntfy.sh:8080/topic").is_ok());
        assert!(validate_ntfy_url("http://[::1]:8080/topic").is_ok());
        assert!(validate_ntfy_url("http://[2001:db8::1]/topic").is_ok());
    }

    #[test]
    fn set_dry_run_previews_without_posting() {
        let (addr, posts) = serve_set(SAMPLE_CONFIG, 202);
        let td = tmpdir();
        let o = set_at(
            addr,
            NtfyTarget::Url("https://ntfy.sh/foo".to_string()),
            false,
            &td,
            Duration::from_secs(2),
        )
        .unwrap();
        assert!(!o.written);
        assert_eq!(o.previous, None);
        assert_eq!(o.applied.as_deref(), Some("https://ntfy.sh/foo"));
        assert!(!o.wifi_client_active);
        assert!(o.backup_path.is_none(), "dry run must not write a backup");
        assert!(posts.lock().unwrap().is_empty(), "dry run must not POST");
        // A dry run writes nothing into the backup dir.
        assert_eq!(std::fs::read_dir(&td).unwrap().count(), 0);
    }

    #[test]
    fn set_apply_posts_config_preserving_other_keys() {
        let (addr, posts) = serve_set(SAMPLE_CONFIG, 202);
        let td = tmpdir();
        let o = set_at(
            addr,
            NtfyTarget::Url("https://ntfy.sh/foo".to_string()),
            true,
            &td,
            Duration::from_secs(2),
        )
        .unwrap();
        assert!(o.written);
        // The backup snapshot exists and holds the EXACT prior config.
        let bp = o.backup_path.as_ref().expect("apply writes a backup");
        assert_eq!(std::fs::read(bp).unwrap(), SAMPLE_CONFIG.as_bytes());
        // Its SHA-256 is reported and matches the snapshot on disk (64 hex).
        let sha = o.backup_sha256.as_ref().expect("apply reports a sha256");
        assert_eq!(sha.len(), 64);
        assert_eq!(sha, &crate::pull::file_sha256(bp).unwrap());
        let posts = posts.lock().unwrap();
        assert_eq!(posts.len(), 1);
        let (path, body) = &posts[0];
        assert_eq!(path, "/api/config");
        // Integers stay integers (no "8080.0"), and every other key survives.
        assert!(body.contains(r#""port":8080"#), "port not integral: {body}");
        assert!(!body.contains("8080.0"), "port serialized as float: {body}");
        let parsed = crate::json::parse(body).unwrap();
        assert_eq!(
            parsed.get("ntfy_url").and_then(Json::as_str),
            Some("https://ntfy.sh/foo")
        );
        assert_eq!(parsed.get("port").and_then(Json::as_u64), Some(8080));
        assert_eq!(
            parsed
                .get("min_space_to_start_recording_mb")
                .and_then(Json::as_u64),
            Some(64)
        );
        // The redacted wifi_password key is preserved verbatim (null, not dropped).
        assert!(parsed.get("wifi_password").map(Json::is_null) == Some(true));
        assert_eq!(
            parsed
                .get("analyzers")
                .and_then(|a| a.get("imsi_requested"))
                .and_then(Json::as_bool),
            Some(true)
        );
        assert_eq!(
            parsed
                .get("enabled_notifications")
                .and_then(Json::as_array)
                .map(<[_]>::len),
            Some(2)
        );
    }

    #[test]
    fn set_clear_writes_null_url() {
        let cfg = r#"{"qmdl_store_path":"/d","port":8080,"ntfy_url":"https://ntfy.sh/old","enabled_notifications":[]}"#;
        let (addr, posts) = serve_set(cfg, 202);
        let o = set_at(
            addr,
            NtfyTarget::Clear,
            true,
            &tmpdir(),
            Duration::from_secs(2),
        )
        .unwrap();
        assert_eq!(o.previous.as_deref(), Some("https://ntfy.sh/old"));
        assert_eq!(o.applied, None);
        assert!(o.backup_path.is_some(), "apply writes a backup");
        let posts = posts.lock().unwrap();
        let parsed = crate::json::parse(&posts[0].1).unwrap();
        assert!(parsed.get("ntfy_url").map(Json::is_null) == Some(true));
    }

    #[test]
    fn set_detects_wifi_client_mode() {
        let cfg = r#"{"qmdl_store_path":"/d","port":8080,"enabled_notifications":[],"wifi_enabled":true,"wifi_ssid":"home","wifi_password":null}"#;
        let (addr, _) = serve_set(cfg, 202);
        let o = set_at(
            addr,
            NtfyTarget::Url("https://ntfy.sh/foo".to_string()),
            false,
            &tmpdir(),
            Duration::from_secs(2),
        )
        .unwrap();
        assert!(o.wifi_client_active);
    }

    #[test]
    fn set_refuses_non_rayhunter_body() {
        let (addr, posts) = serve_set(r#"{"hello":"world"}"#, 202);
        let err = set_at(
            addr,
            NtfyTarget::Url("https://ntfy.sh/foo".to_string()),
            true,
            &tmpdir(),
            Duration::from_secs(2),
        )
        .unwrap_err();
        assert!(err.contains("expected key"), "got: {err}");
        assert!(posts.lock().unwrap().is_empty(), "must not POST garbage");
    }

    #[test]
    fn set_maps_422_to_clear_error() {
        let (addr, _) = serve_set(SAMPLE_CONFIG, 422);
        let err = set_at(
            addr,
            NtfyTarget::Url("https://ntfy.sh/foo".to_string()),
            true,
            &tmpdir(),
            Duration::from_secs(2),
        )
        .unwrap_err();
        assert!(err.contains("422"), "got: {err}");
    }

    #[test]
    fn set_apply_aborts_before_post_when_backup_fails() {
        let (addr, posts) = serve_set(SAMPLE_CONFIG, 202);
        // Point the backup "dir" at a path whose parent is a regular file, so
        // create_dir_all cannot succeed -- the write must fail closed, untouched.
        let f = tmpdir().join("not-a-dir");
        std::fs::write(&f, b"x").unwrap();
        let bad = f.join("sub");
        let err = set_at(
            addr,
            NtfyTarget::Url("https://ntfy.sh/foo".to_string()),
            true,
            &bad,
            Duration::from_secs(2),
        )
        .unwrap_err();
        assert!(err.contains("backup"), "got: {err}");
        assert!(
            posts.lock().unwrap().is_empty(),
            "must not POST when the backup gate fails"
        );
    }
}
