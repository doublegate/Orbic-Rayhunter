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
        let t = ln.trim_start();
        if t.starts_with('#') || t.is_empty() {
            continue;
        }
        let f: Vec<&str> = ln.split_whitespace().collect();
        // `<mountpoint> <fstype> [key=val ...] <state>`: at least mountpoint,
        // fstype and state.
        if f.len() < 3 {
            continue;
        }
        // Parse key=val only from the MIDDLE tokens, so a row with no trailing
        // state does not let its `sha256=` double as the state word.
        let toks: HashMap<&str, &str> = f[2..f.len() - 1]
            .iter()
            .filter_map(|t| t.split_once('='))
            .collect();
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

/// A SHA-256 hex digest is exactly 64 hex characters.
fn is_sha256_hex(s: &str) -> bool {
    s.len() == 64 && s.bytes().all(|b| b.is_ascii_hexdigit())
}

/// The archive filename for a mountpoint must be a single safe path component --
/// no separators, no `.`/`..` -- even though the mountpoint came from an
/// untrusted MANIFEST.txt. `archive_name` already maps `/` to `-`; this is the
/// belt-and-braces check.
fn safe_archive_name(name: &str) -> bool {
    !name.is_empty()
        && name != "."
        && name != ".."
        && !name.contains('/')
        && !name.contains('\\')
        && !name.contains('\0')
}

/// Outcome of verifying one archive against its manifest entry.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Verdict {
    Ok,
    Missing,
    /// The manifest row lacks a `size=` or `sha256=` token, so there is nothing
    /// to verify against. A checksum is mandatory -- a metadata-less row must
    /// never be accepted as a verified backup.
    MissingMetadata,
    TooSmall(u64),
    SizeMismatch {
        expected: u64,
        actual: u64,
    },
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
            Verdict::MissingMetadata => "no size=/sha256= in manifest".to_string(),
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
            Err(e) if e.kind() == std::io::ErrorKind::Interrupted => continue,
            Err(e) => return Err(e.to_string()),
        }
    }
    Ok(crate::sha256::hex_of(h))
}

