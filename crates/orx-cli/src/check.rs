//! `orx check`: a read-only device health report. It queries the Rayhunter
//! daemon's `:8080` API (`/api/system-stats`, `/api/qmdl-manifest`) and probes a
//! few ports, then prints a per-item `[ok]/[warn]/[fail]` summary. It never
//! writes to the device.

use crate::json::Json;
use crate::probe;
use std::net::SocketAddr;
use std::time::Duration;

/// Port the device-side `nc` root shell listens on when present (NOT
/// reboot-persistent; should be CLOSED on a healthy post-install device).
const ROOTSHELL_PORT: u16 = 24;
/// Port `orx install` opens transiently for its lossless transfer; should be
/// CLOSED when no install is in progress.
const INSTALLER_PORT: u16 = 8081;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Level {
    Ok,
    Warn,
    Fail,
}

impl Level {
    pub fn tag(self) -> &'static str {
        match self {
            Level::Ok => "[ok]  ",
            Level::Warn => "[warn]",
            Level::Fail => "[fail]",
        }
    }
}

pub struct CheckReport {
    pub items: Vec<(Level, String)>,
}

impl CheckReport {
    /// Overall health: no `Fail` items (warnings do not fail the check).
    pub fn healthy(&self) -> bool {
        self.items.iter().all(|(l, _)| *l != Level::Fail)
    }
    fn push(&mut self, level: Level, msg: impl Into<String>) {
        self.items.push((level, msg.into()));
    }
}

/// Run the health check against the device at `ip`.
pub fn check(ip: &str, timeout: Duration) -> Result<CheckReport, String> {
    let daemon = probe::socket(ip, probe::RAYHUNTER_PORT)?;
    let rootshell = probe::socket(ip, ROOTSHELL_PORT)?;
    let installer = probe::socket(ip, INSTALLER_PORT)?;
    Ok(check_inner(daemon, rootshell, installer, timeout))
}

/// Core check against explicit addresses. The daemon API is queried on `daemon`
/// (and its port is what the daemon-port probe tests); `rootshell` and
/// `installer` are the ports that must be CLOSED. Tests inject closed ephemeral
/// addresses so the result never depends on services running on the host.
///
/// Fails closed: a broken or unexpected API response, or an exposed shell / stale
/// installer listener, is a `Fail`, not a `Warn` -- the check must not report
/// "healthy" when it cannot actually confirm health.
pub fn check_inner(
    daemon: SocketAddr,
    rootshell: SocketAddr,
    installer: SocketAddr,
    timeout: Duration,
) -> CheckReport {
    let mut r = CheckReport { items: Vec::new() };

    // 1. Daemon reachable + system stats. Anything that stops us confirming a
    // Rayhunter daemon (unreachable, non-JSON, or missing the version) fails.
    match crate::pull::http_get_string(daemon, "/api/system-stats", 256 * 1024, timeout) {
        Ok(body) => match crate::json::parse(&body) {
            Ok(stats) => {
                r.push(Level::Ok, format!("daemon reachable on {daemon}"));
                if !report_stats(&mut r, &stats) {
                    r.push(
                        Level::Fail,
                        "system-stats has no runtime_metadata.rayhunter_version \
                         (not a Rayhunter daemon?)"
                            .to_string(),
                    );
                }
            }
            Err(e) => r.push(
                Level::Fail,
                format!("daemon returned unparsable system-stats: {e}"),
            ),
        },
        Err(e) => r.push(
            Level::Fail,
            format!("daemon NOT reachable on {daemon}: {e}"),
        ),
    }

    // 2. Recording status. A broken manifest endpoint is a failure of the API.
    match crate::pull::http_get_string(daemon, "/api/qmdl-manifest", 4 * 1024 * 1024, timeout) {
        Ok(body) => match crate::json::parse(&body) {
            Ok(m) => report_recording(&mut r, &m),
            Err(e) => r.push(Level::Fail, format!("manifest unparsable: {e}")),
        },
        Err(e) => r.push(Level::Fail, format!("manifest unavailable: {e}")),
    }

    // 3. Port posture. The daemon port should be open; the root shell and the
    // installer transfer port should be CLOSED on a settled device -- an open
    // one is a security exposure, so it fails the check rather than warning.
    if probe::tcp_open(daemon, timeout) {
        r.push(Level::Ok, format!("daemon port {} open", daemon.port()));
    } else {
        r.push(Level::Fail, format!("daemon port {} closed", daemon.port()));
    }
    if probe::tcp_open(rootshell, timeout) {
        r.push(
            Level::Fail,
            format!(
                "root shell port {} is OPEN (exposed; expected closed)",
                rootshell.port()
            ),
        );
    } else {
        r.push(
            Level::Ok,
            format!("root shell port {} closed", rootshell.port()),
        );
    }
    if probe::tcp_open(installer, timeout) {
        r.push(
            Level::Fail,
            format!(
                "installer port {} is OPEN (stale transfer listener?)",
                installer.port()
            ),
        );
    } else {
        r.push(
            Level::Ok,
            format!("installer port {} closed", installer.port()),
        );
    }

    r
}

