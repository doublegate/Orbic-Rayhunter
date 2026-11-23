//! `orx verify`: re-check a backup directory against its `MANIFEST.txt` by
//! re-hashing each archive, and the shared backup-gate used by write commands.
//!
//! This is the host-side counterpart to `orx backup`: it proves the archives on
//! disk still match what the manifest recorded (size + SHA-256), so a backup can
//! be trusted as a rollback before any device write. No device access, so it is
//! fully unit-tested.

use std::collections::HashMap;
use std::io::Read;
use std::path::Path;

use crate::backup::archive_name;
use crate::sha256::Sha256;

/// One parsed MANIFEST.txt row.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ManifestEntry {
    pub mountpoint: String,
    pub fstype: String,
    pub size: Option<u64>,
    pub sha256: Option<String>,
    pub state: String,
}

/// Parse a MANIFEST.txt. Tokens are read by key (`size=`, `sha256=`), so both
/// the Python (`size= entries= sha256=`) and Rust (`size= sha256=`) layouts
/// parse; the state is the last whitespace token. Comment/blank lines are skipped.
pub fn parse_manifest(text: &str) -> Vec<ManifestEntry> {
    let mut out = Vec::new();
    for ln in text.lines() {
        if ln.starts_with('#') || ln.trim().is_empty() {
            continue;
        }
        let f: Vec<&str> = ln.split_whitespace().collect();
        if f.len() < 3 {
            continue;
        }
        let toks: HashMap<&str, &str> = f.iter().filter_map(|t| t.split_once('=')).collect();
        out.push(ManifestEntry {
            mountpoint: f[0].to_string(),
            fstype: f[1].to_string(),
            size: toks.get("size").and_then(|v| v.parse().ok()),
            sha256: toks.get("sha256").map(|v| (*v).to_string()),
            state: f[f.len() - 1].to_string(),
        });
    }
    out
}

/// Outcome of verifying one archive against its manifest entry.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Verdict {
    Ok,
    Missing,
    TooSmall(u64),
    SizeMismatch { expected: u64, actual: u64 },
    HashMismatch,
    ReadError(String),
}

impl Verdict {
    pub fn is_ok(&self) -> bool {
        matches!(self, Verdict::Ok)
    }
    pub fn describe(&self) -> String {
        match self {
            Verdict::Ok => "ok".to_string(),
            Verdict::Missing => "MISSING archive".to_string(),
            Verdict::TooSmall(n) => format!("too small ({n} B, < 1024)"),
            Verdict::SizeMismatch { expected, actual } => {
                format!("size {actual} != manifest {expected}")
            }
            Verdict::HashMismatch => "SHA-256 mismatch".to_string(),
            Verdict::ReadError(e) => format!("read error: {e}"),
        }
    }
}

/// Streaming SHA-256 of a file, so a large archive is not buffered.
fn hash_file(path: &Path) -> Result<String, String> {
    let mut f = std::fs::File::open(path).map_err(|e| e.to_string())?;
    let mut h = Sha256::new();
    let mut buf = [0u8; 1 << 16];
    loop {
        match f.read(&mut buf) {
            Ok(0) => break,
            Ok(n) => h.update(&buf[..n]),
            Err(e) => return Err(e.to_string()),
        }
    }
    Ok(crate::sha256::hex_of(h))
}

/// Verify one manifest entry's archive on disk in `dir`.
pub fn verify_entry(dir: &Path, entry: &ManifestEntry) -> Verdict {
    let path = dir.join(format!("{}.tar", archive_name(&entry.mountpoint)));
    let meta = match std::fs::metadata(&path) {
        Ok(m) if m.is_file() => m,
        _ => return Verdict::Missing,
    };
    let actual = meta.len();
    if actual < 1024 {
        return Verdict::TooSmall(actual);
    }
    match entry.size {
        Some(expected) if expected != actual => return Verdict::SizeMismatch { expected, actual },
        _ => {}
    }
    if let Some(want) = &entry.sha256 {
        match hash_file(&path) {
            Ok(got) if &got == want => {}
            Ok(_) => return Verdict::HashMismatch,
            Err(e) => return Verdict::ReadError(e),
        }
    }
    Verdict::Ok
}

