//! `orx pull-logs`: download recordings from the device's HTTP API to a host
//! directory, with a manifest and SHA-256, skipping entries already pulled
//! unchanged. Per recording it fetches the raw QMDL (`/api/qmdl/{name}`) and the
//! analysis report (`/api/analysis-report/{name}`, NDJSON, best-effort). The
//! device API is unauthenticated on `0.0.0.0:8080`; this is a read-only pull.

use std::io::{Read, Write};
use std::net::{SocketAddr, TcpStream};
use std::path::Path;
use std::time::{Duration, Instant};

/// Recording names are device-supplied; only allow a safe, path- and URL-clean
/// token so a crafted manifest cannot traverse directories or inject a request.
pub fn safe_name(name: &str) -> bool {
    !name.is_empty()
        && name.len() <= 128
        && name
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_' || b == b'.')
        && name != "."
        && name != ".."
        && !name.contains("..")
}

/// GET `path` and return the body as a String, capped at `max` bytes (for small
/// JSON responses). A non-2xx status or exceeding the cap is an error.
pub fn http_get_string(
    addr: SocketAddr,
    path: &str,
    max: usize,
    timeout: Duration,
) -> Result<String, String> {
    let mut buf = Vec::new();
    let n = http_get_to(addr, path, &mut buf, max as u64, timeout)?;
    String::from_utf8(buf[..n as usize].to_vec()).map_err(|_| "response is not UTF-8".to_string())
}

/// GET `path`, streaming the response body into `sink` (never buffering it all),
/// bounded by `max` bytes and `timeout`. Returns the number of body bytes
/// written. A non-2xx status is an error; exceeding `max` is an error.
pub fn http_get_to(
    addr: SocketAddr,
    path: &str,
    sink: &mut dyn Write,
    max: u64,
    timeout: Duration,
) -> Result<u64, String> {
    let deadline = Instant::now() + timeout;
    let mut stream =
        TcpStream::connect_timeout(&addr, timeout).map_err(|e| format!("connect: {e}"))?;
    stream.set_write_timeout(Some(timeout)).ok();
    let req =
        format!("GET {path} HTTP/1.0\r\nHost: {addr}\r\nConnection: close\r\nAccept: */*\r\n\r\n");
    stream
        .write_all(req.as_bytes())
        .map_err(|e| format!("write: {e}"))?;

    // Read until we have the full header block, then stream the body.
    let mut head = Vec::new();
    let mut buf = [0u8; 8192];
    let body_start: Vec<u8> = loop {
        let remaining = deadline.saturating_duration_since(Instant::now());
        if remaining.is_zero() {
            return Err("read: deadline exceeded in headers".to_string());
        }
        stream.set_read_timeout(Some(remaining)).ok();
        match stream.read(&mut buf) {
            Ok(0) => return Err("connection closed before headers completed".to_string()),
            Ok(n) => {
                head.extend_from_slice(&buf[..n]);
                if let Some(pos) = find_sub(&head, b"\r\n\r\n") {
                    let header_end = pos + 4;
                    let body = head[header_end..].to_vec();
                    head.truncate(header_end);
                    break body;
                }
                if head.len() > 64 * 1024 {
                    return Err("HTTP headers too large".to_string());
                }
            }
            Err(e) => return Err(format!("read: {e}")),
        }
    };
    let status = std::str::from_utf8(&head)
        .ok()
        .and_then(|h| h.lines().next())
        .and_then(|l| l.split_whitespace().nth(1))
        .and_then(|c| c.parse::<u16>().ok())
        .ok_or_else(|| "malformed HTTP status line".to_string())?;
    if !(200..300).contains(&status) {
        return Err(format!("HTTP {status}"));
    }

    let mut written = 0u64;
    let write_chunk =
        |sink: &mut dyn Write, data: &[u8], written: &mut u64| -> Result<(), String> {
            *written += data.len() as u64;
            if *written > max {
                return Err(format!("response exceeded {max} bytes"));
            }
            sink.write_all(data).map_err(|e| format!("write body: {e}"))
        };
    write_chunk(sink, &body_start, &mut written)?;
    loop {
        let remaining = deadline.saturating_duration_since(Instant::now());
        if remaining.is_zero() {
            return Err("read: deadline exceeded in body".to_string());
        }
        stream.set_read_timeout(Some(remaining)).ok();
        match stream.read(&mut buf) {
            Ok(0) => break,
            Ok(n) => write_chunk(sink, &buf[..n], &mut written)?,
            Err(e) => return Err(format!("read body: {e}")),
        }
    }
    Ok(written)
}

