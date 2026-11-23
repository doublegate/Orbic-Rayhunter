use std::fs;
use std::io::Write;
use std::os::unix::fs::OpenOptionsExt;
use std::path::Path;
use std::process::ExitCode;
use std::time::Duration;

use orx_cli::{backup, install, probe, transport, usb, verify};
use transport::Shell;

const USAGE: &str = "\
orx - Orbic-Rayhunter host helper

USAGE:
    orx doctor [--ip <addr>] [--json]    Detect the Orbic over USB and probe its web UIs
    orx backup --out <dir> [--ip <addr>] [--allow-live]
                                         Archive each writable device filesystem (tar+SHA-256)
    orx verify --dir <dir>               Re-hash a backup dir against its MANIFEST.txt
    orx install --dir <backup> [--dist <d>] [--daemon <f>] [--ip <addr>] [--apply]
                                         Deploy our firmware build (dry-run unless --apply)
    orx uninstall --dir <backup> [--ip <addr>] [--apply]
                                         Reverse the install (dry-run unless --apply)
    orx --version
    orx --help

Notes:
    install/uninstall WRITE to the device and reboot it; they are dry-run unless --apply,
    and always refuse unless `orx verify --dir <backup>` would pass. --dist defaults to
    build/rayhunter/dist, --daemon to dist/rayhunter-daemon.
    backup needs the device root shell open on TCP 24 (see docs/BACKUP-AND-RECOVERY.md).
    It writes <dir>/<fs>.tar (0600) and <dir>/MANIFEST.txt; a live rw filesystem whose
    tar exits non-zero is only accepted with --allow-live (recorded as ok-live).

EXIT CODES:
    0   success (doctor: something found; backup: every target archived)
    1   doctor: nothing found; backup: a target failed
    2   usage error, or the host environment could not be read";

/// `orx backup`: archive every writable device filesystem to `out_dir`.
fn cmd_backup(ip: &str, out_dir: &Path, allow_live: bool) -> ExitCode {
    let addr = match probe::socket(ip, 24) {
        Ok(a) => a,
        Err(e) => {
            eprintln!("error: {e}");
            return ExitCode::from(2);
        }
    };
    // tar of a whole filesystem streams for a while; give the transfer room but
    // still bound a true stall.
    let mut shell = transport::TcpShell::new(addr, Duration::from_secs(600));

    let mounts_raw = match shell.run("cat /proc/mounts") {
        Ok(r) if r.status == 0 => r.output,
        Ok(r) => {
            eprintln!(
                "error: reading /proc/mounts failed (status {}): is the shell open on {addr}?",
                r.status
            );
            return ExitCode::from(1);
        }
        Err(e) => {
            eprintln!(
                "error: {e} (is the root shell open on {addr}? see docs/BACKUP-AND-RECOVERY.md)"
            );
            return ExitCode::from(1);
        }
    };
    let mounts = backup::parse_mounts(&mounts_raw);
    let all_mps: Vec<String> = mounts.iter().map(|m| m.mountpoint.clone()).collect();
    let targets = backup::backup_targets(&mounts);
    if targets.is_empty() {
        eprintln!("error: no writable filesystems found to back up");
        return ExitCode::from(1);
    }

    if let Err(e) = fs::create_dir_all(out_dir) {
        eprintln!("error: cannot create {}: {e}", out_dir.display());
        return ExitCode::from(2);
    }

    let mut lines: Vec<String> = Vec::new();
    let mut failed = false;
    for t in &targets {
        let name = backup::archive_name(&t.mountpoint);
        let path = out_dir.join(format!("{name}.tar"));
        // Write to an O_EXCL temp (no symlink follow, fresh 0600), then rename
        // over the target. A rejected archive leaves no final file.
        let (mut file, tmp) = match open_tmp(&path) {
            Ok(ft) => ft,
            Err(e) => {
                eprintln!("error: cannot create temp for {}: {e}", path.display());
                failed = true;
                continue;
            }
        };
        let outcome = backup::archive_one(&mut shell, t, &all_mps, &mut file).and_then(
            |(bytes, hash, status)| {
                backup::classify(status, bytes, t.writable, allow_live)
                    .map(|state| (bytes, hash, state))
            },
        );
        drop(file);
        match outcome {
            Ok((bytes, hash, state)) => {
                if let Err(e) = fs::rename(&tmp, &path) {
                    let _ = fs::remove_file(&tmp);
                    eprintln!("  FAIL       {}: rename: {e}", t.mountpoint);
                    failed = true;
                    continue;
                }
                let res = backup::ArchiveResult {
                    mountpoint: t.mountpoint.clone(),
                    fstype: t.fstype.clone(),
                    bytes,
                    sha256: hash,
                    state: state.clone(),
                };
                println!(
                    "  {state:10} {} -> {} ({bytes} B)",
                    t.mountpoint,
                    path.display()
                );
                lines.push(res.manifest_line());
            }
            Err(e) => {
                let _ = fs::remove_file(&tmp);
                eprintln!("  FAIL       {}: {e}", t.mountpoint);
                failed = true;
            }
        }
    }

    // A partial backup must NOT leave a gate-passable MANIFEST.txt: the write
    // gates only look for a `/` ok line, so a manifest from a run where another
    // filesystem failed would still unlock device writes. On failure, remove any
    // stale MANIFEST.txt and write MANIFEST.incomplete.txt instead.
    let manifest = if failed {
        let stale = out_dir.join("MANIFEST.txt");
        if stale.exists() {
            let _ = fs::remove_file(&stale);
        }
        out_dir.join("MANIFEST.incomplete.txt")
    } else {
        out_dir.join("MANIFEST.txt")
    };
    let body = format!(
        "# orx backup {}{}\n# <mountpoint> <fstype> size=<bytes> sha256=<hex> <state>\n{}\n",
        ip,
        if failed {
            " (INCOMPLETE: a target failed)"
        } else {
            ""
        },
        lines.join("\n")
    );
    match open_tmp(&manifest) {
        Ok((mut f, tmp)) => {
            let w = f.write_all(body.as_bytes());
            drop(f);
            if let Err(e) = w.and_then(|()| fs::rename(&tmp, &manifest)) {
                let _ = fs::remove_file(&tmp);
                eprintln!("error: writing {}: {e}", manifest.display());
                return ExitCode::from(2);
            }
        }
        Err(e) => {
            eprintln!("error: cannot write {}: {e}", manifest.display());
            return ExitCode::from(2);
        }
    }
    println!("manifest: {}", manifest.display());
    if failed {
        eprintln!("backup INCOMPLETE: a target failed; no gate-passable MANIFEST.txt written");
        ExitCode::from(1)
    } else {
        ExitCode::SUCCESS
    }
}

