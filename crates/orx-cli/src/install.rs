//! `orx install` / `orx uninstall`: deploy our production firmware build to the
//! device and reverse it. High blast radius (a bad `/etc/init.d` write can break
//! boot, and there is no software unbrick path), so both are DRY-RUN by default
//! and gated on a verified backup (`verify::gate`); `--apply` executes.
//!
//! Design choices that keep this safe:
//! - **Atomic writes.** Every file is sent to a temp beside its target, chmod'd,
//!   then `mv`'d into place (same filesystem). A dropped connection or failed
//!   chmod can never leave `/etc/init.d/misc-daemon` half-written or
//!   non-executable, and `mv`-over-a-running-binary avoids `ETXTBSY`.
//! - **Stock `misc-daemon` comes from the VERIFIED BACKUP, not the device.** The
//!   device may already carry upstream's *hooked* `misc-daemon` (Phase 2), so a
//!   device `cp` would stash the hooked script as "stock". `orx uninstall`
//!   extracts the true stock `./etc/init.d/misc-daemon` from the backup's
//!   `root.tar` (which `verify::gate` has already checked).
//!
//! A step plan is built with all payload bytes resolved up front, so it prints
//! (dry run) and is unit-tested against a fake shell; only `execute(.., true)`
//! writes.

use std::path::Path;

use crate::transport::{Shell, safe_remote};

const DAEMON_REMOTE: &str = "/data/rayhunter/rayhunter-daemon";
const CONFIG_REMOTE: &str = "/data/rayhunter/config.toml";
const RAYHUNTER_INIT: &str = "/etc/init.d/rayhunter_daemon";
const MISC_DAEMON: &str = "/etc/init.d/misc-daemon";

/// One ordered step. Payloads are bytes so a step is self-contained.
pub enum Step {
    Cmd {
        desc: String,
        command: String,
    },
    /// Transfer `bytes` to `remote` atomically (temp -> chmod -> mv), verified by
    /// the device's sha256 against what was sent.
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
                    "{desc}: put {} bytes -> {remote} (chmod {chmod}, atomic)",
                    bytes.len()
                )
            }
        }
    }
}

/// Render `config.toml.in` into the device config: set `device = "orbic"` exactly
/// once, in the ROOT table, before the first `[section]` (so it is never placed
/// inside a sub-table), replacing any existing/commented `device` line. Drops the
/// stray `# cat config.toml` hint line.
pub fn render_config(template: &str) -> String {
    let is_device_line = |t: &str| -> bool {
        let s = t.trim_start_matches('#').trim_start();
        s.strip_prefix("device")
            .map(|r| r.trim_start().starts_with('='))
            .unwrap_or(false)
    };
    let mut out = String::with_capacity(template.len() + 32);
    let mut in_root = true;
    let mut emitted = false;
    for line in template.lines() {
        let t = line.trim_start();
        if t == "# cat config.toml" {
            continue;
        }
        if t.starts_with('[') {
            // Leaving the root table: set device here if not already done.
            if in_root && !emitted {
                out.push_str("device = \"orbic\"\n");
                emitted = true;
            }
            in_root = false;
            out.push_str(line);
            out.push('\n');
            continue;
        }
        if in_root && is_device_line(t) {
            if !emitted {
                out.push_str("device = \"orbic\"\n");
                emitted = true;
            }
            continue; // drop the original (possibly commented) device line
        }
        out.push_str(line);
        out.push('\n');
    }
    if !emitted {
        out.push_str("device = \"orbic\"\n");
    }
    out
}

fn read(path: &Path) -> Result<Vec<u8>, String> {
    std::fs::read(path).map_err(|e| format!("cannot read {}: {e}", path.display()))
}