fn find_sub(hay: &[u8], needle: &[u8]) -> Option<usize> {
    if needle.is_empty() || hay.len() < needle.len() {
        return None;
    }
    (0..=hay.len() - needle.len()).find(|&i| &hay[i..i + needle.len()] == needle)
}

/// One recording to pull, parsed from the manifest.
struct Recording {
    name: String,
    qmdl_size: u64,
    completed: bool,
}

/// Parse `/api/qmdl-manifest` into the recordings to pull: the completed
/// `entries` plus the live `current_entry` if present.
fn parse_manifest(body: &str) -> Result<Vec<Recording>, String> {
    let v = crate::json::parse(body).map_err(|e| format!("manifest JSON: {e}"))?;
    let mut out = Vec::new();
    // `completed` is determined by POSITION, not `stop_reason`: the daemon keeps
    // finished recordings in `entries` and the live one in `current_entry`, and
    // some daemon versions leave `stop_reason` null on finished entries too. A
    // recording in `entries` is final, so its `qmdl_size_bytes` is stable and the
    // skip-if-unchanged check is sound.
    let mut take = |entry: &crate::json::Json, completed: bool| -> Result<(), String> {
        let name = entry
            .get("name")
            .and_then(|n| n.as_str())
            .ok_or_else(|| "manifest entry missing name".to_string())?
            .to_string();
        if !safe_name(&name) {
            return Err(format!("unsafe recording name in manifest: {name:?}"));
        }
        let qmdl_size = entry
            .get("qmdl_size_bytes")
            .and_then(|s| s.as_u64())
            .unwrap_or(0);
        out.push(Recording {
            name,
            qmdl_size,
            completed,
        });
        Ok(())
    };
    if let Some(entries) = v.get("entries").and_then(|e| e.as_array()) {
        for e in entries {
            take(e, true)?;
        }
    }
    if let Some(cur) = v.get("current_entry").filter(|c| !c.is_null()) {
        take(cur, false)?;
    }
    Ok(out)
}

/// sha256 + byte count of a local file, for the host manifest.
fn file_digest(path: &Path) -> Result<(String, u64), String> {
    let bytes = std::fs::read(path).map_err(|e| format!("read {}: {e}", path.display()))?;
    Ok((crate::sha256::hex(&bytes), bytes.len() as u64))
}

pub struct PullOutcome {
    pub pulled: usize,
    pub skipped: usize,
    pub bytes: u64,
}

/// Download all recordings to `dir`. `completed` recordings already present with
/// the manifest's size are skipped; the live recording is always re-pulled.
pub fn pull_logs(ip: &str, dir: &Path, timeout: Duration) -> Result<PullOutcome, String> {
    let addr = crate::probe::socket(ip, crate::probe::RAYHUNTER_PORT)?;
    pull_logs_at(addr, dir, timeout)
}