/// Create a fresh 0600 temp file beside `final_path` with `O_EXCL` (so it never
/// follows or clobbers a pre-existing path, e.g. a planted symlink). The caller
/// writes to it and renames it over `final_path`; `rename` replaces the target
/// (symlink included) atomically, so device data is never written through a link.
fn open_tmp(final_path: &Path) -> std::io::Result<(fs::File, std::path::PathBuf)> {
    let name = final_path
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("out");
    let tmp = final_path.with_file_name(format!(".{name}.{}.tmp", std::process::id()));
    let f = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(&tmp)?;
    Ok((f, tmp))
}

/// `orx verify`: re-check a backup directory against its MANIFEST.txt.
fn cmd_verify(dir: &Path) -> ExitCode {
    let manifest = dir.join("MANIFEST.txt");
    // Hardened read: rejects a symlink/non-regular manifest and caps the size.
    let text = match verify::read_manifest_text(&manifest) {
        Ok(t) => t,
        Err(e) => {
            eprintln!("error: {e}");
            return ExitCode::from(2);
        }
    };
    // A manifest that does not parse is itself a failed-closed integrity signal.
    let entries = match verify::parse_manifest(&text) {
        Ok(e) if !e.is_empty() => e,
        Ok(_) => {
            eprintln!("error: {} has no filesystem entries", manifest.display());
            return ExitCode::from(2);
        }
        Err(e) => {
            eprintln!("error: {} is malformed: {e}", manifest.display());
            return ExitCode::from(2);
        }
    };
    // Hash every archive once, then share the single gate policy (no re-hash).
    let verdicts = verify::verify_all(dir, &entries);
    let mut bad = false;
    for (e, v) in entries.iter().zip(&verdicts) {
        let state_ok = e.state == "ok" || e.state == "ok-live";
        // An entry passes only if its archive verifies AND its recorded state is
        // ok/ok-live: a `failed`/`ABORT` entry is never a backup, even if a stale
        // archive still hashes.
        if !v.is_ok() || !state_ok {
            bad = true;
        }
        let note = if state_ok {
            ""
        } else {
            "  <-- state not ok/ok-live"
        };
        println!(
            "  {:14} {} [{}]{}",
            v.describe(),
            e.mountpoint,
            e.state,
            note
        );
    }
    let gate = verify::gate_from_verdicts(&entries, &verdicts);
    match (&gate, bad) {
        (Ok(()), false) => println!("gate: all archives verified -- device writes may proceed"),
        // The rootfs gate can pass while a non-root archive failed; say so plainly
        // rather than printing "may proceed" next to a non-zero exit.
        (Ok(()), true) => println!(
            "gate: rootfs gate satisfied, but some archives failed above -- NOT a complete backup"
        ),
        (Err(why), _) => println!("gate: NOT satisfied -- {why}"),
    }
    if bad || gate.is_err() {
        ExitCode::from(1)
    } else {
        ExitCode::SUCCESS
    }
}

