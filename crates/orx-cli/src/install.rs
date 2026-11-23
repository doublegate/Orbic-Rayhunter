//! `orx install` / `orx uninstall`: deploy our production firmware build to the
//! device and reverse it. High blast radius (a bad `/etc/init.d` write can break
//! boot), so both are DRY-RUN by default and gated on a verified backup
//! (`verify::gate`); `--apply` executes. The deployed file set and paths mirror
//! upstream's network installer (`upstream/installer/src/orbic_network.rs`) and
//! the RE in `re/notes/install-footprint.md`, with OUR daemon binary.
//!
//! A step plan is built with all payload bytes resolved up front, so the plan
//! can be printed (dry run) and unit-tested against a fake shell without
//! touching hardware; only `execute` with `apply = true` writes to the device.

use std::path::Path;

use crate::transport::{Shell, safe_remote};

const DAEMON_REMOTE: &str = "/data/rayhunter/rayhunter-daemon";
const CONFIG_REMOTE: &str = "/data/rayhunter/config.toml";
const RAYHUNTER_INIT: &str = "/etc/init.d/rayhunter_daemon";
const MISC_DAEMON: &str = "/etc/init.d/misc-daemon";
/// Where install stashes the stock misc-daemon so uninstall can restore it even
/// without the original backup directory.
const MISC_DAEMON_SAVE: &str = "/data/rayhunter/misc-daemon.orig";

/// One ordered step. Payloads are bytes so a step is self-contained and the plan
/// is printable without the source files.
pub enum Step {
    /// Run a device command (mkdir, chmod, cp, rm, reboot).
    Cmd { desc: String, command: String },
    /// Transfer `bytes` to `remote` (verified by device sha256), then chmod.
    Put {
        desc: String,
        remote: String,
        bytes: Vec<u8>,
        chmod: &'static str,
    },
}

impl Step {
    pub fn describe(&self) -> String {
        match self {
            Step::Cmd { desc, command } => format!("{desc}: {command}"),
            Step::Put {
                desc,
                remote,
                bytes,
                chmod,
            } => {
                format!(
                    "{desc}: put {} bytes -> {remote} (chmod {chmod})",
                    bytes.len()
                )
            }
        }
    }
}

/// Render `config.toml.in` into the device config: set `device = "orbic"` (the
/// installer's one substitution) and drop the leading `# cat config.toml` hint.
fn render_config(template: &str) -> String {
    let mut out = String::with_capacity(template.len() + 32);
    let mut set_device = false;
    for line in template.lines() {
        if line.trim_start() == "# cat config.toml" {
            continue;
        }
        if line.trim_start().starts_with("#device = \"orbic\"")
            || line.trim_start().starts_with("device = ")
        {
            out.push_str("device = \"orbic\"\n");
            set_device = true;
            continue;
        }
        out.push_str(line);
        out.push('\n');
    }
    if !set_device {
        out.push_str("device = \"orbic\"\n");
    }
    out
}

fn read(path: &Path) -> Result<Vec<u8>, String> {
    std::fs::read(path).map_err(|e| format!("cannot read {}: {e}", path.display()))
}

