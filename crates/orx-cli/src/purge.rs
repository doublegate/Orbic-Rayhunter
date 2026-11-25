//! `orx purge`: delete COMPLETED recordings from the device via the daemon API
//! (`POST /api/delete-recording/{name}`). It never deletes the active recording.
//! It is dry-run by default (lists what would be deleted); `--apply` performs
//! the deletions. Captures hold real identifiers (IMSI/IMEI); pull them with
//! `orx pull-logs` before purging if you want to keep them.

use crate::json::Json;
use crate::pull::{self, PullError};
use std::net::SocketAddr;
use std::time::Duration;

const MAX_MANIFEST: usize = 4 * 1024 * 1024;

pub struct PurgePlan {
    /// Completed recordings eligible for deletion (the active one is excluded).
    pub completed: Vec<String>,
    /// The active recording's name, if any (never deleted).
    pub active: Option<String>,
}

pub struct PurgeOutcome {
    pub deleted: usize,
    pub failed: usize,
}

/// Fetch the manifest and derive the purge plan (completed names + active name).
fn plan(addr: SocketAddr, timeout: Duration) -> Result<PurgePlan, String> {
    let body = pull::http_get_string(addr, "/api/qmdl-manifest", MAX_MANIFEST, timeout)
        .map_err(|e| format!("fetch manifest: {e}"))?;
    let v = crate::json::parse(&body).map_err(|e| format!("manifest JSON: {e}"))?;
    let entries = v
        .get("entries")
        .and_then(Json::as_array)
        .ok_or_else(|| "manifest has no `entries` array (unexpected response)".to_string())?;
    let mut completed = Vec::new();
    for e in entries {
        let name = e
            .get("name")
            .and_then(Json::as_str)
            .ok_or_else(|| "manifest entry missing name".to_string())?;
        if !pull::safe_name(name) {
            return Err(format!("unsafe recording name in manifest: {name:?}"));
        }
        completed.push(name.to_string());
    }
    let active = v
        .get("current_entry")
        .filter(|c| !c.is_null())
        .and_then(|c| c.get("name"))
        .and_then(Json::as_str)
        .filter(|n| pull::safe_name(n))
        .map(str::to_string);
    Ok(PurgePlan { completed, active })
}

/// Compute the purge plan without deleting anything (for the dry run).
pub fn dry_run(ip: &str, timeout: Duration) -> Result<PurgePlan, String> {
    let addr = crate::probe::socket(ip, crate::probe::RAYHUNTER_PORT)?;
    plan(addr, timeout)
}

/// Delete every completed recording (the active one is always kept).
pub fn apply(ip: &str, timeout: Duration) -> Result<PurgeOutcome, String> {
    let addr = crate::probe::socket(ip, crate::probe::RAYHUNTER_PORT)?;
    apply_at(addr, timeout)
}

/// As [`apply`] but against an explicit address (tests use a loopback server).
pub fn apply_at(addr: SocketAddr, timeout: Duration) -> Result<PurgeOutcome, String> {
    let plan = plan(addr, timeout)?;
    let mut out = PurgeOutcome {
        deleted: 0,
        failed: 0,
    };
    for name in &plan.completed {
        // `safe_name` was already enforced in `plan`, so the path is clean.
        let path = format!("/api/delete-recording/{name}");
        match pull::http_post(addr, &path, timeout) {
            Ok(_) => {
                println!("deleted {name}");
                out.deleted += 1;
            }
            Err(PullError::Http(403)) => {
                return Err(
                    "daemon refused the delete (403): it is in debug mode -- disable debug_mode \
                     in the config and retry"
                        .to_string(),
                );
            }
            Err(e) => {
                eprintln!("failed to delete {name}: {e}");
                out.failed += 1;
            }
        }
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{Read, Write};
    use std::net::TcpListener;
    use std::sync::{Arc, Mutex};

    #[test]
    fn plan_lists_completed_excludes_active() {
        let body = r#"{"entries":[{"name":"1"},{"name":"2"}],"current_entry":{"name":"3"}}"#;
        let v = crate::json::parse(body).unwrap();
        // Exercise the same extraction plan() does.
        let entries = v.get("entries").and_then(Json::as_array).unwrap();
        assert_eq!(entries.len(), 2);
        let active = v
            .get("current_entry")
            .and_then(|c| c.get("name"))
            .and_then(Json::as_str);
        assert_eq!(active, Some("3"));
    }

    /// Loopback daemon: serves the manifest to GET, records POST delete paths,
    /// and answers them 202. Returns (addr, deleted-paths handle).
    fn serve(manifest: &'static str) -> (SocketAddr, Arc<Mutex<Vec<String>>>) {
        let l = TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = l.local_addr().unwrap();
        let deleted = Arc::new(Mutex::new(Vec::new()));
        let d2 = Arc::clone(&deleted);
        std::thread::spawn(move || {
            while let Ok((mut s, _)) = l.accept() {
                let mut req = [0u8; 2048];
                let n = s.read(&mut req).unwrap_or(0);
                let line = String::from_utf8_lossy(&req[..n]);
                let mut parts = line.split_whitespace();
                let method = parts.next().unwrap_or("");
                let path = parts.next().unwrap_or("");
                let resp = if method == "POST" {
                    d2.lock().unwrap().push(path.to_string());
                    "HTTP/1.1 202 Accepted\r\nContent-Length: 2\r\n\r\nok".to_string()
                } else {
                    format!(
                        "HTTP/1.1 200 OK\r\nContent-Length: {}\r\n\r\n{manifest}",
                        manifest.len()
                    )
                };
                let _ = s.write_all(resp.as_bytes());
            }
        });
        (addr, deleted)
    }

    #[test]
    fn apply_deletes_completed_only() {
        let (addr, deleted) = serve(
            r#"{"entries":[{"name":"1795"},{"name":"1796"}],"current_entry":{"name":"1797"}}"#,
        );
        let out = apply_at(addr, Duration::from_secs(2)).unwrap();
        assert_eq!(out.deleted, 2);
        assert_eq!(out.failed, 0);
        let paths = deleted.lock().unwrap();
        assert!(paths.contains(&"/api/delete-recording/1795".to_string()));
        assert!(paths.contains(&"/api/delete-recording/1796".to_string()));
        // The active recording must never be deleted.
        assert!(
            !paths.iter().any(|p| p.contains("1797")),
            "active deleted: {paths:?}"
        );
    }

    #[test]
    fn plan_fails_closed_without_entries() {
        let (addr, _) = serve("{}");
        assert!(plan(addr, Duration::from_secs(2)).is_err());
    }
}