/// Print a usage error and return exit code 2.
fn usage_err(msg: &str) -> ExitCode {
    eprintln!("error: {msg}\n\n{USAGE}");
    ExitCode::from(2)
}

/// `orx install`: deploy our firmware build to the device (dry-run unless --apply).
fn cmd_install(backup: &Path, dist: &Path, daemon: &Path, ip: &str, apply: bool) -> ExitCode {
    let init = dist.join("scripts/rayhunter_daemon");
    let misc = dist.join("scripts/misc-daemon");
    let config = dist.join("config.toml.in");
    let steps = match install::install_plan(daemon, &init, &misc, &config) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("error: {e}");
            return ExitCode::from(2);
        }
    };
    // Refuse to install unless the backup can roll the install BACK: gate first,
    // then confirm it holds an unhooked stock misc-daemon (`orx uninstall`'s
    // restore source). A backup taken after an earlier install would pass the
    // gate but leave no clean rollback -- fail closed here, before any write.
    if let Err(e) = verify::gate(backup) {
        eprintln!("error: backup gate not satisfied, refusing to install: {e}");
        return ExitCode::from(1);
    }
    if let Err(e) = install::stock_misc_daemon(backup) {
        eprintln!(
            "error: backup has no stock misc-daemon to roll back to, refusing to install: {e}"
        );
        return ExitCode::from(1);
    }
    run_plan(&steps, backup, ip, apply, "install")
}

/// `orx uninstall`: reverse the install (restore stock misc-daemon, remove our
/// files), dry-run unless --apply.
fn cmd_uninstall(backup: &Path, ip: &str, apply: bool) -> ExitCode {
    // Gate FIRST, before touching root.tar: a tampered/corrupt archive must be
    // refused by the backup gate, not handed to `tar`. (execute re-checks the
    // gate; the extra rootfs hash on this rare op is acceptable.)
    if let Err(e) = verify::gate(backup) {
        eprintln!("error: backup gate not satisfied, refusing to uninstall: {e}");
        return ExitCode::from(1);
    }
    // The true stock misc-daemon comes from the (now verified) backup, not the
    // device (which may carry the hooked script).
    let stock = match install::stock_misc_daemon(backup) {
        Ok(b) => b,
        Err(e) => {
            eprintln!("error: {e}");
            return ExitCode::from(1); // same as cmd_install: a refusal, not a usage error
        }
    };
    run_plan(
        &install::uninstall_plan(stock),
        backup,
        ip,
        apply,
        "uninstall",
    )
}

fn run_plan(steps: &[install::Step], backup: &Path, ip: &str, apply: bool, what: &str) -> ExitCode {
    let addr = match probe::socket(ip, 24) {
        Ok(a) => a,
        Err(e) => {
            eprintln!("error: {e}");
            return ExitCode::from(2);
        }
    };
    let mut shell = transport::TcpShell::new(addr, Duration::from_secs(600));
    if apply {
        println!("APPLYING {what} (device will be written to and rebooted):");
    } else {
        println!("DRY RUN of {what} (pass --apply to execute); the backup gate is still checked:");
    }
    match install::execute(&mut shell, steps, || verify::gate(backup), apply) {
        Ok(()) => {
            if apply {
                println!("{what} applied.");
            } else {
                println!("(dry run complete; backup gate satisfied)");
            }
            ExitCode::SUCCESS
        }
        Err(e) => {
            eprintln!("error: {e}");
            ExitCode::from(1)
        }
    }
}

/// Escape a string for embedding in the compact JSON `--json` output.
fn json_escape(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 2);
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if (c as u32) < 0x20 => out.push_str(&format!("\\u{:04x}", c as u32)),
            c => out.push(c),
        }
    }
    out
}

fn opt_json(v: Option<&str>) -> String {
    v.map_or_else(|| "null".to_string(), |s| format!("\"{}\"", json_escape(s)))
}