/// Build the install step plan from the daemon binary, the two init scripts, and
/// the config template. Validates every source exists (fail before any device
/// write). All remote paths are the fixed constants above (no arbitrary paths).
pub fn install_plan(
    daemon: &Path,
    rayhunter_init: &Path,
    misc_daemon: &Path,
    config_template: &Path,
) -> Result<Vec<Step>, String> {
    let daemon_bytes = read(daemon)?;
    let init_bytes = read(rayhunter_init)?;
    let misc_bytes = read(misc_daemon)?;
    let config_text = String::from_utf8(read(config_template)?)
        .map_err(|_| "config template is not UTF-8".to_string())?;
    let config_bytes = render_config(&config_text).into_bytes();

    Ok(vec![
        Step::Cmd {
            desc: "create install dirs".into(),
            command: "mkdir -p /data/rayhunter/scripts /data/rayhunter/bin".into(),
        },
        // Save the stock misc-daemon ONCE (never clobber an existing save), so
        // uninstall can restore it; `[ -f save ]` keeps a re-install idempotent.
        Step::Cmd {
            desc: "stash stock misc-daemon".into(),
            command: format!("[ -f {MISC_DAEMON_SAVE} ] || cp {MISC_DAEMON} {MISC_DAEMON_SAVE}"),
        },
        Step::Put {
            desc: "daemon".into(),
            remote: DAEMON_REMOTE.into(),
            bytes: daemon_bytes,
            chmod: "755",
        },
        Step::Put {
            desc: "config".into(),
            remote: CONFIG_REMOTE.into(),
            bytes: config_bytes,
            chmod: "644",
        },
        Step::Put {
            desc: "rayhunter init".into(),
            remote: RAYHUNTER_INIT.into(),
            bytes: init_bytes,
            chmod: "755",
        },
        Step::Put {
            desc: "misc-daemon (hooked)".into(),
            remote: MISC_DAEMON.into(),
            bytes: misc_bytes,
            chmod: "755",
        },
        Step::Cmd {
            desc: "reboot".into(),
            command: "reboot".into(),
        },
    ])
}

/// Build the uninstall step plan: restore the stock misc-daemon from the stash,
/// remove our files (leaving `/data/rayhunter-data` recordings), and reboot.
pub fn uninstall_plan() -> Vec<Step> {
    vec![
        Step::Cmd {
            desc: "restore stock misc-daemon".into(),
            command: format!(
                "if [ -f {MISC_DAEMON_SAVE} ]; then mv {MISC_DAEMON_SAVE} {MISC_DAEMON}; else echo 'no saved misc-daemon; restore from backup root.tar' >&2; exit 1; fi"
            ),
        },
        Step::Cmd {
            desc: "remove rayhunter init".into(),
            command: format!("rm -f {RAYHUNTER_INIT}"),
        },
        Step::Cmd {
            desc: "remove daemon + config".into(),
            command: format!("rm -f {DAEMON_REMOTE} {CONFIG_REMOTE}"),
        },
        Step::Cmd {
            desc: "reboot".into(),
            command: "reboot".into(),
        },
    ]
}