/// Report the stats fields; returns whether the rayhunter version was present
/// (its absence means the response is not a Rayhunter daemon's stats).
fn report_stats(r: &mut CheckReport, stats: &Json) -> bool {
    let meta = stats.get("runtime_metadata");
    // Require a non-empty version; its absence means it is not a Rayhunter
    // daemon's stats (check_inner then records the Fail). Don't print an
    // "[ok] rayhunter ?" line in that case.
    let version = meta
        .and_then(|m| m.get("rayhunter_version"))
        .and_then(Json::as_str)
        .filter(|v| !v.is_empty());
    if let (Some(meta), Some(ver)) = (meta, version) {
        let os = meta.get("system_os").and_then(Json::as_str).unwrap_or("?");
        let arch = meta.get("arch").and_then(Json::as_str).unwrap_or("?");
        r.push(
            Level::Ok,
            format!("rayhunter {} on {} ({})", san(ver), san(os), san(arch)),
        );
    }
    if let Some(disk) = stats.get("disk_stats") {
        let used = disk.get("used_size").and_then(Json::as_str).unwrap_or("?");
        let total = disk.get("total_size").and_then(Json::as_str).unwrap_or("?");
        let avail = disk
            .get("available_size")
            .and_then(Json::as_str)
            .unwrap_or("?");
        let pct = disk
            .get("used_percent")
            .and_then(Json::as_str)
            .unwrap_or("?");
        // Warn when the capture partition is nearly full (daemon stops recording).
        let full = disk
            .get("used_percent")
            .and_then(Json::as_str)
            .and_then(|s| s.trim_end_matches('%').parse::<u32>().ok())
            .is_some_and(|p| p >= 90);
        let level = if full { Level::Warn } else { Level::Ok };
        r.push(
            level,
            format!(
                "disk {}/{} used ({}), {} free",
                san(used),
                san(total),
                san(pct),
                san(avail)
            ),
        );
    }
    if let Some(mem) = stats.get("memory_stats") {
        let used = mem.get("used").and_then(Json::as_str).unwrap_or("?");
        let total = mem.get("total").and_then(Json::as_str).unwrap_or("?");
        r.push(
            Level::Ok,
            format!("memory {}/{} used", san(used), san(total)),
        );
    }
    if let Some(bat) = stats.get("battery_status") {
        let level = bat.get("level").and_then(Json::as_f64).map(|l| l as i64);
        let plugged = matches!(bat.get("is_plugged_in"), Some(Json::Bool(true)));
        let pct = level
            .map(|l| format!("{l}%"))
            .unwrap_or_else(|| "?".to_string());
        let src = if plugged { "charging" } else { "on battery" };
        let warn = level.is_some_and(|l| l < 20) && !plugged;
        r.push(
            if warn { Level::Warn } else { Level::Ok },
            format!("battery {pct} ({src})"),
        );
    }
    version.is_some()
}

/// Strip control characters from an untrusted device string before printing, so
/// a compromised device or rogue `--ip` target cannot inject ANSI escapes or
/// forge `[ok]` status lines in the terminal. Also length-bounded.
fn san(s: &str) -> String {
    s.chars().filter(|c| !c.is_control()).take(120).collect()
}

