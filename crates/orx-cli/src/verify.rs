//! `orx verify`: re-check a backup directory against its `MANIFEST.txt` by
//! re-hashing each archive, and the shared backup-gate used by write commands.
//!
//! This is the host-side counterpart to `orx backup`: it proves the archives on
//! disk still match what the manifest recorded (size + SHA-256), so a backup can
//! be trusted as a rollback before any device write. No device access, so it is
//! fully unit-tested.

use std::collections::HashMap;
use std::io::Read;
use std::os::unix::fs::OpenOptionsExt;
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
/// parse; the state is the last whitespace token. Comment/blank lines are
/// skipped. MANIFEST.txt is an untrusted integrity boundary for a device gate,
/// so this FAILS CLOSED: a non-comment line with fewer than 3 tokens, a middle
/// token without `=`, or a duplicate `size=`/`sha256=` key is an error, not a
/// silently-dropped or last-wins row.
pub fn parse_manifest(text: &str) -> Result<Vec<ManifestEntry>, String> {
    let mut out = Vec::new();
    for (n, ln) in text.lines().enumerate() {
        let t = ln.trim_start();
        if t.starts_with('#') || t.is_empty() {
            continue;
        }
        let f: Vec<&str> = ln.split_whitespace().collect();
        if f.len() < 3 {
            return Err(format!(
                "manifest line {}: malformed (< 3 fields): {ln:?}",
                n + 1
            ));
        }
        // Parse key=val from the MIDDLE tokens only, so a row with no trailing
        // state cannot let its `sha256=` double as the state word. Every middle
        // token must be a key=val, and size/sha256 must not repeat.
        let mut toks: HashMap<&str, &str> = HashMap::new();
        for tok in &f[2..f.len() - 1] {
            let Some((k, v)) = tok.split_once('=') else {
                return Err(format!(
                    "manifest line {}: token {tok:?} is not key=value",
                    n + 1
                ));
            };
            if (k == "size" || k == "sha256") && toks.insert(k, v).is_some() {
                return Err(format!("manifest line {}: duplicate {k}= key", n + 1));
            }
            if k != "size" && k != "sha256" {
                toks.insert(k, v);
            }
        }
        out.push(ManifestEntry {
            mountpoint: f[0].to_string(),
            fstype: f[1].to_string(),
            size: toks.get("size").and_then(|v| v.parse().ok()),
            sha256: toks.get("sha256").map(|v| (*v).to_string()),
            state: f[f.len() - 1].to_string(),
        });
    }
    Ok(out)
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
    /// The path exists but is a symlink or not a regular file (directory/FIFO).
    NotRegularFile,
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
            Verdict::NotRegularFile => "not a regular file (symlink/dir/fifo)".to_string(),
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

/// SHA-256 of an already-open file, returning (lowercase hex, bytes read).
/// Hashing the open handle rather than re-opening by path closes the TOCTOU
/// between a size check and the hash, and counting bytes lets the caller verify
/// the length actually read.
fn hash_reader(f: &mut std::fs::File) -> Result<(String, u64), String> {
    let mut h = Sha256::new();
    let mut buf = [0u8; 1 << 16];
    let mut total: u64 = 0;
    loop {
        match f.read(&mut buf) {
            Ok(0) => break,
            Ok(n) => {
                h.update(&buf[..n]);
                total += n as u64;
            }
            Err(e) if e.kind() == std::io::ErrorKind::Interrupted => continue,
            Err(e) => return Err(e.to_string()),
        }
    }
    Ok((crate::sha256::hex_of(h), total))
}

/// O_NOFOLLOW (Linux): refuse to open a final-component symlink.
const O_NOFOLLOW: i32 = 0o400000;

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
    // Reject a symlink or non-regular file before opening (the backup dir is
    // untrusted); `symlink_metadata` does not follow the final symlink.
    match std::fs::symlink_metadata(&path) {
        Ok(m) if m.file_type().is_symlink() || !m.is_file() => return Verdict::NotRegularFile,
        Ok(_) => {}
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Verdict::Missing,
        Err(e) => return Verdict::ReadError(e.to_string()),
    }
    // Open once with O_NOFOLLOW (closes the symlink-swap TOCTOU after the check)
    // and hash the handle; the byte count read is the authoritative size.
    let mut f = match std::fs::OpenOptions::new()
        .read(true)
        .custom_flags(O_NOFOLLOW)
        .open(&path)
    {
        Ok(f) => f,
        // ELOOP: the path became a symlink between the check and the open.
        Err(e) if e.raw_os_error() == Some(40) => return Verdict::NotRegularFile,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Verdict::Missing,
        Err(e) => return Verdict::ReadError(e.to_string()),
    };
    let (got, actual) = match hash_reader(&mut f) {
        Ok(x) => x,
        Err(e) => return Verdict::ReadError(e),
    };
    if actual < 1024 {
        return Verdict::TooSmall(actual);
    }
    if expected != actual {
        return Verdict::SizeMismatch { expected, actual };
    }
    if !got.eq_ignore_ascii_case(want) {
        // Case-insensitive: a manifest might record uppercase hex.
        return Verdict::HashMismatch;
    }
    Verdict::Ok
}

