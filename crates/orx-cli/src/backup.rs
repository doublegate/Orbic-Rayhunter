//! `orx backup`: archive each writable device filesystem with `tar`, streamed
//! over the root shell to a host file, hashed inline, and recorded in a
//! MANIFEST the write commands gate on. This is the Rust port of
//! `scripts/orbic-backup.py`; the device wire work is the [`crate::transport`]
//! `Shell` trait, so the orchestration is unit-tested against a fake.
//!
//! A raw NAND dump deadlocks this device (the live UBI contends with raw MTD
//! reads), so a tar of the mounted `ubifs` is the correct, safe backup and the
//! right rollback for a Rayhunter install (which only changes mounted volumes).

use std::io::Write;

use crate::sha256::Sha256;
use crate::transport::{Shell, safe_remote};

/// One line of `/proc/mounts` reduced to what a backup needs.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Mount {
    pub mountpoint: String,
    pub fstype: String,
    pub writable: bool,
}

/// Decode the octal escapes `/proc/mounts` uses for spaces etc. (`\040`).
fn unescape(field: &str) -> String {
    let bytes = field.as_bytes();
    let mut out = String::new();
    let mut i = 0;
    while i < bytes.len() {
        let octal = bytes[i] == b'\\'
            && i + 3 < bytes.len()
            && bytes[i + 1..i + 4].iter().all(u8::is_ascii_digit);
        let decoded = if octal {
            u8::from_str_radix(&field[i + 1..i + 4], 8).ok()
        } else {
            None
        };
        if let Some(code) = decoded {
            out.push(code as char);
            i += 4;
        } else {
            out.push(bytes[i] as char);
            i += 1;
        }
    }
    out
}

/// Parse `/proc/mounts`, last mount of a given mountpoint winning (overlays).
pub fn parse_mounts(proc_mounts: &str) -> Vec<Mount> {
    let mut out: Vec<Mount> = Vec::new();
    for line in proc_mounts.lines() {
        let f: Vec<&str> = line.split_whitespace().collect();
        if f.len() < 4 {
            continue;
        }
        let mountpoint = unescape(f[1]);
        let fstype = f[2].to_string();
        let writable = f[3].split(',').any(|o| o == "rw");
        out.retain(|m| m.mountpoint != mountpoint);
        out.push(Mount {
            mountpoint,
            fstype,
            writable,
        });
    }
    out
}

/// The filesystems to back up: every writable `ubifs`, plus `/` whatever its
/// reported fstype (the rootfs is where an install writes).
pub fn backup_targets(mounts: &[Mount]) -> Vec<&Mount> {
    mounts
        .iter()
        .filter(|m| m.writable && (m.fstype == "ubifs" || m.mountpoint == "/"))
        .collect()
}

/// `true` if `child` is a proper sub-path of `parent` (so its tar is a separate
/// archive and must be excluded from the parent's).
fn is_nested(parent: &str, child: &str) -> bool {
    if parent == "/" {
        return child != "/" && child.starts_with('/');
    }
    child.len() > parent.len()
        && child.starts_with(parent)
        && child.as_bytes()[parent.len()] == b'/'
}

/// Path of `child` relative to `parent`, as a `tar` `--exclude` operand (`./sub`).
fn nested_rel(parent: &str, child: &str) -> String {
    let rest = if parent == "/" {
        child.trim_start_matches('/')
    } else {
        child[parent.len()..].trim_start_matches('/')
    };
    format!("./{rest}")
}