/// A device-reported version is trusted only if it is a plain version token:
/// 1-64 chars of `[0-9A-Za-z.+-]`. Anything else (control chars, escape
/// sequences, overlong input) is dropped rather than printed.
fn sane_version(v: &str) -> bool {
    !v.is_empty()
        && v.len() <= 64
        && v.chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '+' | '-'))
}

fn doctor(ip: &str, json: bool) -> ExitCode {
    let (stock, rh) = match (
        probe::socket(ip, probe::STOCK_UI_PORT),
        probe::socket(ip, probe::RAYHUNTER_PORT),
    ) {
        (Ok(a), Ok(b)) => (a, b),
        (Err(e), _) | (_, Err(e)) => {
            eprintln!("error: {e}");
            return ExitCode::from(2);
        }
    };

    let devices = match usb::scan(Path::new("/sys/bus/usb/devices")) {
        Ok(d) => d,
        Err(e) => {
            eprintln!("error: cannot read /sys/bus/usb/devices: {e}");
            return ExitCode::from(2);
        }
    };

    let t = Duration::from_secs(2);
    let stock_up = probe::tcp_open(stock, t);
    let rh_up = probe::tcp_open(rh, t);
    // Read the running Rayhunter version only when the port answers; a failure
    // (version None) is informational, never an error.
    let rh_version = if rh_up {
        probe::http_get(rh, "/api/system-stats", t)
            .ok()
            .and_then(|b| probe::json_str_field(&b, "rayhunter_version"))
            // The version is attacker-influenceable (a compromised daemon or a
            // MITM on the local link), so reject anything that is not a plain
            // version token before it reaches a terminal or JSON output.
            .filter(|v| sane_version(v))
    } else {
        None
    };

    if json {
        let devs: Vec<String> = devices
            .iter()
            .map(|d| {
                format!(
                    "{{\"sysfs\":\"{}\",\"vid\":\"{:04x}\",\"pid\":\"{:04x}\",\"product\":{},\"serial\":{},\"mode\":\"{}\"}}",
                    json_escape(&d.sysfs_name),
                    d.vid,
                    d.pid,
                    opt_json(d.product.as_deref()),
                    opt_json(d.serial.as_deref()),
                    json_escape(&d.mode.describe()),
                )
            })
            .collect();
        println!(
            "{{\"usb\":[{}],\"stock_ui\":{{\"addr\":\"{stock}\",\"reachable\":{stock_up}}},\"rayhunter\":{{\"addr\":\"{rh}\",\"reachable\":{rh_up},\"version\":{}}}}}",
            devs.join(","),
            opt_json(rh_version.as_deref()),
        );
    } else {
        if devices.is_empty() {
            println!("usb:        no Qualcomm (05c6) device found");
        }
        for d in &devices {
            println!(
                "usb:        {} {:04x}:{:04x} {} serial={} mode={}",
                d.sysfs_name,
                d.vid,
                d.pid,
                // `product` is the model only; the serial is printed once, in its own field.
                d.product.as_deref().unwrap_or("?"),
                d.serial.as_deref().unwrap_or("?"),
                d.mode.describe()
            );
        }
        println!(
            "stock ui:   {stock} {}",
            if stock_up { "reachable" } else { "unreachable" }
        );
        let rh_state = match (rh_up, rh_version.as_deref()) {
            (true, Some(v)) => format!("reachable (Rayhunter {v})"),
            (true, None) => "reachable".to_string(),
            (false, _) => "not running / not installed".to_string(),
        };
        println!("rayhunter:  {rh} {rh_state}");
    }

    if devices.is_empty() && !stock_up && !rh_up {
        ExitCode::from(1)
    } else {
        ExitCode::SUCCESS
    }
}

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match args.first().map(String::as_str) {
        Some("doctor") => {
            let mut ip = probe::DEFAULT_DEVICE_IP.to_string();
            let mut json = false;
            let mut rest = args[1..].iter();
            while let Some(arg) = rest.next() {
                match arg.as_str() {
                    "--json" => json = true,
                    "--ip" => match rest.next() {
                        Some(v) => ip = v.clone(),
                        None => {
                            eprintln!("error: --ip needs a value\n\n{USAGE}");
                            return ExitCode::from(2);
                        }
                    },
                    other => {
                        eprintln!("error: unexpected argument {other:?}\n\n{USAGE}");
                        return ExitCode::from(2);
                    }
                }
            }
            doctor(&ip, json)
        }
        Some("backup") => {
            let mut ip = probe::DEFAULT_DEVICE_IP.to_string();
            let mut out: Option<String> = None;
            let mut allow_live = false;
            let mut rest = args[1..].iter();
            while let Some(arg) = rest.next() {
                match arg.as_str() {
                    "--allow-live" => allow_live = true,
                    "--ip" => match rest.next() {
                        Some(v) => ip = v.clone(),
                        None => {
                            eprintln!("error: --ip needs a value\n\n{USAGE}");
                            return ExitCode::from(2);
                        }
                    },
                    "--out" => match rest.next() {
                        Some(v) => out = Some(v.clone()),
                        None => {
                            eprintln!("error: --out needs a value\n\n{USAGE}");
                            return ExitCode::from(2);
                        }
                    },
                    other => {
                        eprintln!("error: unexpected argument {other:?}\n\n{USAGE}");
                        return ExitCode::from(2);
                    }
                }
            }
            match out {
                Some(dir) => cmd_backup(&ip, Path::new(&dir), allow_live),
                None => {
                    eprintln!("error: backup needs --out <dir>\n\n{USAGE}");
                    ExitCode::from(2)
                }
            }
        }
        Some("verify") => {
            let mut dir: Option<String> = None;
            let mut rest = args[1..].iter();
            while let Some(arg) = rest.next() {
                match arg.as_str() {
                    "--dir" => match rest.next() {
                        Some(v) if dir.is_some() => {
                            let _ = v;
                            eprintln!("error: --dir given more than once\n\n{USAGE}");
                            return ExitCode::from(2);
                        }
                        Some(v) if v.is_empty() => {
                            eprintln!("error: --dir value is empty\n\n{USAGE}");
                            return ExitCode::from(2);
                        }
                        Some(v) => dir = Some(v.clone()),
                        None => {
                            eprintln!("error: --dir needs a value\n\n{USAGE}");
                            return ExitCode::from(2);
                        }
                    },
                    other => {
                        eprintln!("error: unexpected argument {other:?}\n\n{USAGE}");
                        return ExitCode::from(2);
                    }
                }
            }
            match dir {
                Some(d) => cmd_verify(Path::new(&d)),
                None => {
                    eprintln!("error: verify needs --dir <dir>\n\n{USAGE}");
                    ExitCode::from(2)
                }
            }
        }
        Some(cmd @ ("install" | "uninstall")) => {
            let mut ip = probe::DEFAULT_DEVICE_IP.to_string();
            let mut backup: Option<String> = None;
            let mut dist = "build/rayhunter/dist".to_string();
            let mut daemon = "dist/rayhunter-daemon".to_string();
            let mut apply = false;
            let mut rest = args[1..].iter();
            while let Some(arg) = rest.next() {
                let need = |o: Option<&String>| -> Result<String, ()> { o.cloned().ok_or(()) };
                match arg.as_str() {
                    "--apply" => apply = true,
                    "--ip" => match need(rest.next()) {
                        Ok(v) => ip = v,
                        Err(()) => return usage_err("--ip needs a value"),
                    },
                    "--dir" => match need(rest.next()) {
                        Ok(v) => backup = Some(v),
                        Err(()) => return usage_err("--dir needs a value"),
                    },
                    "--dist" if cmd == "install" => match need(rest.next()) {
                        Ok(v) => dist = v,
                        Err(()) => return usage_err("--dist needs a value"),
                    },
                    "--daemon" if cmd == "install" => match need(rest.next()) {
                        Ok(v) => daemon = v,
                        Err(()) => return usage_err("--daemon needs a value"),
                    },
                    other => return usage_err(&format!("unexpected argument {other:?}")),
                }
            }
            let Some(backup) = backup else {
                return usage_err(&format!("{cmd} needs --dir <backup-dir>"));
            };
            if cmd == "install" {
                cmd_install(
                    Path::new(&backup),
                    Path::new(&dist),
                    Path::new(&daemon),
                    &ip,
                    apply,
                )
            } else {
                cmd_uninstall(Path::new(&backup), &ip, apply)
            }
        }
        Some("--version" | "-V") => {
            println!("orx {}", env!("CARGO_PKG_VERSION"));
            ExitCode::SUCCESS
        }
        Some("--help" | "-h") | None => {
            println!("{USAGE}");
            ExitCode::SUCCESS
        }
        Some(other) => {
            eprintln!("error: unknown command {other:?}\n\n{USAGE}");
            ExitCode::from(2)
        }
    }
}