/// Verify every entry's archive, hashing each once.
pub fn verify_all(dir: &Path, entries: &[ManifestEntry]) -> Vec<Verdict> {
    entries.iter().map(|e| verify_entry(dir, e)).collect()
}

/// The single gate policy, evaluated over PRE-COMPUTED verdicts so no archive is
/// hashed twice: a usable rollback needs EXACTLY ONE rootfs `/` entry whose state
/// is `ok`/`ok-live` and whose archive verified. Both `gate` and `orx verify`
/// call this, so the rule cannot drift between them.
pub fn gate_from_verdicts(entries: &[ManifestEntry], verdicts: &[Verdict]) -> Result<(), String> {
    let roots: Vec<usize> = (0..entries.len())
        .filter(|&i| entries[i].mountpoint == "/")
        .collect();
    let i = match roots.as_slice() {
        [] => return Err("manifest has no rootfs '/' entry".to_string()),
        [i] => *i,
        _ => return Err("manifest has multiple rootfs '/' entries (ambiguous)".to_string()),
    };
    let e = &entries[i];
    if e.state != "ok" && e.state != "ok-live" {
        return Err(format!("rootfs '/' state is {:?}, not ok/ok-live", e.state));
    }
    match &verdicts[i] {
        Verdict::Ok => Ok(()),
        v => Err(format!(
            "rootfs '/' archive does not verify: {}",
            v.describe()
        )),
    }
}

