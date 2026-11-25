//! `orx rescue`: a READ-ONLY diagnostic over the `:24` root shell for when the
//! daemon's `:8080` API is unresponsive (e.g. the `/data` partition filled up).
//! It reports free space on `/data` and the stored recordings (oldest first), so
//! you can see what is there and decide what to free.
//!
//! Deleting recordings is intentionally NOT done here yet. Doing it safely means
//! stopping the daemon, removing files AND reconciling the daemon's
//! `manifest.toml`, restarting the daemon, and verifying recovery -- a path that
//! must be validated on a genuinely wedged device, which this tool cannot do
//! from a read of a healthy one. Until then `orx rescue` only reports; the manual
//! recovery steps are in docs/INSTALL.md section 9. (Follow-up: T-OR-041b.)
//!
//! The `:24` shell is not reboot-persistent; re-open it first (see INSTALL.md).
//! All device output is untrusted and parsed defensively.

use crate::transport::{Shell, safe_remote};

/// Default recording store (config `qmdl_store_path`) and the partition it lives
/// on. `rescue` reads the store path from the daemon config when it can, and
/// falls back to this default.
pub const STORE_DEFAULT: &str = "/data/rayhunter/qmdl";
pub const DATA_MOUNT: &str = "/data";
const CONFIG_PATH: &str = "/data/rayhunter/config.toml";

/// One stored recording (its compressed `.qmdl.gz`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Recording {
    pub name: String,
    pub bytes: u64,
}

pub struct RescuePlan {
    pub store: String,
    /// Available KiB on the data partition (always `Some`: `plan` fails closed if
    /// free space cannot be read, which also catches an unmounted `/data`).
    pub available_kb: u64,
    /// Recordings sorted OLDEST first (names are numeric unix timestamps).
    pub recordings: Vec<Recording>,
}

impl RescuePlan {
    pub fn total_bytes(&self) -> u64 {
        // Saturating: sizes are parsed from untrusted device output.
        self.recordings
            .iter()
            .fold(0u64, |acc, r| acc.saturating_add(r.bytes))
    }
}

/// Discover the recording store path from the device config. Fails closed: a
/// shell error propagates, and a `qmdl_store_path` that is present but invalid
/// is an error (we never silently fall back on a bad value). The default is used
/// ONLY when the config is unreadable or has no such key.
fn store_path(shell: &mut dyn Shell) -> Result<String, String> {
    let r = shell.run(&format!("cat {CONFIG_PATH}"))?;
    if r.status != 0 {
        return validate_store(STORE_DEFAULT);
    }
    let found = r.output.lines().find_map(|l| {
        let (k, v) = l.split_once('=')?;
        (k.trim() == "qmdl_store_path").then(|| v.trim().trim_matches('"').to_string())
    });
    match found {
        Some(p) => validate_store(&p),
        None => validate_store(STORE_DEFAULT),
    }
}

/// A store path must be a safe absolute path with at least one component beneath
/// `/data/` and no `..` segment (fail closed). A trailing slash is trimmed.
fn validate_store(p: &str) -> Result<String, String> {
    let p = p.trim_end_matches('/');
    let under_data = p
        .strip_prefix("/data/")
        .is_some_and(|rest| !rest.is_empty() && !rest.starts_with('/'));
    let no_dotdot = !p.split('/').any(|seg| seg == "..");
    if safe_remote(p) && under_data && no_dotdot {
        Ok(p.to_string())
    } else {
        Err(format!(
            "refusing unsafe qmdl_store_path {p:?} (must be a clean path with a subdirectory under /data/)"
        ))
    }
}