/// A device init script must be non-empty and start with a `#!` shebang (reject a
/// truncated or CRLF-mangled script before it can break boot).
fn check_init_script(bytes: &[u8], what: &str) -> Result<(), String> {
    if bytes.len() < 10 || &bytes[..2] != b"#!" {
        return Err(format!(
            "{what} is not a plausible shell script (too short or no #! shebang)"
        ));
    }
    if bytes.contains(&b'\r') {
        return Err(format!(
            "{what} contains CR (CRLF line endings break BusyBox sh)"
        ));
    }
    if bytes.contains(&0) {
        return Err(format!("{what} contains a NUL byte (not a text script)"));
    }
    if std::str::from_utf8(bytes).is_err() {
        return Err(format!(
            "{what} is not valid UTF-8 (corrupt or wrong file?)"
        ));
    }
    Ok(())
}

/// Whether `hay` contains the byte sequence `needle`.
fn contains(hay: &[u8], needle: &[u8]) -> bool {
    !needle.is_empty() && hay.windows(needle.len()).any(|w| w == needle)
}

/// A 32-bit little-endian ARM ELF (the RC400L is armv7). Rejects a host x86-64
/// binary, a text file, or a corrupt artifact before it is deployed as the
/// daemon. `e_machine == 40` is EM_ARM; `EI_CLASS == 1` 32-bit; `EI_DATA == 1` LE.
fn is_arm32_elf(bytes: &[u8]) -> bool {
    bytes.len() >= 20
        && &bytes[..4] == b"\x7fELF"
        && bytes[4] == 1
        && bytes[5] == 1
        && u16::from_le_bytes([bytes[18], bytes[19]]) == 40
}