/// Execute a plan. Always enforces the backup gate first. With `apply = false`
/// (the default) it prints the plan and makes no device change. `reboot` is only
/// sent on apply, last.
pub fn execute(
    shell: &mut dyn Shell,
    steps: &[Step],
    gate: impl FnOnce() -> Result<(), String>,
    apply: bool,
) -> Result<(), String> {
    gate().map_err(|e| format!("backup gate not satisfied, refusing to proceed: {e}"))?;
    for s in steps {
        if !apply {
            println!("  [dry-run] {}", s.describe());
            continue;
        }
        match s {
            Step::Put {
                remote,
                bytes,
                chmod,
                ..
            } => {
                if !safe_remote(remote) {
                    return Err(format!("unsafe remote path in plan: {remote:?}"));
                }
                println!("  {}", s.describe());
                shell.send_file(remote, bytes)?;
                let r = shell.run(&format!("chmod {chmod} {remote}"))?;
                if r.status != 0 {
                    return Err(format!(
                        "chmod {chmod} {remote} failed: {}",
                        r.output.trim()
                    ));
                }
            }
            Step::Cmd { command, .. } => {
                println!("  {}", s.describe());
                // reboot closes the shell; do not wait for a status marker.
                if command.trim() == "reboot" {
                    let _ = shell.run("reboot");
                    println!("  device rebooting");
                    continue;
                }
                let r = shell.run(command)?;
                if r.status != 0 {
                    return Err(format!(
                        "step failed (status {}): {} :: {}",
                        r.status,
                        command,
                        r.output.trim()
                    ));
                }
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::transport::fake::FakeShell;

    #[test]
    fn render_config_sets_device_and_drops_hint() {
        let t = "# cat config.toml\nport = 8080\n#device = \"orbic\"\nui_level = 1\n";
        let c = render_config(t);
        assert!(!c.contains("# cat config.toml"));
        assert!(c.contains("device = \"orbic\"\n"));
        assert!(!c.contains("#device"));
        assert_eq!(c.matches("device = \"orbic\"").count(), 1);
    }

    #[test]
    fn install_plan_order_and_paths() {
        let d = tmp();
        std::fs::write(d.join("daemon"), [1u8; 2048]).unwrap();
        std::fs::write(d.join("rayhunter_daemon"), b"#init\n").unwrap();
        std::fs::write(d.join("misc-daemon"), b"#misc\n").unwrap();
        std::fs::write(d.join("config.toml.in"), b"port = 8080\n").unwrap();
        let steps = install_plan(
            &d.join("daemon"),
            &d.join("rayhunter_daemon"),
            &d.join("misc-daemon"),
            &d.join("config.toml.in"),
        )
        .unwrap();
        let descs: Vec<String> = steps.iter().map(|s| s.describe()).collect();
        // mkdir first, reboot last, daemon goes to the fixed remote path.
        assert!(descs[0].contains("mkdir -p /data/rayhunter"));
        assert!(descs.last().unwrap().contains("reboot"));
        assert!(
            descs
                .iter()
                .any(|x| x.contains("/data/rayhunter/rayhunter-daemon"))
        );
        assert!(descs.iter().any(|x| x.contains("/etc/init.d/misc-daemon")));
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn install_plan_errors_on_missing_artifact() {
        let d = tmp();
        assert!(
            install_plan(
                &d.join("nope"),
                &d.join("nope2"),
                &d.join("nope3"),
                &d.join("nope4"),
            )
            .is_err()
        );
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn execute_refuses_when_gate_fails() {
        let mut sh = FakeShell::default();
        let steps = uninstall_plan();
        let err = execute(&mut sh, &steps, || Err("no backup".into()), true).unwrap_err();
        assert!(err.contains("backup gate not satisfied"));
        assert!(sh.fs.is_empty()); // nothing written
    }

    #[test]
    fn execute_dry_run_writes_nothing() {
        let mut sh = FakeShell::default();
        let d = tmp();
        std::fs::write(d.join("daemon"), [1u8; 2048]).unwrap();
        std::fs::write(d.join("i"), b"x").unwrap();
        std::fs::write(d.join("m"), b"y").unwrap();
        std::fs::write(d.join("c"), b"port = 8080\n").unwrap();
        let steps =
            install_plan(&d.join("daemon"), &d.join("i"), &d.join("m"), &d.join("c")).unwrap();
        execute(&mut sh, &steps, || Ok(()), false).unwrap();
        assert!(sh.fs.is_empty()); // dry run: no device write
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn execute_apply_deploys_and_verifies() {
        let mut sh = FakeShell::default();
        let d = tmp();
        std::fs::write(d.join("daemon"), [1u8; 2048]).unwrap();
        std::fs::write(d.join("i"), b"#init\n").unwrap();
        std::fs::write(d.join("m"), b"#misc\n").unwrap();
        std::fs::write(d.join("c"), b"port = 8080\n").unwrap();
        let steps =
            install_plan(&d.join("daemon"), &d.join("i"), &d.join("m"), &d.join("c")).unwrap();
        execute(&mut sh, &steps, || Ok(()), true).unwrap();
        // The daemon and the hooked misc-daemon landed (send_file verified them).
        assert_eq!(
            sh.fs.get("/data/rayhunter/rayhunter-daemon").unwrap().len(),
            2048
        );
        assert!(sh.fs.contains_key("/etc/init.d/misc-daemon"));
        assert!(sh.fs.contains_key("/data/rayhunter/config.toml"));
        let _ = std::fs::remove_dir_all(&d);
    }

    use std::sync::atomic::{AtomicUsize, Ordering};
    static N: AtomicUsize = AtomicUsize::new(0);
    fn tmp() -> std::path::PathBuf {
        let p = std::env::temp_dir().join(format!(
            "orx-install-{}-{}",
            std::process::id(),
            N.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir_all(&p).unwrap();
        p
    }
}