/// Parse a busybox `ls -l <dir>` listing (basenames, no path) into recordings. A
/// regular-file row is EXACTLY nine whitespace tokens
/// `<perms> <links> <owner> <group> <size> <mon> <day> <time> <name>`; rows with
/// a different field count (whitespace in the name, a symlink's ` -> target`), a
/// non-regular type, a name that is not `<digits>.qmdl.gz`, or a duplicate name
/// are rejected rather than guessed.
fn parse_ls(output: &str) -> Result<Vec<Recording>, String> {
    let mut out = Vec::new();
    let mut seen = std::collections::BTreeSet::new();
    for line in output.lines() {
        let fields: Vec<&str> = line.split_whitespace().collect();
        if fields.len() != 9 {
            continue;
        }
        if !fields[0].starts_with('-') {
            continue; // regular files only
        }
        let name_field = fields[8];
        let Some(name) = name_field.strip_suffix(".qmdl.gz") else {
            continue;
        };
        // Recording names are unix timestamps; a non-numeric name is anomalous.
        // Skip it (never let it occupy a slot or mis-sort) rather than guess.
        if name.is_empty() || !name.bytes().all(|b| b.is_ascii_digit()) {
            continue;
        }
        let Ok(bytes) = fields[4].parse::<u64>() else {
            continue;
        };
        if !seen.insert(name.to_string()) {
            return Err(format!(
                "duplicate recording name in device listing: {name}"
            ));
        }
        out.push(Recording {
            name: name.to_string(),
            bytes,
        });
    }
    // Oldest first, numerically (all names are validated numeric above).
    out.sort_by_key(|r| r.name.parse::<u64>().unwrap_or(u64::MAX));
    Ok(out)
}