/// Build the install step plan. Validates every source (fail before any device
/// write); remote paths are the fixed constants above.
pub fn install_plan(
    daemon: &Path,
    rayhunter_init: &Path,
    misc_daemon: &Path,
    config_template: &Path,
) -> Result<Vec<Step>, String> {
    let daemon_bytes = read(daemon)?;
    if !is_arm32_elf(&daemon_bytes) {
        return Err(format!(
            "{} is not a 32-bit little-endian ARM ELF (wrong arch, or not a daemon binary)",
            daemon.display()
        ));
    }
    let init_bytes = read(rayhunter_init)?;
    check_init_script(&init_bytes, "rayhunter_daemon")?;
    // The init script must actually start OUR daemon, and the misc-daemon we
    // deploy must be the HOOKED one (it references rayhunter_daemon); otherwise
    // the install would boot to a broken or unhooked state.
    if !contains(&init_bytes, b"/data/rayhunter/rayhunter-daemon") {
        return Err(
            "rayhunter_daemon init does not reference /data/rayhunter/rayhunter-daemon".to_string(),
        );
    }
    let misc_bytes = read(misc_daemon)?;
    check_init_script(&misc_bytes, "misc-daemon")?;
    if !contains(&misc_bytes, b"rayhunter_daemon") {
        return Err(
            "misc-daemon does not reference rayhunter_daemon (not the hooked script?)".to_string(),
        );
    }
    let config_text = String::from_utf8(read(config_template)?)
        .map_err(|_| "config template is not UTF-8".to_string())?;
    let config_bytes = render_config(&config_text).into_bytes();

    Ok(vec![
        Step::Cmd {
            desc: "create install dirs".into(),
            command: "mkdir -p /data/rayhunter/scripts /data/rayhunter/bin".into(),
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
        // The hooked misc-daemon goes LAST before reboot: once it is in place the
        // boot path references rayhunter_daemon, which is already deployed above.
        Step::Put {
            desc: "misc-daemon (hooked)".into(),
            remote: MISC_DAEMON.into(),
            bytes: misc_bytes,
            chmod: "755",
        },
        Step::Cmd {
            desc: "flush to flash".into(),
            command: "sync".into(),
        },
        Step::Cmd {
            desc: "reboot".into(),
            command: "reboot".into(),
        },
    ])
}

/// Extract the stock `./etc/init.d/misc-daemon` from a verified backup's
/// `root.tar` (host-side `tar`), so uninstall restores the TRUE stock script
/// regardless of the device's current (possibly hooked) state.
pub fn stock_misc_daemon(backup_dir: &Path) -> Result<Vec<u8>, String> {
    let tar = backup_dir.join("root.tar");
    // Open in Rust and feed the archive on stdin to `tar -xO -f -`: `-f -` forces
    // stdin (without it GNU tar honors $TAPE / a default tape device, not stdin),
    // and keeping the backup dir out of `-f` means a path containing `:` is not
    // treated as a remote `host:file` and a leading `-` is not parsed as an
    // option. `--` ends option parsing before the member name.
    let file =
        std::fs::File::open(&tar).map_err(|e| format!("cannot open {}: {e}", tar.display()))?;
    let out = std::process::Command::new("tar")
        .args(["-xO", "-f", "-", "--", "./etc/init.d/misc-daemon"])
        .stdin(std::process::Stdio::from(file))
        .stderr(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .output()
        .map_err(|e| format!("running tar on {}: {e}", tar.display()))?;
    if !out.status.success() {
        return Err(format!(
            "tar could not extract ./etc/init.d/misc-daemon from {}: {}",
            tar.display(),
            String::from_utf8_lossy(&out.stderr).trim()
        ));
    }
    check_init_script(&out.stdout, "stock misc-daemon from backup")?;
    // The backup may have been taken AFTER Phase 2 (upstream already installed),
    // in which case its misc-daemon is the HOOKED one -- restoring that would
    // leave a dangling hook after we remove rayhunter_daemon. Only a genuinely
    // stock (unhooked) script is a valid restore target.
    if out
        .stdout
        .windows(b"rayhunter_daemon".len())
        .any(|w| w == b"rayhunter_daemon")
    {
        return Err(
            "misc-daemon in the backup is already hooked for rayhunter (backup taken after install); \
             supply a stock backup to uninstall"
                .to_string(),
        );
    }
    Ok(out.stdout)
}

/// Build the uninstall plan: atomically restore the stock `misc-daemon` (bytes
/// from the backup), remove our files (leaving `/data/rayhunter-data`), reboot.
pub fn uninstall_plan(stock_misc_daemon: Vec<u8>) -> Vec<Step> {
    vec![
        Step::Put {
            desc: "restore stock misc-daemon (from backup)".into(),
            remote: MISC_DAEMON.into(),
            bytes: stock_misc_daemon,
            chmod: "755",
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
            desc: "flush to flash".into(),
            command: "sync".into(),
        },
        Step::Cmd {
            desc: "reboot".into(),
            command: "reboot".into(),
        },
    ]
}

/// Execute a plan. Enforces the backup gate first. With `apply = false` it prints
/// the plan and makes no device change. Each `Put` is atomic: send to
/// `<remote>.orx-new`, chmod it, then `mv` into place.
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
        println!("  {}", s.describe());
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
                let tmp = format!("{remote}.orx-new");
                // verifies the temp's sha256; clean up the temp on failure so a
                // partial/failed transfer never leaks (a stray 16 MB daemon temp
                // on /data can wedge the daemon and AP, #1192).
                if let Err(e) = shell.send_file(&tmp, bytes) {
                    let _ = shell.run(&format!("rm -f {tmp}"));
                    return Err(e);
                }
                let c = shell.run(&format!("chmod {chmod} {tmp}"))?;
                if c.status != 0 {
                    let _ = shell.run(&format!("rm -f {tmp}"));
                    return Err(format!("chmod {chmod} {tmp} failed: {}", c.output.trim()));
                }
                let m = shell.run(&format!("mv {tmp} {remote}"))?;
                if m.status != 0 {
                    let _ = shell.run(&format!("rm -f {tmp}"));
                    return Err(format!("mv {tmp} {remote} failed: {}", m.output.trim()));
                }
            }
            Step::Cmd { command, .. } => {
                if command.trim() == "reboot" {
                    let _ = shell.run("reboot"); // closes the shell; no status to wait on
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
    fn render_config_sets_device_once_in_root_before_sections() {
        let t = "# cat config.toml\nport = 8080\n# device = \"orbic\"\nui_level = 1\n[analyzers]\nx = true\n";
        let c = render_config(t);
        assert!(!c.contains("# cat config.toml"));
        assert_eq!(c.matches("device = \"orbic\"").count(), 1);
        // device must be BEFORE the [analyzers] section (root table).
        assert!(c.find("device = \"orbic\"").unwrap() < c.find("[analyzers]").unwrap());
        assert!(!c.contains("# device"));
    }

    #[test]
    fn render_config_inserts_before_section_when_absent() {
        let c = render_config("port = 8080\n[analyzers]\nx = true\n");
        assert_eq!(c.matches("device = \"orbic\"").count(), 1);
        assert!(c.find("device = \"orbic\"").unwrap() < c.find("[analyzers]").unwrap());
    }

    #[test]
    fn check_init_script_rejects_bad_scripts() {
        assert!(check_init_script(b"#!/bin/sh\n", "x").is_ok());
        assert!(check_init_script(b"not a script", "x").is_err());
        assert!(check_init_script(b"#!/bin/sh\r\n", "x").is_err()); // CRLF
        assert!(check_init_script(b"", "x").is_err());
    }

    /// A minimal buffer that passes is_arm32_elf (header only; not runnable).
    fn fake_arm_elf() -> Vec<u8> {
        let mut b = vec![0u8; 2048];
        b[..4].copy_from_slice(b"\x7fELF");
        b[4] = 1; // ELFCLASS32
        b[5] = 1; // little-endian
        b[18] = 40; // EM_ARM (little end of e_machine)
        b
    }

    fn write_artifacts(d: &Path) {
        std::fs::write(d.join("daemon"), fake_arm_elf()).unwrap();
        std::fs::write(
            d.join("rayhunter_daemon"),
            b"#!/bin/sh\n/data/rayhunter/rayhunter-daemon &\n",
        )
        .unwrap();
        std::fs::write(
            d.join("misc-daemon"),
            b"#!/bin/sh\n/etc/init.d/rayhunter_daemon start\n",
        )
        .unwrap();
        std::fs::write(d.join("config.toml.in"), b"port = 8080\n").unwrap();
    }

    #[test]
    fn is_arm32_elf_accepts_arm_rejects_others() {
        assert!(is_arm32_elf(&fake_arm_elf()));
        assert!(!is_arm32_elf(&[0u8; 2048])); // no magic
        let mut x86 = fake_arm_elf();
        x86[18] = 0x3e; // EM_X86_64
        assert!(!is_arm32_elf(&x86));
        let mut be = fake_arm_elf();
        be[5] = 2; // big-endian
        assert!(!is_arm32_elf(&be));
    }

    #[test]
    fn check_init_script_rejects_nul_and_non_utf8() {
        assert!(check_init_script(b"#!/bin/sh\n\0bad", "x").is_err());
        assert!(check_init_script(b"#!/bin/sh\n\xff\xfe", "x").is_err());
        assert!(check_init_script(b"#!", "x").is_err()); // 2-byte
    }

    #[test]
    fn install_plan_rejects_wrong_arch_and_unhooked_scripts() {
        let d = tmp();
        write_artifacts(&d);
        // Wrong-arch daemon.
        std::fs::write(d.join("daemon"), [0u8; 2048]).unwrap();
        assert!(plan_res(&d).is_err());
        std::fs::write(d.join("daemon"), fake_arm_elf()).unwrap();
        // misc-daemon that does not reference rayhunter_daemon.
        std::fs::write(d.join("misc-daemon"), b"#!/bin/sh\nnohook\n").unwrap();
        match plan_res(&d) {
            Err(e) => assert!(e.contains("misc-daemon does not reference")),
            Ok(_) => panic!("expected an error for an unhooked misc-daemon"),
        }
        let _ = std::fs::remove_dir_all(&d);
    }

    fn plan_res(d: &Path) -> Result<Vec<Step>, String> {
        install_plan(
            &d.join("daemon"),
            &d.join("rayhunter_daemon"),
            &d.join("misc-daemon"),
            &d.join("config.toml.in"),
        )
    }

    fn plan(d: &Path) -> Vec<Step> {
        install_plan(
            &d.join("daemon"),
            &d.join("rayhunter_daemon"),
            &d.join("misc-daemon"),
            &d.join("config.toml.in"),
        )
        .unwrap()
    }

    #[test]
    fn install_plan_order_paths_and_validation() {
        let d = tmp();
        write_artifacts(&d);
        let steps = plan(&d);
        let descs: Vec<String> = steps.iter().map(|s| s.describe()).collect();
        assert!(descs[0].contains("mkdir -p /data/rayhunter"));
        assert!(descs.last().unwrap().contains("reboot"));
        assert!(
            descs
                .iter()
                .any(|x| x.contains("/data/rayhunter/rayhunter-daemon"))
        );
        assert!(descs.iter().any(|x| x.contains("/etc/init.d/misc-daemon")));
        // No device cp stash step.
        assert!(!descs.iter().any(|x| x.contains("misc-daemon.orig")));
        // A non-shebang init script is rejected.
        std::fs::write(d.join("rayhunter_daemon"), b"oops\n").unwrap();
        assert!(
            install_plan(
                &d.join("daemon"),
                &d.join("rayhunter_daemon"),
                &d.join("misc-daemon"),
                &d.join("config.toml.in")
            )
            .is_err()
        );
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn execute_refuses_when_gate_fails() {
        let mut sh = FakeShell::default();
        let steps = uninstall_plan(b"#!/bin/sh\nmisc\n".to_vec());
        let err = execute(&mut sh, &steps, || Err("no backup".into()), true).unwrap_err();
        assert!(err.contains("backup gate not satisfied"));
        assert!(sh.fs.is_empty());
    }

    #[test]
    fn execute_dry_run_writes_nothing() {
        let mut sh = FakeShell::default();
        let d = tmp();
        write_artifacts(&d);
        execute(&mut sh, &plan(&d), || Ok(()), false).unwrap();
        assert!(sh.fs.is_empty());
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn execute_apply_deploys_atomically_and_verifies() {
        let mut sh = FakeShell::default();
        let d = tmp();
        write_artifacts(&d);
        execute(&mut sh, &plan(&d), || Ok(()), true).unwrap();
        // Atomic Put stages to <remote>.orx-new then `mv`s; the FakeShell mv is a
        // run() no-op, so the staged temp is what persists -- assert on it.
        assert_eq!(
            sh.fs
                .get("/data/rayhunter/rayhunter-daemon.orx-new")
                .unwrap()
                .len(),
            2048
        );
        assert!(sh.fs.contains_key("/etc/init.d/misc-daemon.orx-new"));
        assert!(sh.fs.contains_key("/data/rayhunter/config.toml.orx-new"));
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn stock_misc_daemon_extracts_and_rejects_hooked() {
        let d = tmp();
        // Build a root.tar containing ./etc/init.d/misc-daemon via host tar.
        let make_tar = |body: &str| {
            let root = d.join("tarroot");
            let p = root.join("etc/init.d");
            std::fs::create_dir_all(&p).unwrap();
            std::fs::write(p.join("misc-daemon"), body).unwrap();
            let status = std::process::Command::new("tar")
                .arg("-cf")
                .arg(d.join("root.tar"))
                .arg("-C")
                .arg(&root)
                .arg("./etc/init.d/misc-daemon")
                .status()
                .unwrap();
            assert!(status.success());
            let _ = std::fs::remove_dir_all(&root);
        };
        // Stock (unhooked) -> extracted.
        make_tar("#!/bin/sh\nstart() { echo hi; }\n");
        assert!(stock_misc_daemon(&d).unwrap().starts_with(b"#!/bin/sh"));
        // Hooked (references rayhunter_daemon) -> rejected.
        make_tar("#!/bin/sh\nstart() { /etc/init.d/rayhunter_daemon start; }\n");
        assert!(
            stock_misc_daemon(&d)
                .unwrap_err()
                .contains("already hooked")
        );
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