/// As [`pull_logs`] but against an explicit address (so tests can use a loopback
/// server on an arbitrary port).
pub fn pull_logs_at(
    addr: SocketAddr,
    dir: &Path,
    timeout: Duration,
) -> Result<PullOutcome, String> {
    const MAX_MANIFEST: usize = 4 * 1024 * 1024;
    const MAX_QMDL: u64 = 512 * 1024 * 1024;
    std::fs::create_dir_all(dir).map_err(|e| format!("create {}: {e}", dir.display()))?;

    let body = http_get_string(addr, "/api/qmdl-manifest", MAX_MANIFEST, timeout)
        .map_err(|e| format!("fetch manifest: {e}"))?;
    let recordings = parse_manifest(&body)?;

    // Prior pull state: recording name -> the manifest `qmdl_size_bytes` it was
    // last pulled at. The on-device size is COMPRESSED and differs from the
    // decompressed bytes we download, so we skip by "manifest size unchanged
    // since last pull", not by comparing file lengths.
    let state_path = dir.join(".orx-pull-state");
    let prior = read_pull_state(&state_path);

    let mut outcome = PullOutcome {
        pulled: 0,
        skipped: 0,
        bytes: 0,
    };
    let mut manifest_lines = Vec::new();
    let mut new_state: Vec<(String, u64)> = Vec::new();
    for r in &recordings {
        let qmdl = dir.join(format!("{}.qmdl", r.name));
        new_state.push((r.name.clone(), r.qmdl_size));
        let up_to_date =
            r.completed && prior.get(r.name.as_str()) == Some(&r.qmdl_size) && qmdl.is_file();
        if up_to_date {
            outcome.skipped += 1;
        } else {
            let tmp = dir.join(format!("{}.qmdl.part", r.name));
            let mut f = std::fs::File::create(&tmp)
                .map_err(|e| format!("create {}: {e}", tmp.display()))?;
            let n = http_get_to(
                addr,
                &format!("/api/qmdl/{}", r.name),
                &mut f,
                MAX_QMDL,
                timeout,
            )
            .map_err(|e| format!("download {}.qmdl: {e}", r.name))?;
            drop(f);
            std::fs::rename(&tmp, &qmdl).map_err(|e| format!("rename {}: {e}", tmp.display()))?;
            outcome.pulled += 1;
            outcome.bytes += n;
            // Analysis report (NDJSON) is best-effort: it may not exist yet.
            let nd = dir.join(format!("{}.ndjson", r.name));
            let ndtmp = dir.join(format!("{}.ndjson.part", r.name));
            if let Ok(mut nf) = std::fs::File::create(&ndtmp) {
                match http_get_to(
                    addr,
                    &format!("/api/analysis-report/{}", r.name),
                    &mut nf,
                    MAX_QMDL,
                    timeout,
                ) {
                    Ok(nb) => {
                        drop(nf);
                        let _ = std::fs::rename(&ndtmp, &nd);
                        outcome.bytes += nb;
                    }
                    Err(_) => {
                        drop(nf);
                        let _ = std::fs::remove_file(&ndtmp);
                    }
                }
            }
        }
        // Record digests of whatever is on disk for this recording.
        for suffix in ["qmdl", "ndjson"] {
            let p = dir.join(format!("{}.{suffix}", r.name));
            if p.is_file() {
                let (hash, _) = file_digest(&p)?;
                manifest_lines.push(format!("{hash}  {}.{suffix}", r.name));
            }
        }
    }
    let manifest = format!(
        "# orx pull-logs manifest\n# pulled {} recording file set(s)\n{}\n",
        recordings.len(),
        manifest_lines.join("\n")
    );
    std::fs::write(dir.join("PULL-MANIFEST.txt"), manifest)
        .map_err(|e| format!("write manifest: {e}"))?;
    write_pull_state(&state_path, &new_state)?;
    Ok(outcome)
}

/// Read `name<TAB>qmdl_size_bytes` lines into a map (missing/garbled -> empty).
fn read_pull_state(path: &Path) -> std::collections::BTreeMap<String, u64> {
    let mut map = std::collections::BTreeMap::new();
    if let Ok(text) = std::fs::read_to_string(path) {
        for line in text.lines() {
            let Some((name, size)) = line.split_once('\t') else {
                continue;
            };
            let Ok(n) = size.trim().parse::<u64>() else {
                continue;
            };
            map.insert(name.to_string(), n);
        }
    }
    map
}