/// Build the rescue plan: free space on the data partition + stored recordings.
/// Fails closed if `/data` is not mounted or the store directory is missing.
pub fn plan(shell: &mut dyn Shell) -> Result<RescuePlan, String> {
    let store = store_path(shell)?;
    // Free space. `parse_df_available_kb` returns None if the row is not actually
    // for /data (e.g. unmounted, so /data resolves to /), which we fail closed on.
    let df = shell.run(&format!("df -k {DATA_MOUNT}"))?;
    let available_kb = crate::install::parse_df_available_kb(&df.output, DATA_MOUNT)
        .ok_or_else(|| format!("could not read free space on {DATA_MOUNT} (is it mounted?)"))?;
    // The store must exist (catches a missing store / unmounted partition).
    let test = shell.run(&format!("test -d {store} && echo OK"))?;
    if !test.output.contains("OK") {
        return Err(format!("recording store {store} not found on the device"));
    }
    // List the directory itself (no shell glob) so an unbounded number of files
    // cannot exceed ARG_MAX; parse_ls filters for `*.qmdl.gz`.
    let ls = shell.run(&format!("ls -l {store}"))?;
    let recordings = parse_ls(&ls.output)?;
    Ok(RescuePlan {
        store,
        available_kb,
        recordings,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::transport::CmdResult;
    use crate::transport::fake::FakeShell;

    fn ls_line(size: u64, name: &str) -> String {
        format!("-rw-r--r--    1 root     root     {size:>9} Nov 23 13:08 {name}.qmdl.gz")
    }

    #[test]
    fn parse_ls_extracts_name_and_size_oldest_first() {
        let out = format!(
            "{}\n{}\n",
            ls_line(200, "1795457329"),
            ls_line(100, "1795442375")
        );
        let recs = parse_ls(&out).unwrap();
        assert_eq!(recs.len(), 2);
        assert_eq!(recs[0].name, "1795442375", "oldest first");
        assert_eq!(recs[0].bytes, 100);
        assert_eq!(recs[1].name, "1795457329");
    }

    #[test]
    fn parse_ls_skips_junk_symlinks_dirs_and_nonnumeric() {
        let out = "total 1234\n\
                   lrwxrwxrwx 1 root root 20 Nov 23 13:08 a.qmdl.gz -> /other/b.qmdl.gz\n\
                   drwxr-xr-x 2 root root 4096 Nov 23 13:08 weird.qmdl.gz\n\
                   -rw-r--r-- 1 root root 123 Nov 23 13:08 copy 1795457329.qmdl.gz\n\
                   -rw-r--r-- 1 root root 123 Nov 23 13:08 notatimestamp.qmdl.gz\n";
        assert!(parse_ls(out).unwrap().is_empty());
    }

    #[test]
    fn parse_ls_fails_on_duplicate_names() {
        let out = format!(
            "{}\n{}\n",
            ls_line(100, "1795442375"),
            ls_line(200, "1795442375")
        );
        assert!(parse_ls(&out).is_err(), "duplicate names must fail closed");
    }

    #[test]
    fn validate_store_rules() {
        assert!(validate_store("/data/rayhunter/qmdl").is_ok());
        assert!(
            validate_store("/data/rayhunter/qmdl/").is_ok(),
            "trailing slash trimmed"
        );
        assert!(
            validate_store("/data/").is_err(),
            "partition root, no subdir"
        );
        assert!(validate_store("/data").is_err());
        assert!(validate_store("/etc/passwd").is_err(), "not under /data/");
        assert!(validate_store("/data/../etc").is_err(), "dotdot rejected");
    }

    fn shell_with(df_avail_kb: u64, recs: &[(u64, &str)], store_ok: bool) -> FakeShell {
        let mut sh = FakeShell::default();
        sh.responses.insert(
            "cat /data/rayhunter/config.toml".into(),
            CmdResult {
                output: "qmdl_store_path = \"/data/rayhunter/qmdl\"\n".into(),
                status: 0,
            },
        );
        sh.responses.insert(
            "df -k /data".into(),
            CmdResult {
                output: format!(
                    "Filesystem 1K-blocks Used Available Use% Mounted on\nubi0:usrfs 219904 30000 {df_avail_kb} 13% /data\n"
                ),
                status: 0,
            },
        );
        sh.responses.insert(
            "test -d /data/rayhunter/qmdl && echo OK".into(),
            CmdResult {
                output: if store_ok {
                    "OK\n".into()
                } else {
                    String::new()
                },
                status: if store_ok { 0 } else { 1 },
            },
        );
        let ls: String = recs
            .iter()
            .map(|(s, n)| format!("{}\n", ls_line(*s, n)))
            .collect();
        sh.responses.insert(
            "ls -l /data/rayhunter/qmdl".into(),
            CmdResult {
                output: ls,
                status: 0,
            },
        );
        sh
    }

    #[test]
    fn plan_reports_space_and_recordings() {
        let mut sh = shell_with(187000, &[(100, "1795442375"), (200, "1795457329")], true);
        let p = plan(&mut sh).unwrap();
        assert_eq!(p.store, "/data/rayhunter/qmdl");
        assert_eq!(p.available_kb, 187000);
        assert_eq!(p.recordings.len(), 2);
        assert_eq!(p.total_bytes(), 300);
    }

    #[test]
    fn plan_fails_closed_when_store_missing() {
        let mut sh = shell_with(187000, &[], false);
        assert!(plan(&mut sh).is_err(), "a missing store must fail closed");
    }

    #[test]
    fn plan_fails_closed_when_data_not_mounted() {
        let mut sh = shell_with(187000, &[], true);
        // df row for `/` instead of `/data` (the unmounted case) -> None -> err.
        sh.responses.insert(
            "df -k /data".into(),
            CmdResult {
                output: "Filesystem 1K-blocks Used Available Use% Mounted on\nubi0:rootfs 67584 49000 17000 74% /\n".into(),
                status: 0,
            },
        );
        assert!(
            plan(&mut sh).is_err(),
            "an unmounted /data must fail closed"
        );
    }

    #[test]
    fn store_path_fails_closed_on_unsafe_value() {
        let mut sh = FakeShell::default();
        sh.responses.insert(
            "cat /data/rayhunter/config.toml".into(),
            CmdResult {
                output: "qmdl_store_path = \"/etc/passwd\"\n".into(),
                status: 0,
            },
        );
        assert!(store_path(&mut sh).is_err());
    }
}
