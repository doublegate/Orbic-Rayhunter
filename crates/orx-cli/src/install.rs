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
use std::time::Duration;

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
    if bytes.len() < 10 || !bytes.starts_with(b"#!") {
        return Err(format!(
            "{what} is not a plausible shell script (too short or no #! shebang)"
        ));
    }
    // The first line is the shebang and must invoke /bin/sh (BusyBox on the
    // RC400L has no /bin/bash); there must be a line after it.
    let first_line_end = bytes
        .iter()
        .position(|&b| b == b'\n')
        .ok_or_else(|| format!("{what} has no newline after the shebang"))?;
    if !contains(&bytes[..first_line_end], b"/bin/sh") {
        return Err(format!("{what} shebang does not invoke /bin/sh"));
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
    if bytes.len() < 52 || &bytes[..4] != b"\x7fELF" || bytes[4] != 1 || bytes[5] != 1 {
        return false;
    }
    let e_type = u16::from_le_bytes([bytes[16], bytes[17]]);
    let e_machine = u16::from_le_bytes([bytes[18], bytes[19]]);
    // ET_EXEC (2) or ET_DYN (3) -- not an ET_REL object; EM_ARM (40).
    (e_type == 2 || e_type == 3) && e_machine == 40
}

/// Inspect a 32-bit LE ELF's section table for a symbol table (`SHT_SYMTAB`):
/// - `Some(true)`  -- has a `SHT_SYMTAB` (NOT stripped -- a devel build);
/// - `Some(false)` -- no section table (`e_shoff == 0`) or none is `SHT_SYMTAB`
///   (a stripped release build);
/// - `None`        -- the section table is malformed or points past EOF (a
///   truncated/corrupt binary) -- the caller must fail closed.
fn elf32_symbol_table(bytes: &[u8]) -> Option<bool> {
    // Every read is bounds-checked and returns None (fail closed) on any surprise.
    let rd_u32 = |o: usize| -> Option<u32> {
        bytes
            .get(o..o.checked_add(4)?)?
            .try_into()
            .ok()
            .map(u32::from_le_bytes)
    };
    let rd_u16 = |o: usize| -> Option<u16> {
        bytes
            .get(o..o.checked_add(2)?)?
            .try_into()
            .ok()
            .map(u16::from_le_bytes)
    };
    // ELF32 header: e_shoff@0x20 (u32), e_shentsize@0x2e (u16), e_shnum@0x30 (u16).
    if bytes.len() < 0x34 {
        return None; // too short to be the ELF32 we vet -> corrupt/truncated
    }
    let e_shoff = rd_u32(0x20)? as usize;
    let e_shentsize = rd_u16(0x2e)? as usize;
    let mut e_shnum = rd_u16(0x30)? as usize;
    if e_shoff == 0 {
        // Valid only with e_shnum == 0 (genuinely no section table -> stripped);
        // e_shoff==0 with e_shnum!=0 is inconsistent.
        return if e_shnum == 0 { Some(false) } else { None };
    }
    if e_shentsize != 40 {
        return None; // Elf32_Shdr is exactly 40 bytes
    }
    // Section header 0 must be fully present before we can trust the table.
    if bytes.len() < e_shoff.checked_add(e_shentsize)? {
        return None;
    }
    if e_shnum == 0 {
        // Extended numbering (SHN_XINDEX): the real count is section 0's sh_size
        // (Elf32_Shdr offset 0x14).
        e_shnum = rd_u32(e_shoff + 0x14)? as usize;
        if e_shnum == 0 {
            return None;
        }
    }
    // The WHOLE section table must be in bounds before we scan it, so a truncated
    // table can never be partially scanned and an early SHT_SYMTAB accepted.
    let table_end = e_shoff.checked_add(e_shnum.checked_mul(e_shentsize)?)?;
    if table_end > bytes.len() {
        return None;
    }
    for i in 0..e_shnum {
        let off = e_shoff + i * e_shentsize;
        // sh_type is the u32 at offset 4 of each section header; SHT_SYMTAB == 2.
        if rd_u32(off + 4)? == 2 {
            return Some(true);
        }
    }
    Some(false)
}

