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
    let addr = probe::socket(ip, probe::RAYHUNTER_PORT)?;
    Ok(check_at(addr, timeout))
}

/// As [`check`] but against an explicit daemon address (tests use a loopback
/// server). Port probes reuse that address's IP with the fixed ports.
pub fn check_at(addr: SocketAddr, timeout: Duration) -> CheckReport {
    let mut r = CheckReport { items: Vec::new() };

    // 1. Daemon reachable + system stats.
    match crate::pull::http_get_string(addr, "/api/system-stats", 256 * 1024, timeout) {
        Ok(body) => match crate::json::parse(&body) {
            Ok(stats) => {
                r.push(Level::Ok, format!("daemon reachable on {addr}"));
                report_stats(&mut r, &stats);
            }
            Err(e) => r.push(
                Level::Warn,
                format!("daemon reachable but stats unparsable: {e}"),
            ),
        },
        Err(e) => {
            r.push(Level::Fail, format!("daemon NOT reachable on {addr}: {e}"));
            // Without the daemon the rest of the API checks are moot; still probe ports.
        }
    }

    // 2. Recording status.
    match crate::pull::http_get_string(addr, "/api/qmdl-manifest", 4 * 1024 * 1024, timeout) {
        Ok(body) => match crate::json::parse(&body) {
            Ok(m) => report_recording(&mut r, &m),
            Err(e) => r.push(Level::Warn, format!("manifest unparsable: {e}")),
        },
        Err(e) => r.push(Level::Warn, format!("manifest unavailable: {e}")),
    }

    // 3. Port posture. The daemon port should be open; the root shell and the
    // installer transfer port should be CLOSED on a settled device.
    let probe_port = |port: u16| probe::tcp_open(SocketAddr::new(addr.ip(), port), timeout);
    if probe_port(probe::RAYHUNTER_PORT) {
        r.push(
            Level::Ok,
            format!("daemon port {} open", probe::RAYHUNTER_PORT),
        );
    } else {
        r.push(
            Level::Fail,
            format!("daemon port {} closed", probe::RAYHUNTER_PORT),
        );
    }
    if probe_port(ROOTSHELL_PORT) {
        r.push(
            Level::Warn,
            format!("root shell port {ROOTSHELL_PORT} is OPEN (exposed; expected closed)"),
        );
    } else {
        r.push(
            Level::Ok,
            format!("root shell port {ROOTSHELL_PORT} closed"),
        );
    }
    if probe_port(INSTALLER_PORT) {
        r.push(
            Level::Warn,
            format!("installer port {INSTALLER_PORT} is OPEN (stale transfer listener?)"),
        );
    } else {
        r.push(Level::Ok, format!("installer port {INSTALLER_PORT} closed"));
    }

    r
}

fn report_stats(r: &mut CheckReport, stats: &Json) {
    if let Some(meta) = stats.get("runtime_metadata") {
        let ver = meta
            .get("rayhunter_version")
            .and_then(Json::as_str)
            .unwrap_or("?");
        let os = meta.get("system_os").and_then(Json::as_str).unwrap_or("?");
        let arch = meta.get("arch").and_then(Json::as_str).unwrap_or("?");
        r.push(Level::Ok, format!("rayhunter {ver} on {os} ({arch})"));
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
            format!("disk {used}/{total} used ({pct}), {avail} free"),
        );
    }
    if let Some(mem) = stats.get("memory_stats") {
        let used = mem.get("used").and_then(Json::as_str).unwrap_or("?");
        let total = mem.get("total").and_then(Json::as_str).unwrap_or("?");
        r.push(Level::Ok, format!("memory {used}/{total} used"));
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
}

fn report_recording(r: &mut CheckReport, manifest: &Json) {
    match manifest.get("current_entry") {
        Some(cur) if !cur.is_null() => {
            let name = cur.get("name").and_then(Json::as_str).unwrap_or("?");
            r.push(
                Level::Ok,
                format!("recording active (current entry {name})"),
            );
        }
        _ => r.push(Level::Warn, "no recording active".to_string()),
    }
    if let Some(entries) = manifest.get("entries").and_then(Json::as_array) {
        r.push(
            Level::Ok,
            format!("{} completed recording(s) stored", entries.len()),
        );
    }
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

    fn serve_stats(stats: &'static str, manifest: &'static str) -> SocketAddr {
        let l = TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = l.local_addr().unwrap();
        std::thread::spawn(move || {
            for _ in 0..2 {
                let Ok((mut s, _)) = l.accept() else { break };
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

    #[test]
    fn check_at_reports_daemon_up_and_ports() {
        let addr = serve_stats(
            r#"{"runtime_metadata":{"rayhunter_version":"0.13.0","system_os":"Linux","arch":"armv7l"}}"#,
            r#"{"entries":[],"current_entry":{"name":"9"}}"#,
        );
        let r = check_at(addr, Duration::from_secs(2));
        let text = r
            .items
            .iter()
            .map(|(l, m)| format!("{} {m}", l.tag()))
            .collect::<Vec<_>>()
            .join("\n");
        assert!(text.contains("daemon reachable"), "{text}");
        assert!(text.contains("rayhunter 0.13.0"), "{text}");
        assert!(text.contains("recording active"), "{text}");
        // The root shell / installer ports are not listening on loopback -> closed.
        assert!(
            text.contains(&format!("root shell port {ROOTSHELL_PORT} closed")),
            "{text}"
        );
    }
}