fn report_recording(r: &mut CheckReport, manifest: &Json) {
    // The manifest must carry an `entries` array; its absence means the response
    // is not a real qmdl-manifest (fail closed rather than silently pass).
    let Some(entries) = manifest.get("entries").and_then(Json::as_array) else {
        r.push(
            Level::Fail,
            "manifest has no `entries` array (unexpected response)".to_string(),
        );
        return;
    };
    // "Active" only when current_entry is an object with a non-empty string name;
    // a missing, null, or malformed current_entry means no active recording.
    let active = manifest
        .get("current_entry")
        .filter(|c| !c.is_null())
        .and_then(|c| c.get("name"))
        .and_then(Json::as_str)
        .filter(|n| !n.is_empty());
    match active {
        Some(name) => r.push(
            Level::Ok,
            format!("recording active (current entry {})", san(name)),
        ),
        None => r.push(Level::Warn, "no recording active".to_string()),
    }
    r.push(
        Level::Ok,
        format!("{} completed recording(s) stored", entries.len()),
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{Read, Write};
    use std::net::TcpListener;

    #[test]
    fn healthy_requires_no_fail() {
        let mut r = CheckReport { items: Vec::new() };
        r.push(Level::Ok, "a");
        r.push(Level::Warn, "b");
        assert!(r.healthy());
        r.push(Level::Fail, "c");
        assert!(!r.healthy());
    }

    #[test]
    fn report_stats_reads_fields_and_warns_on_full_disk() {
        let stats = crate::json::parse(
            r#"{"disk_stats":{"total_size":"214.7M","used_size":"200M","available_size":"14M","used_percent":"95%"},"memory_stats":{"total":"159.8M","used":"136.1M"},"runtime_metadata":{"rayhunter_version":"0.13.0","system_os":"Linux 3.18.48","arch":"armv7l"},"battery_status":{"level":100,"is_plugged_in":false}}"#,
        )
        .unwrap();
        let mut r = CheckReport { items: Vec::new() };
        report_stats(&mut r, &stats);
        let text = r
            .items
            .iter()
            .map(|(_, m)| m.clone())
            .collect::<Vec<_>>()
            .join("\n");
        assert!(text.contains("rayhunter 0.13.0"));
        assert!(text.contains("armv7l"));
        // 95% disk -> a Warn item exists.
        assert!(
            r.items
                .iter()
                .any(|(l, m)| *l == Level::Warn && m.contains("disk"))
        );
    }

    #[test]
    fn report_recording_detects_active_and_count() {
        let m = crate::json::parse(
            r#"{"entries":[{"name":"1"},{"name":"2"}],"current_entry":{"name":"3"}}"#,
        )
        .unwrap();
        let mut r = CheckReport { items: Vec::new() };
        report_recording(&mut r, &m);
        let text = r
            .items
            .iter()
            .map(|(_, x)| x.clone())
            .collect::<Vec<_>>()
            .join("\n");
        assert!(text.contains("recording active"));
        assert!(text.contains("2 completed"));
    }

    /// A loopback daemon that keeps its listener alive (so the daemon-port probe
    /// after the two API reads still sees it open) and answers both API paths.
    fn serve_stats(stats: &'static str, manifest: &'static str) -> SocketAddr {
        let l = TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = l.local_addr().unwrap();
        std::thread::spawn(move || {
            while let Ok((mut s, _)) = l.accept() {
                let mut req = [0u8; 1024];
                let n = s.read(&mut req).unwrap_or(0);
                let line = String::from_utf8_lossy(&req[..n]);
                let path = line.split_whitespace().nth(1).unwrap_or("");
                let body = if path.contains("system-stats") {
                    stats
                } else {
                    manifest
                };
                let _ = s.write_all(
                    format!(
                        "HTTP/1.1 200 OK\r\nContent-Length: {}\r\n\r\n{body}",
                        body.len()
                    )
                    .as_bytes(),
                );
            }
        });
        addr
    }

    /// An ephemeral address that is guaranteed closed (bound, then dropped), so a
    /// probe of it never depends on a service running on the host.
    fn closed_addr() -> SocketAddr {
        let l = TcpListener::bind("127.0.0.1:0").unwrap();
        let a = l.local_addr().unwrap();
        drop(l);
        a
    }

    #[test]
    fn check_inner_reports_daemon_up_and_ports_healthy() {
        let daemon = serve_stats(
            r#"{"runtime_metadata":{"rayhunter_version":"0.13.0","system_os":"Linux","arch":"armv7l"}}"#,
            r#"{"entries":[],"current_entry":{"name":"9"}}"#,
        );
        let rootshell = closed_addr();
        let installer = closed_addr();
        let r = check_inner(daemon, rootshell, installer, Duration::from_secs(2));
        let text = r
            .items
            .iter()
            .map(|(l, m)| format!("{} {m}", l.tag()))
            .collect::<Vec<_>>()
            .join("\n");
        assert!(text.contains("daemon reachable"), "{text}");
        assert!(text.contains("rayhunter 0.13.0"), "{text}");
        assert!(text.contains("recording active"), "{text}");
        assert!(
            text.contains(&format!("root shell port {} closed", rootshell.port())),
            "{text}"
        );
        assert!(r.healthy(), "all items ok -> healthy: {text}");
    }

    #[test]
    fn check_inner_fails_on_exposed_root_shell() {
        let daemon = serve_stats(
            r#"{"runtime_metadata":{"rayhunter_version":"0.13.0"}}"#,
            r#"{"entries":[]}"#,
        );
        // A live listener for the "root shell" -> an OPEN port -> Fail.
        let shell = TcpListener::bind("127.0.0.1:0").unwrap();
        let shell_addr = shell.local_addr().unwrap();
        let r = check_inner(daemon, shell_addr, closed_addr(), Duration::from_secs(2));
        assert!(!r.healthy(), "an exposed root shell must fail the check");
        assert!(
            r.items
                .iter()
                .any(|(l, m)| *l == Level::Fail && m.contains("root shell")),
        );
    }

    #[test]
    fn report_recording_fails_without_entries_array() {
        for body in ["{}", "[]", "null", r#"{"entries":"nope"}"#] {
            let m = crate::json::parse(body).unwrap();
            let mut r = CheckReport { items: Vec::new() };
            report_recording(&mut r, &m);
            assert!(!r.healthy(), "manifest {body:?} should fail");
        }
        // A malformed current_entry is "no recording active", not active.
        let m = crate::json::parse(r#"{"entries":[],"current_entry":{}}"#).unwrap();
        let mut r = CheckReport { items: Vec::new() };
        report_recording(&mut r, &m);
        let text = r
            .items
            .iter()
            .map(|(_, x)| x.clone())
            .collect::<Vec<_>>()
            .join("\n");
        assert!(text.contains("no recording active"), "{text}");
    }

    #[test]
    fn device_strings_are_sanitized() {
        // An injected ANSI escape + a forged status line must be stripped.
        let stats = crate::json::parse(
            "{\"runtime_metadata\":{\"rayhunter_version\":\"\\u001b[31m0.13\",\"system_os\":\"a\\nb\",\"arch\":\"x\"}}",
        )
        .unwrap();
        let mut r = CheckReport { items: Vec::new() };
        assert!(report_stats(&mut r, &stats));
        let text = r
            .items
            .iter()
            .map(|(_, m)| m.clone())
            .collect::<Vec<_>>()
            .join("\n");
        assert!(!text.contains('\u{1b}'), "escape not stripped: {text:?}");
        assert!(!text.contains('\n') || text.lines().count() >= 1); // os newline stripped from the value
        assert!(text.contains("0.13") && text.contains("ab"), "{text}");
    }

    #[test]
    fn empty_version_is_not_confirmed() {
        let stats = crate::json::parse(r#"{"runtime_metadata":{"rayhunter_version":""}}"#).unwrap();
        let mut r = CheckReport { items: Vec::new() };
        assert!(
            !report_stats(&mut r, &stats),
            "empty version must not confirm a daemon"
        );
        // and no contradictory "[ok] rayhunter" line is emitted
        assert!(!r.items.iter().any(|(_, m)| m.contains("rayhunter")));
    }

    #[test]
    fn check_inner_fails_on_unparsable_stats() {
        let daemon = serve_stats("<html>not json</html>", r#"{"entries":[]}"#);
        let r = check_inner(daemon, closed_addr(), closed_addr(), Duration::from_secs(2));
        assert!(!r.healthy(), "unparsable system-stats must fail the check");
    }
}
