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
    /// Completed recordings eligible for deletion (the active one is excluded),
    /// each with its current manifest `qmdl_size_bytes` (compressed, on-device).
    pub completed: Vec<(String, u64)>,
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
        // Fail closed if the size is absent/invalid: the verified-pull gate needs
        // it to confirm the pulled copy is the current completed recording.
        let size = e
            .get("qmdl_size_bytes")
            .and_then(Json::as_u64)
            .ok_or_else(|| format!("manifest entry {name:?} has no valid qmdl_size_bytes"))?;
        completed.push((name.to_string(), size));
    }
    // A present current_entry must carry a safe string name -- fail closed on a
    // malformed one rather than silently treating it as "no active recording",
    // which could let the active recording be deleted.
    let active = match v.get("current_entry") {
        // Absent field: the manifest schema is not what we expect -- fail closed
        // rather than assume there is no active recording.
        None => {
            return Err("manifest has no `current_entry` field (unexpected response)".to_string());
        }
        Some(c) if c.is_null() => None,
        Some(c) => {
            let name = c
                .get("name")
                .and_then(Json::as_str)
                .ok_or_else(|| "current_entry has no string name".to_string())?;
            if !pull::safe_name(name) {
                return Err(format!("unsafe current_entry name: {name:?}"));
            }
            Some(name.to_string())
        }
    };
    // Belt and braces: never offer the active recording for deletion even if the
    // daemon ever lists it in `entries` too.
    if let Some(a) = &active {
        completed.retain(|(n, _)| n != a);
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
    // Load and validate the pull directory ONCE up front: a missing directory or
    // unreadable/absent `.orx-pull-state` / `PULL-MANIFEST.txt` is a hard error
    // (non-zero exit), never a silent per-recording skip that still exits 0.
    let pulled = match gate {
        PurgeGate::VerifiedPull(dir) => Some(PulledDir::load(dir)?),
        PurgeGate::Force => None,
    };
    let mut out = PurgeOutcome {
        deleted: 0,
        failed: 0,
        skipped: 0,
    };
    for (name, size) in &plan.completed {
        // Safety gate: under VerifiedPull, only delete a recording we hold a
        // hash-verified local copy of that corresponds to THIS completed
        // recording; skip (never delete) anything unverified.
        if let Some(pulled) = &pulled {
            let verified = pulled.verifies(name, *size)?;
            if !verified {
                eprintln!(
                    "skip {name}: no verified current pull in {} (pull it first)",
                    pulled.dir.display()
                );
                out.skipped += 1;
                continue;
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

/// A validated `orx pull-logs` directory: its `.orx-pull-state` (recording ->
/// pull-time compressed size) and `PULL-MANIFEST.txt` (filename -> SHA-256),
/// loaded once. Construction fails if the directory or either metadata file is
/// missing or unreadable, so an invalid `--pulled <dir>` fails closed rather
/// than silently skipping every recording.
struct PulledDir {
    dir: std::path::PathBuf,
    state: std::collections::BTreeMap<String, u64>,
    hashes: std::collections::BTreeMap<String, String>,
}

impl PulledDir {
    fn load(dir: &std::path::Path) -> Result<Self, String> {
        if !dir.is_dir() {
            return Err(format!("pull dir {} is not a directory", dir.display()));
        }
        let state_path = dir.join(".orx-pull-state");
        let state_text = std::fs::read_to_string(&state_path).map_err(|e| {
            format!(
                "read {}: {e} -- run `orx pull-logs --to {}` first",
                state_path.display(),
                dir.display()
            )
        })?;
        let mut state = std::collections::BTreeMap::new();
        for l in state_text.lines() {
            let Some((n, s)) = l.split_once('\t') else {
                continue;
            };
            let Ok(sz) = s.trim().parse::<u64>() else {
                continue;
            };
            state.insert(n.to_string(), sz);
        }
        let manifest_path = dir.join("PULL-MANIFEST.txt");
        let manifest_text = std::fs::read_to_string(&manifest_path)
            .map_err(|e| format!("read {}: {e}", manifest_path.display()))?;
        let mut hashes = std::collections::BTreeMap::new();
        for l in manifest_text.lines() {
            // Lines are "<sha256>  <filename>"; comment lines start with '#'.
            if let Some((h, f)) = l.split_once("  ") {
                hashes.insert(f.trim().to_string(), h.trim().to_string());
            }
        }
        Ok(PulledDir {
            dir: dir.to_path_buf(),
            state,
            hashes,
        })
    }

    /// Whether this dir holds a verified copy of the CURRENT completed recording
    /// `name` (current manifest size `current_size`). Both must hold:
    ///
    ///  1. `.orx-pull-state` records `name` at a size equal to `current_size`.
    ///     `orx pull-logs` records that state only for recordings pulled while
    ///     already COMPLETED, so this proves the copy is of a finished recording
    ///     and unchanged since (same compressed on-device size). A copy pulled
    ///     while the recording was still active has no such entry and is rejected.
    ///  2. `<name>.qmdl` exists and its SHA-256 matches `PULL-MANIFEST.txt`.
    ///
    /// (We cannot compare the local byte length to `qmdl_size_bytes`: the
    /// manifest size is COMPRESSED while the pulled file is decompressed.)
    fn verifies(&self, name: &str, current_size: u64) -> Result<bool, String> {
        let qmdl = self.dir.join(format!("{name}.qmdl"));
        if !qmdl.is_file() {
            return Ok(false);
        }
        if self.state.get(name) != Some(&current_size) {
            return Ok(false);
        }
        let actual = pull::file_sha256(&qmdl)?;
        Ok(self
            .hashes
            .get(&format!("{name}.qmdl"))
            .is_some_and(|h| h.eq_ignore_ascii_case(&actual)))
    }
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
            r#"{"entries":[{"name":"1795","qmdl_size_bytes":100},{"name":"1796","qmdl_size_bytes":200}],"current_entry":{"name":"1797","qmdl_size_bytes":5}}"#,
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

    /// Write a pull dir holding `<name>.qmdl` with the given bytes, a matching
    /// PULL-MANIFEST.txt, and a .orx-pull-state recording `name` at `pulled_size`.
    fn pulled_dir(tag: &str, name: &str, bytes: &[u8], pulled_size: u64) -> std::path::PathBuf {
        let dir = tmpdir(tag);
        std::fs::write(dir.join(format!("{name}.qmdl")), bytes).unwrap();
        let hash = crate::sha256::hex(bytes);
        std::fs::write(
            dir.join("PULL-MANIFEST.txt"),
            format!("{hash}  {name}.qmdl\n"),
        )
        .unwrap();
        std::fs::write(
            dir.join(".orx-pull-state"),
            format!("{name}\t{pulled_size}\n"),
        )
        .unwrap();
        dir
    }

    #[test]
    fn verified_pull_deletes_only_verified_and_skips_the_rest() {
        let (addr, deleted) = serve(
            r#"{"entries":[{"name":"1795","qmdl_size_bytes":100},{"name":"1796","qmdl_size_bytes":200}],"current_entry":null}"#,
        );
        // 1795 is pulled at the current size (100); 1796 is not pulled.
        let dir = pulled_dir("vp", "1795", b"AAAAA", 100);
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
        let (addr, deleted) =
            serve(r#"{"entries":[{"name":"1795","qmdl_size_bytes":100}],"current_entry":null}"#);
        let dir = pulled_dir("mm", "1795", b"AAAAA", 100);
        // Corrupt the manifest hash -> local integrity check fails -> skip.
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
    fn verified_pull_skips_when_recording_changed_since_pull() {
        // Current completed size (200) differs from the pulled size (100): the
        // local copy is a stale/partial pull -> must NOT delete the larger
        // completed recording. This is the silent-data-loss guard.
        let (addr, deleted) =
            serve(r#"{"entries":[{"name":"1795","qmdl_size_bytes":200}],"current_entry":null}"#);
        let dir = pulled_dir("changed", "1795", b"AAAAA", 100);
        let out = apply_at(
            addr,
            &PurgeGate::VerifiedPull(dir.clone()),
            Duration::from_secs(2),
        )
        .unwrap();
        assert_eq!(
            out.deleted, 0,
            "a recording changed since the pull must not be deleted"
        );
        assert_eq!(out.skipped, 1);
        assert!(deleted.lock().unwrap().is_empty());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn verified_pull_skips_without_pull_state() {
        // A dir with the file + manifest but no state entry (e.g. pulled while the
        // recording was still active) is not a verified completed pull -> skip.
        let (addr, _deleted) =
            serve(r#"{"entries":[{"name":"1795","qmdl_size_bytes":100}],"current_entry":null}"#);
        let dir = tmpdir("nostate");
        std::fs::write(dir.join("1795.qmdl"), b"AAAAA").unwrap();
        let hash = crate::sha256::hex(b"AAAAA");
        std::fs::write(
            dir.join("PULL-MANIFEST.txt"),
            format!("{hash}  1795.qmdl\n"),
        )
        .unwrap();
        std::fs::write(dir.join(".orx-pull-state"), "").unwrap(); // no entry for 1795
        let out = apply_at(
            addr,
            &PurgeGate::VerifiedPull(dir.clone()),
            Duration::from_secs(2),
        )
        .unwrap();
        assert_eq!(out.deleted, 0);
        assert_eq!(out.skipped, 1);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn apply_errors_on_missing_pull_dir() {
        let (addr, _d) =
            serve(r#"{"entries":[{"name":"1795","qmdl_size_bytes":100}],"current_entry":null}"#);
        let missing = std::env::temp_dir().join(format!("orx-purge-nope-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&missing);
        assert!(
            apply_at(
                addr,
                &PurgeGate::VerifiedPull(missing),
                Duration::from_secs(2)
            )
            .is_err(),
            "a nonexistent --pulled dir must be a hard error, not a silent skip"
        );
    }

    #[test]
    fn apply_errors_on_missing_state_file() {
        let (addr, _d) =
            serve(r#"{"entries":[{"name":"1795","qmdl_size_bytes":100}],"current_entry":null}"#);
        let dir = tmpdir("nostatefile");
        std::fs::write(dir.join("1795.qmdl"), b"AAAAA").unwrap();
        std::fs::write(dir.join("PULL-MANIFEST.txt"), "x  1795.qmdl\n").unwrap();
        // No .orx-pull-state at all -> hard error.
        assert!(
            apply_at(
                addr,
                &PurgeGate::VerifiedPull(dir.clone()),
                Duration::from_secs(2)
            )
            .is_err()
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn plan_fails_closed_without_entries() {
        let (addr, _) = serve("{}");
        assert!(plan(addr, Duration::from_secs(2)).is_err());
    }

    #[test]
    fn plan_fails_closed_on_absent_current_entry() {
        // The field is omitted entirely (not null) -> unexpected schema -> fail.
        let (addr, _) = serve(r#"{"entries":[]}"#);
        assert!(
            plan(addr, Duration::from_secs(2)).is_err(),
            "an absent current_entry field must fail closed"
        );
    }

    #[test]
    fn plan_fails_closed_without_size() {
        let (addr, _) = serve(r#"{"entries":[{"name":"1795"}],"current_entry":null}"#);
        assert!(
            plan(addr, Duration::from_secs(2)).is_err(),
            "an entry with no qmdl_size_bytes must fail closed"
        );
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
            r#"{"entries":[{"name":"1795","qmdl_size_bytes":100},{"name":"1797","qmdl_size_bytes":5}],"current_entry":{"name":"1797","qmdl_size_bytes":5}}"#,
        );
        let p = plan(addr, Duration::from_secs(2)).unwrap();
        assert_eq!(
            p.completed,
            vec![("1795".to_string(), 100)],
            "active 1797 excluded from completed"
        );
    }
}