fn write_pull_state(path: &Path, state: &[(String, u64)]) -> Result<(), String> {
    let text: String = state.iter().map(|(n, s)| format!("{n}\t{s}\n")).collect();
    std::fs::write(path, text).map_err(|e| format!("write pull state: {e}"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::TcpListener;

    #[test]
    fn safe_name_rejects_traversal_and_junk() {
        assert!(safe_name("1795442375"));
        assert!(safe_name("rec-1_v2.qmdl"));
        assert!(!safe_name(""));
        assert!(!safe_name(".."));
        assert!(!safe_name("a/b"));
        assert!(!safe_name("../etc"));
        assert!(!safe_name("a b"));
    }

    #[test]
    fn parse_manifest_collects_entries_and_current() {
        let body = r#"{"entries":[{"name":"1795","qmdl_size_bytes":100,"stop_reason":"UserRequested"}],"current_entry":{"name":"1796","qmdl_size_bytes":200,"stop_reason":null}}"#;
        let r = parse_manifest(body).unwrap();
        assert_eq!(r.len(), 2);
        assert_eq!(r[0].name, "1795");
        assert!(r[0].completed);
        assert_eq!(r[1].name, "1796");
        assert!(!r[1].completed);
    }

    #[test]
    fn parse_manifest_rejects_unsafe_name() {
        let body =
            r#"{"entries":[{"name":"../../etc/passwd","qmdl_size_bytes":1,"stop_reason":"x"}]}"#;
        assert!(parse_manifest(body).is_err());
    }

    /// Serve canned HTTP/1.0 responses keyed by request path, for `count` conns.
    fn serve(count: usize, routes: Vec<(&'static str, &'static str)>) -> SocketAddr {
        let l = TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = l.local_addr().unwrap();
        std::thread::spawn(move || {
            for _ in 0..count {
                let (mut s, _) = match l.accept() {
                    Ok(x) => x,
                    Err(_) => break,
                };
                let mut req = [0u8; 2048];
                let n = s.read(&mut req).unwrap_or(0);
                let line = String::from_utf8_lossy(&req[..n]);
                let path = line.split_whitespace().nth(1).unwrap_or("");
                let body = routes.iter().find(|(p, _)| *p == path).map(|(_, b)| *b);
                let resp = match body {
                    Some(b) => format!(
                        "HTTP/1.0 200 OK\r\nContent-Type: application/octet-stream\r\n\r\n{b}"
                    ),
                    None => "HTTP/1.0 404 Not Found\r\n\r\n".to_string(),
                };
                let _ = s.write_all(resp.as_bytes());
            }
        });
        addr
    }

    #[test]
    fn pull_logs_downloads_then_skips_completed() {
        let manifest = r#"{"entries":[{"name":"1795","qmdl_size_bytes":5,"stop_reason":"UserRequested"}],"current_entry":{"name":"1796","qmdl_size_bytes":4,"stop_reason":null}}"#;
        let routes = vec![
            ("/api/qmdl-manifest", manifest),
            ("/api/qmdl/1795", "AAAAA"),
            ("/api/analysis-report/1795", "{\"report_version\":2}\n"),
            ("/api/qmdl/1796", "BBBB"),
            ("/api/analysis-report/1796", "{\"report_version\":2}\n"),
        ];
        // Run 1: manifest + both recordings' qmdl+ndjson = 5 connections.
        // Run 2: manifest + only the live 1796's qmdl+ndjson (1795 is skipped) = 3.
        let addr = serve(8, routes);
        let dir = tmp();

        let o1 = pull_logs_at(addr, &dir, Duration::from_secs(3)).unwrap();
        assert_eq!(o1.pulled, 2);
        assert_eq!(o1.skipped, 0);
        assert_eq!(std::fs::read(dir.join("1795.qmdl")).unwrap(), b"AAAAA");
        assert!(dir.join("1795.ndjson").is_file());
        assert!(dir.join("PULL-MANIFEST.txt").is_file());

        let o2 = pull_logs_at(addr, &dir, Duration::from_secs(3)).unwrap();
        assert_eq!(o2.skipped, 1, "completed 1795 should be skipped");
        assert_eq!(o2.pulled, 1, "live 1796 is always re-pulled");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn http_get_to_errors_on_404() {
        let addr = serve(1, vec![("/present", "x")]);
        let mut e = Vec::new();
        assert!(http_get_to(addr, "/missing", &mut e, 1 << 20, Duration::from_secs(2)).is_err());
    }

    #[test]
    fn http_get_to_enforces_cap() {
        let addr = serve(1, vec![("/big", "0123456789")]);
        let mut buf = Vec::new();
        assert!(http_get_to(addr, "/big", &mut buf, 4, Duration::from_secs(2)).is_err());
    }

    use std::sync::atomic::{AtomicUsize, Ordering};
    static N: AtomicUsize = AtomicUsize::new(0);
    fn tmp() -> std::path::PathBuf {
        let p = std::env::temp_dir().join(format!(
            "orx-pull-{}-{}",
            std::process::id(),
            N.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir_all(&p).unwrap();
        p
    }
}
