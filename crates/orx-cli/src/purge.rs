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
    /// Completed recordings left untouched because they were not verified in the
    /// pull directory (safety gate), not a failure.
    pub skipped: usize,
}

/// How `--apply` is authorized to delete. A device write is irreversible, so it
/// must be gated: either every target is confirmed present and hash-verified in
/// a local pull directory, or the operator explicitly forces an unguarded purge.
pub enum PurgeGate {
    /// Delete only recordings whose `<name>.qmdl` is present in this directory
    /// and matches its `PULL-MANIFEST.txt` SHA-256 (the pulled copy is the undo).
    VerifiedPull(std::path::PathBuf),
    /// Explicit unguarded override: delete without checking a local copy exists.
    Force,
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
    // A present current_entry must carry a safe string name -- fail closed on a
    // malformed one rather than silently treating it as "no active recording",
    // which could let the active recording be deleted.
    let active = match v.get("current_entry") {
        Some(c) if !c.is_null() => {
            let name = c
                .get("name")
                .and_then(Json::as_str)
                .ok_or_else(|| "current_entry has no string name".to_string())?;
            if !pull::safe_name(name) {
                return Err(format!("unsafe current_entry name: {name:?}"));
            }
            Some(name.to_string())
        }
        _ => None,
    };
    // Belt and braces: never offer the active recording for deletion even if the
    // daemon ever lists it in `entries` too.
    if let Some(a) = &active {
        completed.retain(|n| n != a);
    }
    Ok(PurgePlan { completed, active })
}

/// Compute the purge plan without deleting anything (for the dry run).
pub fn dry_run(ip: &str, timeout: Duration) -> Result<PurgePlan, String> {
    let addr = crate::probe::socket(ip, crate::probe::RAYHUNTER_PORT)?;
    plan(addr, timeout)
}

/// Delete completed recordings (never the active one), subject to `gate`.
pub fn apply(ip: &str, gate: &PurgeGate, timeout: Duration) -> Result<PurgeOutcome, String> {
    let addr = crate::probe::socket(ip, crate::probe::RAYHUNTER_PORT)?;
    apply_at(addr, gate, timeout)
}

/// As [`apply`] but against an explicit address (tests use a loopback server).
pub fn apply_at(
    addr: SocketAddr,
    gate: &PurgeGate,
    timeout: Duration,
) -> Result<PurgeOutcome, String> {
    let plan = plan(addr, timeout)?;
    let mut out = PurgeOutcome {
        deleted: 0,
        failed: 0,
        skipped: 0,
    };
    for name in &plan.completed {
        // Safety gate: under VerifiedPull, only delete a recording we hold a
        // hash-verified local copy of; skip (never delete) anything unverified.
        if let PurgeGate::VerifiedPull(dir) = gate {
            match verify_pulled(dir, name) {
                Ok(true) => {}
                Ok(false) => {
                    eprintln!(
                        "skip {name}: not verified in pull dir {} (pull it first)",
                        dir.display()
                    );
                    out.skipped += 1;
                    continue;
                }
                Err(e) => {
                    eprintln!("skip {name}: pull verification failed: {e}");
                    out.skipped += 1;
                    continue;
                }
            }
        }
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

/// Whether `<dir>/<name>.qmdl` exists and its SHA-256 matches the hash recorded
/// for it in `<dir>/PULL-MANIFEST.txt` -- i.e. a verified pulled copy exists.
fn verify_pulled(dir: &std::path::Path, name: &str) -> Result<bool, String> {
    let qmdl = dir.join(format!("{name}.qmdl"));
    if !qmdl.is_file() {
        return Ok(false);
    }
    let actual = pull::file_sha256(&qmdl)?;
    let manifest = dir.join("PULL-MANIFEST.txt");
    let text = std::fs::read_to_string(&manifest)
        .map_err(|e| format!("read {}: {e}", manifest.display()))?;
    let target = format!("{name}.qmdl");
    for line in text.lines() {
        // Lines are "<sha256>  <filename>"; comment lines start with '#'.
        let Some((hash, file)) = line.split_once("  ") else {
            continue;
        };
        if file.trim() == target {
            return Ok(hash.trim().eq_ignore_ascii_case(&actual));
        }
    }
    Ok(false)
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
    fn force_deletes_completed_only() {
        let (addr, deleted) = serve(
            r#"{"entries":[{"name":"1795"},{"name":"1796"}],"current_entry":{"name":"1797"}}"#,
        );
        let out = apply_at(addr, &PurgeGate::Force, Duration::from_secs(2)).unwrap();
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

    fn tmpdir(tag: &str) -> std::path::PathBuf {
        let d = std::env::temp_dir().join(format!("orx-purge-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    #[test]
    fn verified_pull_deletes_only_verified_and_skips_the_rest() {
        let (addr, deleted) =
            serve(r#"{"entries":[{"name":"1795"},{"name":"1796"}],"current_entry":null}"#);
        let dir = tmpdir("vp");
        std::fs::write(dir.join("1795.qmdl"), b"AAAAA").unwrap();
        let hash = crate::sha256::hex(b"AAAAA");
        std::fs::write(
            dir.join("PULL-MANIFEST.txt"),
            format!("{hash}  1795.qmdl\n"),
        )
        .unwrap();

        let out = apply_at(
            addr,
            &PurgeGate::VerifiedPull(dir.clone()),
            Duration::from_secs(2),
        )
        .unwrap();
        assert_eq!(out.deleted, 1, "only the verified 1795 is deleted");
        assert_eq!(out.skipped, 1, "1796 (not pulled) is skipped, not deleted");
        let paths = deleted.lock().unwrap();
        assert!(paths.contains(&"/api/delete-recording/1795".to_string()));
        assert!(
            !paths.iter().any(|p| p.contains("1796")),
            "unverified deleted: {paths:?}"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn verified_pull_skips_on_hash_mismatch() {
        let (addr, deleted) = serve(r#"{"entries":[{"name":"1795"}],"current_entry":null}"#);
        let dir = tmpdir("mm");
        std::fs::write(dir.join("1795.qmdl"), b"AAAAA").unwrap();
        // Manifest records a DIFFERENT hash -> not verified -> skipped.
        std::fs::write(dir.join("PULL-MANIFEST.txt"), "deadbeef  1795.qmdl\n").unwrap();
        let out = apply_at(
            addr,
            &PurgeGate::VerifiedPull(dir.clone()),
            Duration::from_secs(2),
        )
        .unwrap();
        assert_eq!(out.deleted, 0);
        assert_eq!(out.skipped, 1);
        assert!(deleted.lock().unwrap().is_empty());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn plan_fails_closed_without_entries() {
        let (addr, _) = serve("{}");
        assert!(plan(addr, Duration::from_secs(2)).is_err());
    }

    #[test]
    fn plan_fails_closed_on_malformed_current_entry() {
        let (addr, _) = serve(r#"{"entries":[],"current_entry":{"foo":1}}"#);
        assert!(
            plan(addr, Duration::from_secs(2)).is_err(),
            "current_entry with no name must fail"
        );
    }

    #[test]
    fn plan_excludes_active_even_if_in_entries() {
        let (addr, _) = serve(
            r#"{"entries":[{"name":"1795"},{"name":"1797"}],"current_entry":{"name":"1797"}}"#,
        );
        let p = plan(addr, Duration::from_secs(2)).unwrap();
        assert_eq!(
            p.completed,
            vec!["1795".to_string()],
            "active 1797 excluded from completed"
        );
    }
}