/// Build the `tar` command for `target`, excluding every other mountpoint nested
/// under it so each filesystem is archived exactly once. Returns `None` if the
/// target path is not shell-safe (defense in depth; device mountpoints are ours).
pub fn tar_command(target: &str, all_mountpoints: &[String]) -> Option<String> {
    if !safe_remote(target) && target != "/" {
        return None;
    }
    let mut excludes = String::new();
    let mut nested: Vec<&String> = all_mountpoints
        .iter()
        .filter(|mp| is_nested(target, mp))
        .collect();
    nested.sort();
    for mp in nested {
        let rel = nested_rel(target, mp);
        // rel is derived from a mountpoint; reject anything not plain.
        if rel.contains(|c: char| c.is_whitespace() || "';\"$`\\&|<>(){}*?!".contains(c)) {
            return None;
        }
        excludes.push_str(&format!(" --exclude={rel}"));
    }
    // Matches scripts/orbic-backup.py, which is proven against this device's
    // busybox tar: excludes before `-cf`, and `2>/dev/null` so tar's warnings
    // (e.g. "file changed as we read it" on a live rw fs) are discarded instead
    // of being merged into the archive stream by the run framing's `2>&1`. A
    // live fs still makes tar exit non-zero, which `classify` records as ok-live.
    // `--ignore-failed-read` is NOT used: this device's busybox tar rejects it,
    // which would abort the archive.
    Some(format!("tar -C {target}{excludes} -cf - . 2>/dev/null"))
}

/// A short archive name for a mountpoint: `/` -> `root`, `/usrdata` -> `usrdata`,
/// `/a/b` -> `a-b`.
pub fn archive_name(mountpoint: &str) -> String {
    if mountpoint == "/" {
        return "root".to_string();
    }
    mountpoint.trim_matches('/').replace('/', "-")
}

/// One archived filesystem's result.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ArchiveResult {
    pub mountpoint: String,
    pub fstype: String,
    pub bytes: u64,
    pub sha256: String,
    /// `"ok"` (tar exit 0) or `"ok-live"` (exit non-zero on a writable fs with
    /// `--allow-live`); anything else is an `Err` from `backup_all`.
    pub state: String,
}

impl ArchiveResult {
    /// The MANIFEST line, matching the format the write-gate reads
    /// (`<mp> <fstype> size=<n> sha256=<h> <state>`).
    pub fn manifest_line(&self) -> String {
        format!(
            "{} {} size={} sha256={} {}",
            self.mountpoint, self.fstype, self.bytes, self.sha256, self.state
        )
    }
}

/// A writer that tees into an inline SHA-256 as bytes stream through.
struct Hashing<'a, W: Write> {
    inner: &'a mut W,
    hasher: Sha256,
    bytes: u64,
}

impl<W: Write> Write for Hashing<'_, W> {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        let n = self.inner.write(buf)?;
        self.hasher.update(&buf[..n]);
        self.bytes += n as u64;
        Ok(n)
    }
    fn flush(&mut self) -> std::io::Result<()> {
        self.inner.flush()
    }
}

/// Archive one target through `shell`, streaming into `sink` while hashing.
/// `allow_live` permits a non-zero tar exit on a writable fs (recorded `ok-live`).
pub fn archive_one<W: Write>(
    shell: &mut dyn Shell,
    target: &Mount,
    all_mountpoints: &[String],
    sink: &mut W,
) -> Result<(u64, String, i32), String> {
    let cmd = tar_command(&target.mountpoint, all_mountpoints).ok_or_else(|| {
        format!(
            "unsafe mountpoint, refusing to tar: {:?}",
            target.mountpoint
        )
    })?;
    let mut hashing = Hashing {
        inner: sink,
        hasher: Sha256::new(),
        bytes: 0,
    };
    let status = shell.run_to(&cmd, &mut hashing)?;
    let bytes = hashing.bytes;
    let hash = crate::sha256::hex_of(hashing.hasher);
    Ok((bytes, hash, status))
}