/// Verify one manifest entry's archive on disk in `dir`.
pub fn verify_entry(dir: &Path, entry: &ManifestEntry) -> Verdict {
    // A checksum is mandatory: without a valid size AND a valid 64-hex sha256
    // there is nothing to verify, and an unverified archive is not a backup.
    let (Some(expected), Some(want)) = (entry.size, entry.sha256.as_ref()) else {
        return Verdict::MissingMetadata;
    };
    if !is_sha256_hex(want) {
        return Verdict::MissingMetadata;
    }
    let name = archive_name(&entry.mountpoint);
    if !safe_archive_name(&name) {
        return Verdict::ReadError(format!(
            "unsafe archive name from mountpoint {:?}",
            entry.mountpoint
        ));
    }
    let path = dir.join(format!("{name}.tar"));
    let meta = match std::fs::metadata(&path) {
        Ok(m) if m.is_file() => m,
        Ok(_) => return Verdict::Missing, // exists but not a regular file
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Verdict::Missing,
        Err(e) => return Verdict::ReadError(e.to_string()),
    };
    let actual = meta.len();
    if actual < 1024 {
        return Verdict::TooSmall(actual);
    }
    if expected != actual {
        return Verdict::SizeMismatch { expected, actual };
    }
    match hash_file(&path) {
        // Case-insensitive: a manifest might record uppercase hex.
        Ok(got) if got.eq_ignore_ascii_case(want) => {}
        Ok(_) => return Verdict::HashMismatch,
        Err(e) => return Verdict::ReadError(e),
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
    gate_from_entries(dir, &parse_manifest(&text))
}

/// Gate decision over already-parsed entries (so a caller that has verified the
/// entries need not re-hash). Requires EXACTLY ONE rootfs `/` entry whose state
/// is `ok`/`ok-live` and whose archive verifies; duplicate `/` rows are rejected
/// as an ambiguous manifest.
pub fn gate_from_entries(dir: &Path, entries: &[ManifestEntry]) -> Result<(), String> {
    let roots: Vec<&ManifestEntry> = entries.iter().filter(|e| e.mountpoint == "/").collect();
    let root = match roots.as_slice() {
        [] => return Err("manifest has no rootfs '/' entry".to_string()),
        [one] => *one,
        _ => return Err("manifest has multiple rootfs '/' entries (ambiguous)".to_string()),
    };
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
    use std::sync::atomic::{AtomicUsize, Ordering};

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
    fn parse_reads_both_layouts_and_middle_tokens_only() {
        let e = parse_manifest(&MANIFEST.replace("SHA", "deadbeef"));
        assert_eq!(e.len(), 2);
        assert_eq!(e[0].mountpoint, "/");
        assert_eq!(e[0].size, Some(4096));
        assert_eq!(e[0].sha256.as_deref(), Some("deadbeef"));
        assert_eq!(e[1].state, "ok-live"); // entries= did not confuse the state
        // A row with no trailing state must NOT let sha256= double as the state.
        let e2 = parse_manifest("/ ubifs size=4096 sha256=abc");
        assert_eq!(e2[0].state, "sha256=abc");
        assert_eq!(e2[0].sha256, None); // sha256= was the state slot, not a middle token
    }

    #[test]
    fn verify_entry_matches_and_catches_corruption() {
        let d = TempDir::new();
        let text = setup(d.path(), &[7u8; 4096]);
        let entries = parse_manifest(&text);
        assert!(verify_entry(d.path(), &entries[0]).is_ok());
        std::fs::write(d.path().join("data.tar"), [9u8; 4096]).unwrap();
        assert_eq!(verify_entry(d.path(), &entries[1]), Verdict::HashMismatch);
        std::fs::remove_file(d.path().join("root.tar")).unwrap();
        assert_eq!(verify_entry(d.path(), &entries[0]), Verdict::Missing);
    }

    #[test]
    fn verify_entry_requires_valid_metadata() {
        let d = TempDir::new();
        std::fs::write(d.path().join("root.tar"), [7u8; 4096]).unwrap();
        let no_sha = parse_manifest("/ ubifs size=4096 ok");
        assert_eq!(verify_entry(d.path(), &no_sha[0]), Verdict::MissingMetadata);
        let bad_hex = parse_manifest("/ ubifs size=4096 sha256=notvalidhex ok");
        assert_eq!(
            verify_entry(d.path(), &bad_hex[0]),
            Verdict::MissingMetadata
        );
        let bad_size = parse_manifest("/ ubifs size=corrupt sha256=deadbeef ok");
        assert_eq!(
            verify_entry(d.path(), &bad_size[0]),
            Verdict::MissingMetadata
        );
    }

    #[test]
    fn verify_entry_size_mismatch() {
        let d = TempDir::new();
        std::fs::write(d.path().join("root.tar"), [7u8; 4096]).unwrap();
        let sha = crate::sha256::hex(&[7u8; 4096]);
        let e = parse_manifest(&format!("/ ubifs size=9999 sha256={sha} ok"));
        assert_eq!(
            verify_entry(d.path(), &e[0]),
            Verdict::SizeMismatch {
                expected: 9999,
                actual: 4096
            }
        );
    }

    #[test]
    fn gate_requires_verified_rootfs() {
        let d = TempDir::new();
        setup(d.path(), &[7u8; 4096]);
        assert!(gate(d.path()).is_ok());
        std::fs::write(d.path().join("root.tar"), [7u8; 10]).unwrap();
        assert!(gate(d.path()).is_err()); // below the 1 KiB floor
    }

    #[test]
    fn gate_rejects_missing_file_bad_state_and_duplicates() {
        let d = TempDir::new();
        // Missing MANIFEST.txt entirely.
        assert!(gate(d.path()).is_err());
        // No rootfs entry.
        assert!(
            gate_from_entries(
                d.path(),
                &parse_manifest("/data ubifs size=4096 sha256=x ok")
            )
            .is_err()
        );
        // Bad state.
        let sha = crate::sha256::hex(&[7u8; 4096]);
        std::fs::write(d.path().join("root.tar"), [7u8; 4096]).unwrap();
        let failed = parse_manifest(&format!("/ ubifs size=4096 sha256={sha} failed"));
        assert!(gate_from_entries(d.path(), &failed).is_err());
        // Duplicate rootfs rows -> ambiguous -> reject.
        let dup = parse_manifest(&format!(
            "/ ubifs size=4096 sha256={sha} ok\n/ rootfs size=4096 sha256={sha} ok"
        ));
        assert!(
            gate_from_entries(d.path(), &dup)
                .unwrap_err()
                .contains("multiple")
        );
    }

    /// std-only temp dir with a collision-resistant name and Drop cleanup, so a
    /// panicking assertion does not leak the directory.
    struct TempDir(std::path::PathBuf);
    static N: AtomicUsize = AtomicUsize::new(0);
    impl TempDir {
        fn new() -> Self {
            let p = std::env::temp_dir().join(format!(
                "orx-verify-{}-{}",
                std::process::id(),
                N.fetch_add(1, Ordering::Relaxed)
            ));
            std::fs::create_dir_all(&p).unwrap();
            Self(p)
        }
        fn path(&self) -> &Path {
            &self.0
        }
    }
    impl Drop for TempDir {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }
}