/// The backup-gate for write commands: a directory holds a usable rollback only
/// if its MANIFEST.txt parses, records rootfs `/` as `ok`/`ok-live`, and that
/// archive verifies. The Rust counterpart of `orbic-ota-block.py`'s
/// `_backup_gate`.
pub fn gate(dir: &Path) -> Result<(), String> {
    let manifest = dir.join("MANIFEST.txt");
    let text = std::fs::read_to_string(&manifest)
        .map_err(|e| format!("cannot read {}: {e}", manifest.display()))?;
    let entries = parse_manifest(&text)?;
    let verdicts = verify_all(dir, &entries);
    gate_from_verdicts(&entries, &verdicts)
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

    fn parse(text: &str) -> Vec<ManifestEntry> {
        parse_manifest(text).expect("manifest should parse")
    }

    #[test]
    fn parse_reads_both_layouts_and_middle_tokens_only() {
        let e = parse(&MANIFEST.replace("SHA", "deadbeef"));
        assert_eq!(e.len(), 2);
        assert_eq!(e[0].mountpoint, "/");
        assert_eq!(e[0].size, Some(4096));
        assert_eq!(e[0].sha256.as_deref(), Some("deadbeef"));
        assert_eq!(e[1].state, "ok-live"); // entries= did not confuse the state
        // A row with no trailing state must NOT let sha256= double as the state.
        let e2 = parse("/ ubifs size=4096 sha256=abc");
        assert_eq!(e2[0].state, "sha256=abc");
        assert_eq!(e2[0].sha256, None); // sha256= was the state slot, not a middle token
    }

    #[test]
    fn parse_fails_closed_on_malformed_and_duplicates() {
        assert!(parse_manifest("/ ubifs").is_err()); // < 3 fields
        assert!(parse_manifest("/ ubifs bareword sha256=deadbeef ok").is_err()); // middle token without =
        assert!(parse_manifest("/ ubifs size=1 size=2 ok").is_err()); // duplicate key
        assert!(parse_manifest("/ ubifs sha256=a sha256=b ok").is_err());
    }

    #[test]
    fn verify_entry_matches_and_catches_corruption() {
        let d = TempDir::new();
        let text = setup(d.path(), &[7u8; 4096]);
        let entries = parse(&text);
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
        assert_eq!(
            verify_entry(d.path(), &parse("/ ubifs size=4096 ok")[0]),
            Verdict::MissingMetadata
        );
        assert_eq!(
            verify_entry(
                d.path(),
                &parse("/ ubifs size=4096 sha256=notvalidhex ok")[0]
            ),
            Verdict::MissingMetadata
        );
        assert_eq!(
            verify_entry(
                d.path(),
                &parse("/ ubifs size=corrupt sha256=deadbeef ok")[0]
            ),
            Verdict::MissingMetadata
        );
    }

    #[test]
    fn verify_entry_size_mismatch_uppercase_and_nonregular() {
        let d = TempDir::new();
        std::fs::write(d.path().join("root.tar"), [7u8; 4096]).unwrap();
        let sha = crate::sha256::hex(&[7u8; 4096]);
        assert_eq!(
            verify_entry(
                d.path(),
                &parse(&format!("/ ubifs size=9999 sha256={sha} ok"))[0]
            ),
            Verdict::SizeMismatch {
                expected: 9999,
                actual: 4096
            }
        );
        // Uppercase hex in the manifest still matches.
        let up = sha.to_uppercase();
        assert!(
            verify_entry(
                d.path(),
                &parse(&format!("/ ubifs size=4096 sha256={up} ok"))[0]
            )
            .is_ok()
        );
        // A directory at <name>.tar is not a regular file.
        std::fs::create_dir(d.path().join("data.tar")).unwrap();
        assert_eq!(
            verify_entry(
                d.path(),
                &parse(&format!("/data ubifs size=4096 sha256={sha} ok"))[0]
            ),
            Verdict::NotRegularFile
        );
    }

    #[test]
    fn unsafe_mountpoint_is_rejected() {
        // archive_name maps `/` to `-`, so a crafted mountpoint cannot escape the
        // dir; the guard is belt-and-braces. `/..` -> ".." -> rejected.
        assert!(!safe_archive_name(".."));
        assert!(!safe_archive_name("a/b"));
        assert!(safe_archive_name("root"));
    }

    #[test]
    fn gate_requires_verified_rootfs() {
        let d = TempDir::new();
        setup(d.path(), &[7u8; 4096]);
        assert!(gate(d.path()).is_ok());
        std::fs::write(d.path().join("root.tar"), [7u8; 10]).unwrap();
        assert!(gate(d.path()).is_err()); // below the 1 KiB floor
    }

    fn gate_entries(dir: &Path, entries: &[ManifestEntry]) -> Result<(), String> {
        let v = verify_all(dir, entries);
        gate_from_verdicts(entries, &v)
    }

    #[test]
    fn gate_rejects_missing_file_bad_state_and_duplicates() {
        let d = TempDir::new();
        // Missing MANIFEST.txt entirely.
        assert!(gate(d.path()).is_err());
        // No rootfs entry.
        assert!(
            gate_entries(d.path(), &parse("/data ubifs size=4096 sha256=deadbeef ok")).is_err()
        );
        // Bad state.
        let sha = crate::sha256::hex(&[7u8; 4096]);
        std::fs::write(d.path().join("root.tar"), [7u8; 4096]).unwrap();
        assert!(
            gate_entries(
                d.path(),
                &parse(&format!("/ ubifs size=4096 sha256={sha} failed"))
            )
            .is_err()
        );
        // Duplicate rootfs rows -> ambiguous -> reject.
        let dup = parse(&format!(
            "/ ubifs size=4096 sha256={sha} ok\n/ rootfs size=4096 sha256={sha} ok"
        ));
        assert!(
            gate_entries(d.path(), &dup)
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