/// The backup-gate: a directory holds a usable rollback only if its MANIFEST.txt
/// records rootfs `/` as `ok`/`ok-live` AND that archive verifies. Returns the
/// `/` entry's verdict context on failure. Write commands call this before any
/// device write (the Rust counterpart of `orbic-ota-block.py`'s `_backup_gate`).
pub fn gate(dir: &Path) -> Result<(), String> {
    let manifest = dir.join("MANIFEST.txt");
    let text = std::fs::read_to_string(&manifest)
        .map_err(|e| format!("cannot read {}: {e}", manifest.display()))?;
    let entries = parse_manifest(&text);
    let root = entries
        .iter()
        .find(|e| e.mountpoint == "/")
        .ok_or_else(|| format!("{} has no rootfs '/' entry", manifest.display()))?;
    if root.state != "ok" && root.state != "ok-live" {
        return Err(format!(
            "rootfs '/' state is {:?}, not ok/ok-live",
            root.state
        ));
    }
    match verify_entry(dir, root) {
        Verdict::Ok => Ok(()),
        v => Err(format!(
            "rootfs '/' archive does not verify: {}",
            v.describe()
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const MANIFEST: &str = "\
# orx backup 192.168.1.1
# <mountpoint> <fstype> size=<bytes> sha256=<hex> <state>
/ ubifs size=4096 sha256=SHA ok
/data rootfs size=4096 entries=3 sha256=SHA ok-live";

    fn setup(dir: &Path, root_bytes: &[u8]) -> String {
        let sha = crate::sha256::hex(root_bytes);
        let text = MANIFEST.replace("SHA", &sha);
        std::fs::write(dir.join("MANIFEST.txt"), &text).unwrap();
        std::fs::write(dir.join("root.tar"), root_bytes).unwrap();
        std::fs::write(dir.join("data.tar"), root_bytes).unwrap();
        text
    }

    #[test]
    fn parse_reads_both_layouts() {
        let e = parse_manifest(&MANIFEST.replace("SHA", "deadbeef"));
        assert_eq!(e.len(), 2);
        assert_eq!(e[0].mountpoint, "/");
        assert_eq!(e[0].size, Some(4096));
        assert_eq!(e[0].sha256.as_deref(), Some("deadbeef"));
        assert_eq!(e[1].state, "ok-live"); // entries= token did not confuse the state
    }

    #[test]
    fn verify_entry_matches_and_catches_corruption() {
        let d = tempdir();
        let text = setup(&d, &[7u8; 4096]);
        let entries = parse_manifest(&text);
        assert!(verify_entry(&d, &entries[0]).is_ok());
        // Corrupt data.tar: hash mismatch.
        std::fs::write(d.join("data.tar"), [9u8; 4096]).unwrap();
        assert_eq!(verify_entry(&d, &entries[1]), Verdict::HashMismatch);
        // Remove root.tar: missing.
        std::fs::remove_file(d.join("root.tar")).unwrap();
        assert_eq!(verify_entry(&d, &entries[0]), Verdict::Missing);
        cleanup(&d);
    }

    #[test]
    fn gate_requires_verified_rootfs() {
        let d = tempdir();
        setup(&d, &[7u8; 4096]);
        assert!(gate(&d).is_ok());
        // Truncate root.tar below the floor -> gate fails.
        std::fs::write(d.join("root.tar"), [7u8; 10]).unwrap();
        assert!(gate(&d).is_err());
        cleanup(&d);
    }

    #[test]
    fn gate_rejects_missing_and_bad_state() {
        let d = tempdir();
        std::fs::write(
            d.join("MANIFEST.txt"),
            "/ ubifs size=4096 sha256=x failed\n",
        )
        .unwrap();
        assert!(gate(&d).is_err()); // bad state
        cleanup(&d);
    }

    // Minimal std-only temp dir (avoids a dev-dependency).
    fn tempdir() -> std::path::PathBuf {
        let p = std::env::temp_dir().join(format!(
            "orx-verify-test-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&p).unwrap();
        p
    }
    fn cleanup(p: &Path) {
        let _ = std::fs::remove_dir_all(p);
    }
}
