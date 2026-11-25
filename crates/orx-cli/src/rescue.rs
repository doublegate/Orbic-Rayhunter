//! `orx rescue`: recover a wedged device through the `:24` root shell when the
//! daemon's `:8080` API is unresponsive (e.g. the `/data` partition filled up).
//! It reports free space and the stored recordings, and -- only with `--apply`
//! -- stops the daemon and deletes the OLDEST recordings, keeping the newest N.
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
const INIT_STOP: &str = "/etc/init.d/rayhunter_daemon stop";

/// One stored recording (its compressed `.qmdl.gz` and companion files).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Recording {
    pub name: String,
    pub bytes: u64,
}

pub struct RescuePlan {
    pub store: String,
    pub available_kb: Option<u64>,
    /// Recordings sorted OLDEST first (names are unix timestamps).
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

pub struct RescueOutcome {
    pub deleted: usize,
    pub failed: usize,
    pub freed_bytes: u64,
}

/// Discover the recording store path from the device config. Fails closed: a
/// shell error propagates, and a `qmdl_store_path` that is present but unsafe or
/// not under `/data/` is an error (we never silently fall back in that case).
/// The default is used ONLY when the config has no such key (status 0, absent).
fn store_path(shell: &mut dyn Shell) -> Result<String, String> {
    let r = shell.run(&format!("cat {CONFIG_PATH}"))?;
    if r.status != 0 {
        // Config unreadable (missing/permission): fall back to the known default,
        // which is itself validated below.
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

/// A store path must be a safe absolute path under `/data/` (fail closed).
fn validate_store(p: &str) -> Result<String, String> {
    if safe_remote(p) && p.starts_with("/data/") {
        Ok(p.to_string())
    } else {
        Err(format!(
            "refusing unsafe qmdl_store_path {p:?} (must be a clean path under /data/)"
        ))
    }
}

/// Parse a busybox `ls -l` listing of `*.qmdl.gz` into (name, size) pairs. A
/// line is `<perms> <links> <owner> <group> <size> <mon> <day> <time> <path>`;
/// we take a numeric size column and the basename minus `.qmdl.gz`. Lines that
/// do not parse (totals, errors) are skipped.
fn parse_ls(output: &str) -> Vec<Recording> {
    let mut out = Vec::new();
    for line in output.lines() {
        let fields: Vec<&str> = line.split_whitespace().collect();
        if fields.len() < 9 {
            continue;
        }
        // Regular files only: skip directories ('d'), symlinks ('l', whose last
        // field is the link target), etc. `ls -ld` lists each entry itself.
        if !fields[0].starts_with('-') {
            continue;
        }
        let Some(path) = fields.last() else { continue };
        let Some(file) = path.rsplit('/').next() else {
            continue;
        };
        let Some(name) = file.strip_suffix(".qmdl.gz") else {
            continue;
        };
        // The size is the last all-digit field before the date columns (index 4
        // in the canonical layout, but owner/group can shift it); scan for it.
        let Some(bytes) = fields[2..fields.len() - 4]
            .iter()
            .rev()
            .find_map(|f| f.parse::<u64>().ok())
        else {
            continue;
        };
        if !name.is_empty() {
            out.push(Recording {
                name: name.to_string(),
                bytes,
            });
        }
    }
    // Oldest first. Sort NUMERICALLY by the timestamp name: lexicographic order
    // would misrank pre-NITZ/NTP captures with short timestamps (e.g. "3600")
    // against normal 10-digit ones and could classify a newer capture as oldest.
    // Non-numeric names sort last (treated as newest, so kept).
    out.sort_by_key(|r| (r.name.parse::<u64>().unwrap_or(u64::MAX), r.name.clone()));
    out
}

/// Build the rescue plan: free space on the data partition + stored recordings.
pub fn plan(shell: &mut dyn Shell) -> Result<RescuePlan, String> {
    let store = store_path(shell)?;
    let df = shell.run(&format!("df -k {DATA_MOUNT}"))?;
    let available_kb = crate::install::parse_df_available_kb(&df.output, DATA_MOUNT);
    // `ls -ld -- <store>/*.qmdl.gz`: -d lists each match itself (never a dir's
    // contents), -- ends options; a non-zero status (no matches) yields no rows.
    let ls = shell.run(&format!("ls -ld -- {store}/*.qmdl.gz 2>/dev/null"))?;
    let recordings = parse_ls(&ls.output);
    Ok(RescuePlan {
        store,
        available_kb,
        recordings,
    })
}

/// Stop the daemon and delete the oldest recordings, keeping the newest `keep`.
/// Each deleted recording removes its `.qmdl.gz`, `.ndjson` and `-gps.ndjson`.
pub fn apply(shell: &mut dyn Shell, keep: usize) -> Result<RescueOutcome, String> {
    let plan = plan(shell)?;
    let mut out = RescueOutcome {
        deleted: 0,
        failed: 0,
        freed_bytes: 0,
    };
    let to_delete = plan.recordings.len().saturating_sub(keep);
    if to_delete == 0 {
        return Ok(out);
    }
    // Attempt to stop the daemon, then VERIFY it is actually gone. The init stop
    // script returns non-zero when the daemon has already crashed/exited -- which
    // is the exact wedged state rescue exists for -- so its exit code is not a
    // reliable signal. We proceed only once the process is confirmed not running,
    // and fail closed if it is still alive (don't delete under a live writer).
    let _ = shell.run(INIT_STOP);
    if daemon_running(shell)? {
        return Err(format!(
            "daemon still running after {INIT_STOP}; not deleting anything"
        ));
    }
    for rec in plan.recordings.iter().take(to_delete) {
        let base = format!("{}/{}", plan.store, rec.name);
        let files = [
            format!("{base}.qmdl.gz"),
            format!("{base}.ndjson"),
            format!("{base}-gps.ndjson"),
        ];
        if !files.iter().all(|p| safe_remote(p)) {
            eprintln!("skip {}: unsafe path", rec.name);
            out.failed += 1;
            continue;
        }
        match shell.run(&format!("rm -f -- {}", files.join(" "))) {
            Ok(r) if r.status == 0 => {
                println!("deleted {} ({} bytes)", rec.name, rec.bytes);
                out.deleted += 1;
                out.freed_bytes = out.freed_bytes.saturating_add(rec.bytes);
            }
            Ok(r) => {
                eprintln!("failed to delete {}: rm exited {}", rec.name, r.status);
                out.failed += 1;
            }
            Err(e) => {
                eprintln!("failed to delete {}: {e}", rec.name);
                out.failed += 1;
            }
        }
    }
    Ok(out)
}

/// Whether the rayhunter daemon process is currently running on the device.
fn daemon_running(shell: &mut dyn Shell) -> Result<bool, String> {
    let ps = shell.run("ps")?;
    // The daemon's argv contains the binary name; `ps` lists the command.
    Ok(ps.output.contains("rayhunter-daemon"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::transport::CmdResult;
    use crate::transport::fake::FakeShell;

    fn ls_line(size: u64, name: &str) -> String {
        format!(
            "-rw-r--r--    1 root     root     {size:>9} Nov 23 13:08 /data/rayhunter/qmdl/{name}.qmdl.gz"
        )
    }

    #[test]
    fn parse_ls_extracts_name_and_size_oldest_first() {
        let out = format!(
            "{}\n{}\n",
            ls_line(200, "1795457329"),
            ls_line(100, "1795442375")
        );
        let recs = parse_ls(&out);
        assert_eq!(recs.len(), 2);
        assert_eq!(recs[0].name, "1795442375", "oldest first");
        assert_eq!(recs[0].bytes, 100);
        assert_eq!(recs[1].name, "1795457329");
    }

    #[test]
    fn parse_ls_skips_junk_lines() {
        let out = "total 1234\nls: /x/*.qmdl.gz: No such file or directory\n";
        assert!(parse_ls(out).is_empty());
    }

    #[test]
    fn parse_ls_skips_symlinks_and_dirs() {
        // A symlink ('l') whose last field is the target, and a directory ('d'):
        // both must be skipped (only regular files '-' count).
        let out = "lrwxrwxrwx 1 root root 20 Nov 23 13:08 /data/rayhunter/qmdl/a.qmdl.gz -> /other/b.qmdl.gz\n\
                   drwxr-xr-x 2 root root 4096 Nov 23 13:08 /data/rayhunter/qmdl/weird.qmdl.gz\n";
        assert!(parse_ls(out).is_empty());
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
        assert!(
            store_path(&mut sh).is_err(),
            "a store path outside /data/ must fail closed"
        );
    }

    fn shell_with(df_avail_kb: u64, recs: &[(u64, &str)]) -> FakeShell {
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
        let ls: String = recs
            .iter()
            .map(|(s, n)| format!("{}\n", ls_line(*s, n)))
            .collect();
        sh.responses.insert(
            "ls -ld -- /data/rayhunter/qmdl/*.qmdl.gz 2>/dev/null".into(),
            CmdResult {
                output: ls,
                status: 0,
            },
        );
        // By default the daemon is NOT running (post-stop); tests that need it
        // alive override this `ps` response.
        sh.responses.insert(
            "ps".into(),
            CmdResult {
                output: "  PID USER  COMMAND\n  1 root /sbin/init\n".into(),
                status: 0,
            },
        );
        sh
    }

    #[test]
    fn plan_reports_space_and_recordings() {
        let mut sh = shell_with(187000, &[(100, "1795442375"), (200, "1795457329")]);
        let p = plan(&mut sh).unwrap();
        assert_eq!(p.store, "/data/rayhunter/qmdl");
        assert_eq!(p.available_kb, Some(187000));
        assert_eq!(p.recordings.len(), 2);
        assert_eq!(p.total_bytes(), 300);
    }

    #[test]
    fn apply_deletes_oldest_keeping_newest_n_after_stopping_daemon() {
        let mut sh = shell_with(
            187000,
            &[
                (100, "1795442375"),
                (200, "1795457329"),
                (300, "1795583694"),
            ],
        );
        sh.responses.insert(
            INIT_STOP.into(),
            CmdResult {
                output: "stopped".into(),
                status: 0,
            },
        );
        // Accept the rm commands for the two oldest.
        sh.responses.insert(
            "rm -f -- /data/rayhunter/qmdl/1795442375.qmdl.gz /data/rayhunter/qmdl/1795442375.ndjson /data/rayhunter/qmdl/1795442375-gps.ndjson".into(),
            CmdResult { output: String::new(), status: 0 },
        );
        sh.responses.insert(
            "rm -f -- /data/rayhunter/qmdl/1795457329.qmdl.gz /data/rayhunter/qmdl/1795457329.ndjson /data/rayhunter/qmdl/1795457329-gps.ndjson".into(),
            CmdResult { output: String::new(), status: 0 },
        );
        let out = apply(&mut sh, 1).unwrap();
        assert_eq!(out.deleted, 2, "two oldest deleted, newest 1 kept");
        assert_eq!(out.failed, 0);
        assert_eq!(out.freed_bytes, 300);
        // The newest recording's qmdl must NOT have been removed.
        assert!(
            !sh.cmds.iter().any(|c| c.contains("1795583694.qmdl.gz")),
            "newest deleted: {:?}",
            sh.cmds
        );
        // The daemon stop ran before any rm.
        let stop_idx = sh.cmds.iter().position(|c| c == INIT_STOP).unwrap();
        let first_rm = sh.cmds.iter().position(|c| c.starts_with("rm -f")).unwrap();
        assert!(
            stop_idx < first_rm,
            "daemon must be stopped before deleting"
        );
    }

    #[test]
    fn apply_keeps_all_when_keep_ge_count() {
        let mut sh = shell_with(187000, &[(100, "1795442375")]);
        let out = apply(&mut sh, 5).unwrap();
        assert_eq!(out.deleted, 0);
        // Must not even stop the daemon when there is nothing to delete.
        assert!(!sh.cmds.iter().any(|c| c == INIT_STOP), "{:?}", sh.cmds);
    }

    #[test]
    fn apply_proceeds_when_stop_returns_nonzero_but_daemon_is_gone() {
        // The wedged case: the daemon already crashed, so the init stop script
        // exits non-zero -- but `ps` shows it is gone, so rescue must proceed.
        let mut sh = shell_with(187000, &[(100, "1795442375"), (200, "1795457329")]);
        sh.responses.insert(
            INIT_STOP.into(),
            CmdResult {
                output: "not running".into(),
                status: 1,
            },
        );
        sh.responses.insert(
            "rm -f -- /data/rayhunter/qmdl/1795442375.qmdl.gz /data/rayhunter/qmdl/1795442375.ndjson /data/rayhunter/qmdl/1795442375-gps.ndjson".into(),
            CmdResult { output: String::new(), status: 0 },
        );
        let out = apply(&mut sh, 1).unwrap();
        assert_eq!(
            out.deleted, 1,
            "a non-zero stop with a dead daemon must not block rescue"
        );
    }

    #[test]
    fn apply_aborts_if_daemon_still_running_after_stop() {
        let mut sh = shell_with(187000, &[(100, "1795442375"), (200, "1795457329")]);
        sh.responses.insert(
            INIT_STOP.into(),
            CmdResult {
                output: "ok".into(),
                status: 0,
            },
        );
        // ps still shows the daemon -> must abort before any delete.
        sh.responses.insert(
            "ps".into(),
            CmdResult {
                output: "  PID USER COMMAND\n 1553 root /data/rayhunter/rayhunter-daemon /data/rayhunter/config.toml\n".into(),
                status: 0,
            },
        );
        assert!(
            apply(&mut sh, 0).is_err(),
            "a daemon still alive after stop must abort before any delete"
        );
        assert!(!sh.cmds.iter().any(|c| c.starts_with("rm -f")));
    }
}
