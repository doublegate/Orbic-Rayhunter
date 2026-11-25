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
        self.recordings.iter().map(|r| r.bytes).sum()
    }
}

pub struct RescueOutcome {
    pub deleted: usize,
    pub failed: usize,
    pub freed_bytes: u64,
}

/// Discover the recording store path from the device config, else the default.
fn store_path(shell: &mut dyn Shell) -> String {
    match shell.run(&format!("cat {CONFIG_PATH}")) {
        Ok(r) if r.status == 0 => r
            .output
            .lines()
            .find_map(|l| {
                let (k, v) = l.split_once('=')?;
                (k.trim() == "qmdl_store_path").then(|| v.trim().trim_matches('"').to_string())
            })
            .filter(|p| safe_remote(p))
            .unwrap_or_else(|| STORE_DEFAULT.to_string()),
        _ => STORE_DEFAULT.to_string(),
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
    // Oldest first (timestamp names sort chronologically as strings of equal len,
    // but sort numerically to be safe).
    out.sort_by(|a, b| a.name.cmp(&b.name));
    out
}

/// Build the rescue plan: free space on the data partition + stored recordings.
pub fn plan(shell: &mut dyn Shell) -> Result<RescuePlan, String> {
    let store = store_path(shell);
    let df = shell.run(&format!("df -k {DATA_MOUNT}"))?;
    let available_kb = crate::install::parse_df_available_kb(&df.output, DATA_MOUNT);
    // `ls -l <store>/*.qmdl.gz`; a non-zero status (no matches) yields no rows.
    let ls = shell.run(&format!("ls -l {store}/*.qmdl.gz 2>/dev/null"))?;
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
    // Stop the daemon first so it is not writing while we delete under it.
    let stop = shell.run(INIT_STOP)?;
    if stop.status != 0 {
        return Err(format!(
            "failed to stop the daemon ({INIT_STOP} exited {}); not deleting anything",
            stop.status
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
        match shell.run(&format!("rm -f {}", files.join(" "))) {
            Ok(r) if r.status == 0 => {
                println!("deleted {} ({} bytes)", rec.name, rec.bytes);
                out.deleted += 1;
                out.freed_bytes += rec.bytes;
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
            "ls -l /data/rayhunter/qmdl/*.qmdl.gz 2>/dev/null".into(),
            CmdResult {
                output: ls,
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
            "rm -f /data/rayhunter/qmdl/1795442375.qmdl.gz /data/rayhunter/qmdl/1795442375.ndjson /data/rayhunter/qmdl/1795442375-gps.ndjson".into(),
            CmdResult { output: String::new(), status: 0 },
        );
        sh.responses.insert(
            "rm -f /data/rayhunter/qmdl/1795457329.qmdl.gz /data/rayhunter/qmdl/1795457329.ndjson /data/rayhunter/qmdl/1795457329-gps.ndjson".into(),
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
    fn apply_aborts_if_daemon_stop_fails() {
        let mut sh = shell_with(187000, &[(100, "a"), (200, "b")]);
        sh.responses.insert(
            INIT_STOP.into(),
            CmdResult {
                output: "err".into(),
                status: 1,
            },
        );
        assert!(
            apply(&mut sh, 0).is_err(),
            "a failed stop must abort before any delete"
        );
        assert!(!sh.cmds.iter().any(|c| c.starts_with("rm -f")));
    }
}