/// Verify `daemon` against a `SHA256SUMS` beside it (the format `build.sh` writes:
/// `<64-hex>  <basename>`), when that file exists. Returns:
/// - `Ok(true)`  -- SHA256SUMS present and the daemon's hash matches its entry;
/// - `Ok(false)` -- no SHA256SUMS beside the daemon (cannot verify);
/// - `Err`       -- SHA256SUMS present but the daemon is absent from it or its
///   hash does not match (a corrupted or swapped artifact) -- fail closed.
pub fn verify_daemon_against_sums(daemon: &Path, daemon_bytes: &[u8]) -> Result<bool, String> {
    let sums = match daemon.parent() {
        Some(dir) => dir.join("SHA256SUMS"),
        None => return Ok(false),
    };
    // Only a genuine absence (NotFound) downgrades to "cannot verify"; a broken
    // symlink or an unreadable file must fail closed, not be treated as absent.
    match std::fs::symlink_metadata(&sums) {
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(false),
        Err(e) => return Err(format!("cannot stat {}: {e}", sums.display())),
        Ok(_) => {}
    }
    let name = daemon
        .file_name()
        .and_then(|n| n.to_str())
        .ok_or_else(|| "daemon has no file name".to_string())?;
    let text =
        String::from_utf8(read(&sums)?).map_err(|_| format!("{} is not UTF-8", sums.display()))?;
    // `sha256sum` lines are `<64-hex><sep><filename>`, where <sep> is two spaces
    // (text mode, build.sh) or a space + `*` (binary mode). The release workflow
    // runs `sha256sum -- ./*`, so entries carry a leading `./`. Match the daemon's
    // exact name, allowing only that leading `./` -- NOT an arbitrary directory
    // prefix (an entry for `other/dir/<name>` is for a different file).
    let matches: Vec<&str> = text
        .lines()
        .filter_map(|l| {
            let l = l.trim_end();
            // Check the raw bytes BEFORE slicing: `l[..64]` would panic if byte 64
            // fell mid-character. Once bytes 0..64 are all ASCII hex (single-byte
            // chars), 64 is a char boundary and split_at(64) is safe.
            let b = l.as_bytes();
            if b.len() < 66 || !b[..64].iter().all(u8::is_ascii_hexdigit) {
                return None;
            }
            let (hash, rest) = l.split_at(64);
            // The separator must be exactly two spaces (text) or space+`*`
            // (binary). This also rejects a SHA-512 line, whose 65th/66th chars
            // are hex, not a separator.
            if rest.as_bytes()[0] != b' ' || !matches!(rest.as_bytes()[1], b' ' | b'*') {
                return None;
            }
            let file = rest[2..].trim();
            let file = file.strip_prefix("./").unwrap_or(file);
            (file == name).then_some(hash)
        })
        .collect();
    let want = match matches.as_slice() {
        [] => {
            return Err(format!(
                "{} does not list {name} (stale or wrong SHA256SUMS?)",
                sums.display()
            ));
        }
        [one] => *one,
        _ => {
            return Err(format!(
                "{} has multiple entries for {name} (ambiguous); refusing",
                sums.display()
            ));
        }
    };
    let got = crate::sha256::hex(daemon_bytes);
    // sha256sum manifests may use uppercase hex; compare case-insensitively.
    if !got.eq_ignore_ascii_case(want) {
        return Err(format!(
            "{} hash {got} does not match SHA256SUMS entry {want} (corrupt or swapped artifact)",
            daemon.display()
        ));
    }
    Ok(true)
}

/// Whether `daemon_bytes` is an unstripped build (carries a symbol table): the
/// devel profile, not the stripped ~6 MB firmware release. `Err` if the ELF
/// section table is truncated/corrupt, so the caller fails closed rather than
/// treating a damaged binary as a clean stripped release.
pub fn daemon_is_unstripped(daemon_bytes: &[u8]) -> Result<bool, String> {
    // Self-contained: validate ELF identity here too, so a caller that did not go
    // through install_plan cannot feed a non-ELF buffer (e.g. all-zero bytes,
    // which have e_shoff == e_shnum == 0) and have it read as a stripped release.
    if !is_arm32_elf(daemon_bytes) {
        return Err("daemon is not a 32-bit little-endian ARM ELF".to_string());
    }
    elf32_symbol_table(daemon_bytes).ok_or_else(|| {
        "daemon ELF section table is out of bounds (truncated or corrupt)".to_string()
    })
}