/// Decide the per-archive state from the tar exit status and archive size.
///
/// A size floor guards against a failed tar masquerading as a backup: any valid
/// tar is at least the two final 512-byte zero blocks (1024 bytes) plus a
/// header, so anything under 1024 bytes is not an archive (e.g. a busybox usage
/// error from an unsupported flag) and fails regardless of `--allow-live`.
pub fn classify(
    status: i32,
    bytes: u64,
    writable: bool,
    allow_live: bool,
) -> Result<String, String> {
    if bytes < 1024 {
        return Err(format!(
            "archive is only {bytes} bytes -- tar did not produce a valid archive"
        ));
    }
    match status {
        0 => Ok("ok".to_string()),
        s if writable && allow_live => Ok(format!("ok-live (tar exit {s})")),
        s => Err(format!(
            "tar exited {s}; rerun with --allow-live for a writable filesystem"
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::transport::CmdResult;
    use crate::transport::fake::FakeShell;

    const MOUNTS: &str = "\
ubi0:rootfs / ubifs rw,relatime 0 0
proc /proc proc rw,nosuid 0 0
tmpfs /tmp tmpfs rw 0 0
ubi1:data /data ubifs rw,relatime 0 0
ubi2:usr /usrdata ubifs ro,relatime 0 0
ubi1:data /data ubifs ro,remount 0 0";

    #[test]
    fn parse_mounts_last_wins_and_reads_rw() {
        let m = parse_mounts(MOUNTS);
        let data = m.iter().find(|x| x.mountpoint == "/data").unwrap();
        assert!(!data.writable, "last /data line is a ro remount -> ro wins");
        assert!(m.iter().find(|x| x.mountpoint == "/").unwrap().writable);
    }

    #[test]
    fn parse_mounts_unescapes_spaces() {
        let m = parse_mounts("src /mnt/a\\040b ubifs rw 0 0");
        assert_eq!(m[0].mountpoint, "/mnt/a b");
    }

    #[test]
    fn targets_are_writable_ubifs_plus_root() {
        let m = parse_mounts(MOUNTS);
        let t: Vec<&str> = backup_targets(&m)
            .iter()
            .map(|x| x.mountpoint.as_str())
            .collect();
        assert_eq!(t, vec!["/"]); // /data ends ro; /usrdata ro; only / is rw here
    }

    #[test]
    fn tar_command_excludes_nested_mounts_sorted() {
        let mps = vec!["/".to_string(), "/data".to_string(), "/proc".to_string()];
        let cmd = tar_command("/", &mps).unwrap();
        assert_eq!(
            cmd,
            "tar -C / --exclude=./data --exclude=./proc -cf - . 2>/dev/null"
        );
        let sub = tar_command("/data", &mps).unwrap();
        assert_eq!(sub, "tar -C /data -cf - . 2>/dev/null"); // nothing nested under /data
    }

    #[test]
    fn tar_command_rejects_unsafe_mountpoint() {
        assert!(tar_command("/a;reboot", &[]).is_none());
    }

    #[test]
    fn archive_name_maps_paths() {
        assert_eq!(archive_name("/"), "root");
        assert_eq!(archive_name("/usrdata"), "usrdata");
        assert_eq!(archive_name("/a/b"), "a-b");
    }

    #[test]
    fn archive_one_streams_hashes_and_reports_status() {
        let payload = b"TARBALL-CONTENTS-1234567890".to_vec();
        let mut sh = FakeShell::default();
        sh.responses.insert(
            "tar -C / -cf - . 2>/dev/null".to_string(),
            CmdResult {
                output: String::from_utf8(payload.clone()).unwrap(),
                status: 0,
            },
        );
        let target = Mount {
            mountpoint: "/".to_string(),
            fstype: "ubifs".to_string(),
            writable: true,
        };
        let mut sink = Vec::new();
        let (bytes, hash, status) =
            archive_one(&mut sh, &target, &["/".to_string()], &mut sink).unwrap();
        assert_eq!(sink, payload);
        assert_eq!(bytes, payload.len() as u64);
        assert_eq!(hash, crate::sha256::hex(&payload));
        assert_eq!(status, 0);
    }

    #[test]
    fn classify_states() {
        let big = 4096;
        assert_eq!(classify(0, big, true, false).unwrap(), "ok");
        assert!(classify(1, big, true, false).is_err());
        assert!(classify(1, big, true, true).unwrap().starts_with("ok-live"));
        assert!(classify(2, big, false, true).is_err()); // not writable -> no live allowance
        // Size floor: a tiny "archive" (e.g. a tar usage error) always fails.
        assert!(
            classify(0, 654, true, true)
                .unwrap_err()
                .contains("654 bytes")
        );
    }
}