/// The daemon payload captured in a plan built by `install_plan` -- the exact
/// bytes that will be pushed -- so pre-write checks verify what is deployed
/// instead of re-reading the file (no extra I/O, no TOCTOU gap).
pub fn plan_daemon_bytes(steps: &[Step]) -> Option<&[u8]> {
    steps.iter().find_map(|s| match s {
        Step::Put { remote, bytes, .. } if remote == DAEMON_REMOTE => Some(bytes.as_slice()),
        _ => None,
    })
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
    if contains(&out.stdout, b"rayhunter_daemon") {
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
            desc: "remove daemon, config, and install dirs".into(),
            command: format!(
                "rm -f {DAEMON_REMOTE} {CONFIG_REMOTE}; rm -rf /data/rayhunter/scripts /data/rayhunter/bin"
            ),
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
/// Available kibibytes parsed from a busybox `df -k <mount>`. The row layout is
/// `Filesystem 1K-blocks Used Available Use% Mounted`; a long filesystem name may
/// wrap to its own line, so anchor from the END (mount = last, Use% = last-1,
/// Available = last-2) rather than by column index.
fn parse_df_available_kb(df_output: &str) -> Option<u64> {
    let toks: Vec<&str> = df_output
        .lines()
        .skip(1) // header
        .flat_map(|l| l.split_whitespace())
        .collect();
    let n = toks.len();
    if n < 3 {
        return None;
    }
    toks[n - 3].parse::<u64>().ok()
}

/// Bytes of headroom each mount must have for the plan's writes: the sum of the
/// `Put` payloads landing on that mount (each is staged full-size as `.orx-new`
/// before the mv) plus an 8 MiB slack. Mount is derived from the target path.
fn space_needs(steps: &[Step]) -> Vec<(&'static str, u64)> {
    let mut data = 0u64;
    let mut root = 0u64;
    for s in steps {
        if let Step::Put { remote, bytes, .. } = s {
            if remote.starts_with("/data") {
                data += bytes.len() as u64;
            } else {
                root += bytes.len() as u64; // /etc/init.d/* etc. live on rootfs
            }
        }
    }
    const SLACK: u64 = 8 * 1024 * 1024;
    let mut needs = Vec::new();
    if data > 0 {
        needs.push(("/data", data + SLACK));
    }
    if root > 0 {
        needs.push(("/", root + SLACK));
    }
    needs
}

/// Refuse the write if `mount` has less than `need` bytes free on the device.
fn check_free_space(shell: &mut dyn Shell, mount: &str, need: u64) -> Result<(), String> {
    let r = shell.run(&format!("df -k {mount}"))?;
    if r.status != 0 {
        return Err(format!(
            "df {mount} failed (status {}): {}",
            r.status,
            r.output.trim()
        ));
    }
    let avail_kb = parse_df_available_kb(&r.output).ok_or_else(|| {
        format!(
            "could not parse free space for {mount}: {}",
            r.output.trim()
        )
    })?;
    let avail = avail_kb.saturating_mul(1024);
    if avail < need {
        return Err(format!(
            "{mount} has {} KiB free but the install needs {} KiB there -- free space first (#1192)",
            avail_kb,
            need / 1024
        ));
    }
    Ok(())
}

pub fn execute(
    shell: &mut dyn Shell,
    steps: &[Step],
    gate: impl FnOnce() -> Result<(), String>,
    apply: bool,
) -> Result<(), String> {
    gate().map_err(|e| format!("backup gate not satisfied, refusing to proceed: {e}"))?;
    // Free-space preflight before any write (apply only): a full /data wedged the
    // daemon and the hotspot (#1192), and the atomic staging writes a full copy of
    // each file to `<path>.orx-new` before the mv.
    if apply {
        for (mount, need) in space_needs(steps) {
            check_free_space(shell, mount, need)?;
        }
    }
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
                // execute is pub; defend the interpolated mode even though the
                // plans only ever pass "755"/"644".
                if !((3..=4).contains(&chmod.len())
                    && chmod.bytes().all(|b| (b'0'..=b'7').contains(&b)))
                {
                    return Err(format!("invalid chmod mode in plan: {chmod:?}"));
                }
                let tmp = format!("{remote}.orx-new");
                // cleanup helper: a partial/failed write must never leave a stray
                // temp (a 16 MB daemon temp on /data can wedge the daemon+AP, #1192).
                let cleanup = |sh: &mut dyn Shell| {
                    let _ = sh.run(&format!("rm -f {tmp}"));
                };
                if let Err(e) = shell.send_file(&tmp, bytes) {
                    cleanup(shell);
                    return Err(e);
                }
                match shell.run(&format!("chmod {chmod} {tmp}")) {
                    Ok(c) if c.status == 0 => {}
                    Ok(c) => {
                        cleanup(shell);
                        return Err(format!("chmod {chmod} {tmp} failed: {}", c.output.trim()));
                    }
                    Err(e) => {
                        cleanup(shell);
                        return Err(format!("chmod {chmod} {tmp}: {e}"));
                    }
                }
                match shell.run(&format!("mv -f {tmp} {remote}")) {
                    Ok(m) if m.status == 0 => {}
                    Ok(m) => {
                        cleanup(shell);
                        return Err(format!("mv {tmp} {remote} failed: {}", m.output.trim()));
                    }
                    Err(e) => {
                        cleanup(shell);
                        return Err(format!("mv {tmp} {remote}: {e}"));
                    }
                }
            }
            Step::Cmd { command, .. } => {
                if command.trim() == "reboot" {
                    // reboot drops the link without a clean close: a disconnect or
                    // read timeout is the EXPECTED success. Bound it so orx cannot
                    // block for the long transfer timeout, and surface only an
                    // explicit non-zero exit as a failure.
                    shell.set_timeout(Duration::from_secs(10));
                    match shell.run("reboot") {
                        Ok(r) if r.status != 0 => {
                            return Err(format!(
                                "reboot returned status {}: {}",
                                r.status,
                                r.output.trim()
                            ));
                        }
                        _ => println!("  device rebooting"),
                    }
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
    use crate::transport::CmdResult;
    use crate::transport::fake::FakeShell;

    /// A FakeShell that answers `df -k` with ample free space on /data and /.
    fn shell_with_space() -> FakeShell {
        let mut sh = FakeShell::default();
        let mk = |avail_kb: u64, mount: &str| CmdResult {
            output: format!(
                "Filesystem 1K-blocks Used Available Use% Mounted on\nubi0:x 500000 1000 {avail_kb} 1% {mount}\n"
            ),
            status: 0,
        };
        sh.responses
            .insert("df -k /data".into(), mk(150_000, "/data"));
        sh.responses.insert("df -k /".into(), mk(40_000, "/"));
        sh
    }

    #[test]
    fn parse_df_available_handles_normal_and_wrapped() {
        let normal = "Filesystem 1K-blocks Used Available Use% Mounted on\nubi0:data 200000 50000 150000 25% /data\n";
        assert_eq!(parse_df_available_kb(normal), Some(150000));
        // busybox wraps a long filesystem name onto its own line.
        let wrapped = "Filesystem 1K-blocks Used Available Use% Mounted on\nubi0:userdata-very-long-name\n 200000 50000 150000 25% /data\n";
        assert_eq!(parse_df_available_kb(wrapped), Some(150000));
        assert_eq!(parse_df_available_kb("garbage"), None);
    }

    #[test]
    fn execute_refuses_on_insufficient_space() {
        let mut sh = FakeShell::default();
        sh.responses.insert(
            "df -k /data".into(),
            CmdResult {
                output: "Filesystem 1K-blocks Used Available Use% Mounted on\nubi0:x 200000 199000 1000 99% /data\n".into(),
                status: 0,
            },
        );
        let d = tmp();
        write_artifacts(&d);
        let err = execute(&mut sh, &plan(&d), || Ok(()), true).unwrap_err();
        assert!(err.contains("free") && err.contains("/data"), "got: {err}");
        assert!(sh.fs.is_empty()); // refused before any write
        let _ = std::fs::remove_dir_all(&d);
    }

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
        b[16] = 2; // e_type = ET_EXEC
        b[18] = 40; // EM_ARM (little end of e_machine)
        b
    }

    /// An ARM32 ELF whose section-header table has, optionally, a SHT_SYMTAB.
    fn elf_with_sections(with_symtab: bool) -> Vec<u8> {
        let mut b = fake_arm_elf();
        let shoff = 512usize;
        let shentsize = 40usize;
        let n = if with_symtab { 2 } else { 1 };
        b[0x20..0x24].copy_from_slice(&(shoff as u32).to_le_bytes()); // e_shoff
        b[0x2e..0x30].copy_from_slice(&(shentsize as u16).to_le_bytes()); // e_shentsize
        b[0x30..0x32].copy_from_slice(&(n as u16).to_le_bytes()); // e_shnum
        // Section 0: NULL (sh_type 0); section 1 (optional): SHT_SYMTAB (2).
        if with_symtab {
            let off = shoff + shentsize; // second header
            b[off + 4..off + 8].copy_from_slice(&2u32.to_le_bytes());
        }
        b
    }

    #[test]
    fn elf32_symbol_table_detection() {
        assert_eq!(elf32_symbol_table(&elf_with_sections(true)), Some(true));
        assert_eq!(elf32_symbol_table(&elf_with_sections(false)), Some(false));
        assert_eq!(elf32_symbol_table(&fake_arm_elf()), Some(false)); // no section headers
        // A section table pointing past EOF is corrupt -> None (caller fails closed).
        let mut trunc = elf_with_sections(true);
        trunc[0x20..0x24].copy_from_slice(&0x7fff_ffffu32.to_le_bytes()); // e_shoff past EOF
        assert_eq!(elf32_symbol_table(&trunc), None);
        assert!(daemon_is_unstripped(&trunc).is_err());
        // Too short to be a valid ELF32 we vet -> None (not Some(false)).
        assert_eq!(elf32_symbol_table(&[0u8; 20]), None);
        // e_shentsize != 40 -> None.
        let mut bad_ent = elf_with_sections(true);
        bad_ent[0x2e..0x30].copy_from_slice(&8u16.to_le_bytes());
        assert_eq!(elf32_symbol_table(&bad_ent), None);
        // A huge e_shnum pushing the table past EOF -> None, even though a
        // SHT_SYMTAB sits early (no partial scan / early accept).
        let mut big = elf_with_sections(true);
        big[0x30..0x32].copy_from_slice(&0xffffu16.to_le_bytes());
        assert_eq!(elf32_symbol_table(&big), None);
        // e_shoff == 0 but e_shnum != 0 is inconsistent -> None.
        let mut incons = fake_arm_elf();
        incons[0x30..0x32].copy_from_slice(&3u16.to_le_bytes());
        assert_eq!(elf32_symbol_table(&incons), None);
        // daemon_is_unstripped validates ELF identity itself: a non-ELF buffer
        // (e.g. all zero) is rejected, not read as a clean stripped release.
        assert!(daemon_is_unstripped(&[0u8; 2048]).is_err());
        assert_eq!(daemon_is_unstripped(&fake_arm_elf()), Ok(false));
        assert_eq!(daemon_is_unstripped(&elf_with_sections(true)), Ok(true));
    }

    #[test]
    fn verify_daemon_against_sums_cases() {
        let d = tmp();
        let daemon = d.join("rayhunter-daemon");
        std::fs::write(&daemon, [7u8; 2048]).unwrap();
        let bytes = [7u8; 2048];
        // No SHA256SUMS -> Ok(false) (cannot verify).
        assert_eq!(verify_daemon_against_sums(&daemon, &bytes), Ok(false));
        // Matching entry -> Ok(true), across the formats sha256sum / the release
        // workflow (`./` prefix) / binary mode (`*`) produce.
        let good = crate::sha256::hex(&[7u8; 2048]);
        for line in [
            format!("{good}  rayhunter-daemon\n"),   // build.sh text mode
            format!("{good}  ./rayhunter-daemon\n"), // release workflow (sha256sum -- ./*)
            format!("{good} *rayhunter-daemon\n"),   // binary mode
        ] {
            std::fs::write(d.join("SHA256SUMS"), &line).unwrap();
            assert_eq!(
                verify_daemon_against_sums(&daemon, &bytes),
                Ok(true),
                "line: {line:?}"
            );
        }
        // Wrong hash -> Err.
        std::fs::write(
            d.join("SHA256SUMS"),
            format!("{}  rayhunter-daemon\n", "0".repeat(64)),
        )
        .unwrap();
        assert!(verify_daemon_against_sums(&daemon, &bytes).is_err());
        // Present but daemon not listed -> Err.
        std::fs::write(d.join("SHA256SUMS"), format!("{good}  something-else\n")).unwrap();
        assert!(verify_daemon_against_sums(&daemon, &bytes).is_err());
        // A corrupt line with a multi-byte char straddling byte 64 must not panic
        // (fail-closed on file input); the valid line is still found.
        let junk = format!("{}\u{00e9}junk\n", "a".repeat(63)); // 'é' spans bytes 63-64
        std::fs::write(
            d.join("SHA256SUMS"),
            format!("{junk}{good}  rayhunter-daemon\n"),
        )
        .unwrap();
        assert_eq!(verify_daemon_against_sums(&daemon, &bytes), Ok(true));
        // Uppercase hex digest still matches (case-insensitive).
        std::fs::write(
            d.join("SHA256SUMS"),
            format!("{}  rayhunter-daemon\n", good.to_uppercase()),
        )
        .unwrap();
        assert_eq!(verify_daemon_against_sums(&daemon, &bytes), Ok(true));
        // Duplicate entries for the daemon -> ambiguous -> Err.
        std::fs::write(
            d.join("SHA256SUMS"),
            format!("{good}  rayhunter-daemon\n{good}  ./rayhunter-daemon\n"),
        )
        .unwrap();
        assert!(verify_daemon_against_sums(&daemon, &bytes).is_err());
        // A SHA-512 line (128 hex, no valid 2-char separator at byte 64) is not a match.
        std::fs::write(
            d.join("SHA256SUMS"),
            format!("{}  rayhunter-daemon\n", "a".repeat(128)),
        )
        .unwrap();
        assert!(verify_daemon_against_sums(&daemon, &bytes).is_err());
        // An entry under a different directory (not just a leading `./`) is for a
        // different file -> not a match -> Err.
        std::fs::write(
            d.join("SHA256SUMS"),
            format!("{good}  other/dir/rayhunter-daemon\n"),
        )
        .unwrap();
        assert!(verify_daemon_against_sums(&daemon, &bytes).is_err());
        // A broken-symlink SHA256SUMS must fail closed (not be treated as absent).
        #[cfg(unix)]
        {
            let link = d.join("SHA256SUMS");
            let _ = std::fs::remove_file(&link);
            std::os::unix::fs::symlink(d.join("does-not-exist"), &link).unwrap();
            assert!(verify_daemon_against_sums(&daemon, &bytes).is_err());
            let _ = std::fs::remove_file(&link);
        }
        let _ = std::fs::remove_dir_all(&d);
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
        let mut sh = shell_with_space();
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

    #[test]
    fn execute_apply_issues_expected_command_sequence() {
        let mut sh = shell_with_space();
        let d = tmp();
        write_artifacts(&d);
        execute(&mut sh, &plan(&d), || Ok(()), true).unwrap();
        // Ignoring the sha256sum verification calls, the ordered device commands.
        let seq: Vec<&String> = sh
            .cmds
            .iter()
            .filter(|c| !c.starts_with("sha256sum") && !c.starts_with("df "))
            .collect();
        assert!(seq[0].starts_with("mkdir -p /data/rayhunter"));
        assert!(
            seq.iter()
                .any(|c| c.starts_with("chmod 755 /data/rayhunter/rayhunter-daemon.orx-new"))
        );
        assert!(
            seq.iter()
                .any(|c| c.starts_with("mv -f /data/rayhunter/rayhunter-daemon.orx-new"))
        );
        assert_eq!(*seq[seq.len() - 2], "sync");
        assert_eq!(*seq[seq.len() - 1], "reboot");
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn execute_cleans_up_temp_on_failed_send_file() {
        let mut sh = shell_with_space();
        sh.drop_writes = true; // put drops data -> device sha256 mismatch -> send_file fails
        let d = tmp();
        write_artifacts(&d);
        assert!(execute(&mut sh, &plan(&d), || Ok(()), true).is_err());
        assert!(
            sh.cmds
                .iter()
                .any(|c| c.starts_with("rm -f ") && c.contains(".orx-new"))
        );
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn execute_rejects_unsafe_remote_and_bad_mode() {
        let mut bad_path = shell_with_space();
        let steps = vec![Step::Put {
            desc: "x".into(),
            remote: "/etc/init.d/misc-daemon; rm -rf /".into(),
            bytes: vec![0u8; 16],
            chmod: "755",
        }];
        assert!(execute(&mut bad_path, &steps, || Ok(()), true).is_err());
        assert!(bad_path.fs.is_empty());

        let mut bad_mode = shell_with_space();
        let steps = vec![Step::Put {
            desc: "x".into(),
            remote: "/data/rayhunter/x".into(),
            bytes: vec![0u8; 16],
            chmod: "7o5",
        }];
        assert!(execute(&mut bad_mode, &steps, || Ok(()), true).is_err());
    }

    #[test]
    fn uninstall_apply_restores_stock_and_removes_our_files() {
        let mut sh = shell_with_space();
        let steps = uninstall_plan(b"#!/bin/sh\nstock\n".to_vec());
        execute(&mut sh, &steps, || Ok(()), true).unwrap();
        assert!(sh.fs.contains_key("/etc/init.d/misc-daemon.orx-new"));
        assert!(
            sh.cmds
                .iter()
                .any(|c| c.contains("rm -f /etc/init.d/rayhunter_daemon"))
        );
        assert!(
            sh.cmds
                .iter()
                .any(|c| c.contains("rm -rf /data/rayhunter/scripts"))
        );
        assert_eq!(sh.cmds.last().unwrap(), "reboot");
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
