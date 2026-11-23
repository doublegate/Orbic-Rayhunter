#!/usr/bin/env python3
"""Back up and verify the RC400L writable filesystems (Sprint 1.3, T-OR-015..017).

This is the pre-write safety backup. It archives each mounted `ubifs` volume with
`tar` on the device, streamed over the port-24 root shell to the host, and never
writes the device.

Why tar and not a raw NAND image: `nanddump` of `/dev/mtd*` DEADLOCKS this device
after the first partition -- raw MTD access contends with the live UBI that backs
the mounted filesystems, wedging the NAND into unkillable D-state reads that need
a power-cycle. `tar` reads through the kernel VFS (the path the device itself uses
constantly), so it cannot wedge, and it is byte-deterministic on a quiescent
filesystem, so two passes can be compared. It is also the RIGHT backup for
install rollback: a Rayhunter install changes the writable `ubifs` volumes (the
daemon under `/data`, an init script on `/`), which is exactly what tar captures.
A raw partition image of the static bootloader/modem regions is a separate,
aggressive-tier concern and is not attempted here.

For each filesystem the archive is streamed to a temporary file while a rolling
SHA-256 is computed (no whole-image buffering), two independent passes must
produce the same SHA-256, and the host must be able to read the resulting tar.
Only then is it promoted atomically to its final name with mode 0600 (the
archives contain IMEI, Wi-Fi keys and other identifiers). A transfer that ends
without the end marker ABORTS the run rather than starting the next one.

Prerequisite: the root shell must already be open on TCP 24 (see
BACKUP-AND-RECOVERY.md step 1).

Usage:
    scripts/orbic-backup.py --selftest
    scripts/orbic-backup.py --proc-mounts backups/<d>/layout/<mounts>.txt \
        --out-dir backups/<d> [--ip 192.168.1.1] [--shell-port 24] \
        [--only /,/data]
"""

from __future__ import annotations

import argparse
import hashlib
import os
import re
import secrets
import shlex
import socket
import sys
import tarfile
import time
from pathlib import Path

# A mount point / exclude component may contain only these characters, so a
# crafted name from /proc/mounts can never traverse out of out_dir or inject a
# shell token.
_SAFE_PATH = re.compile(r"^/[A-Za-z0-9._/-]*$")
_SAFE_COMPONENT = re.compile(r"^[A-Za-z0-9._][A-Za-z0-9._/-]*$")
# Virtual / ephemeral filesystems never worth backing up.
_SKIP_FSTYPES = {
    "proc",
    "sysfs",
    "devtmpfs",
    "tmpfs",
    "devpts",
    "debugfs",
    "configfs",
    "functionfs",
}


def parse_mounts(text: str) -> list[tuple[str, str, str]]:
    """[(mount_point, fstype, "ro"|"rw"), ...] for EVERY mount in /proc/mounts, in
    order, one entry per mount point (the kernel lists `/` twice -- rootfs then the
    real fs; the first wins). Virtual filesystems are kept here because a parent's
    tar must still EXCLUDE them; `backup_targets` decides what is archived. The
    ro/rw flag gates the integrity verdict: a read-only mount's two passes MUST be
    byte-identical (it cannot legitimately change). When a mount point appears more
    than once (the kernel lists `/` twice, and an overmount shadows an earlier
    one), the LAST entry wins -- that is the filesystem actually visible there."""
    by_mp: dict[str, tuple[str, str, str]] = {}
    for line in text.splitlines():
        parts = line.split()
        if len(parts) >= 4 and parts[1].startswith("/"):
            opts = parts[3].split(",")
            by_mp[parts[1]] = (parts[1], parts[2], "ro" if "ro" in opts else "rw")
    return list(by_mp.values())


def backup_targets(mounts: list[tuple[str, str, str]]) -> list[tuple[str, str, str]]:
    """The real filesystems worth archiving (virtual/ephemeral ones skipped)."""
    return [m for m in mounts if m[1] not in _SKIP_FSTYPES]


def excludes_for(mount_point: str, all_mounts: list[str]) -> list[str]:
    """Other mount points nested under `mount_point`, as tar-relative components
    (`/data` under `/` -> `data`). tar at `mount_point` must skip these so it
    archives only this one filesystem, not the volumes mounted inside it."""
    base = "/" if mount_point == "/" else mount_point + "/"
    nested = []
    for mp in all_mounts:
        if mp != mount_point and mp.startswith(base):
            nested.append(mp[len(base) :])
    return sorted(nested)


def tar_command(mount_point: str, excludes: list[str]) -> str:
    """The one command sent per filesystem. The mount point and every exclude are
    validated against a strict allowlist, so there is no shell-injection or
    path-traversal path."""
    if not _SAFE_PATH.match(mount_point) or ".." in mount_point.split("/"):
        raise ValueError(f"unsafe mount point: {mount_point!r}")
    for e in excludes:
        if not _SAFE_COMPONENT.match(e) or ".." in e:
            raise ValueError(f"unsafe exclude: {e!r}")
    ex = " ".join(f"--exclude=./{e}" for e in excludes)
    mp = shlex.quote(mount_point)
    return f"tar -C {mp} {ex} -cf - . 2>/dev/null".replace("  ", " ")


def _label(mount_point: str) -> str:
    """A safe filename stem for a mount point (`/` -> `root`, `/data` -> `data`)."""
    return "root" if mount_point == "/" else mount_point.strip("/").replace("/", "-")


class ShellTar:
    """Streams one `tar` archive over a FRESH port-24 connection to a file while
    hashing it. Fresh connection per archive: busybox `nc -ll -e /bin/sh` forks a
    new shell per connection and the device streams reliably as the first command
    on a shell. Sequential (one tar at a time), so nothing contends."""

    def __init__(self, ip: str, port: int, idle_timeout: float, deadline: float):
        self.ip = ip
        self.port = port
        self.idle_timeout = idle_timeout
        self.deadline = deadline

    def dump(self, command: str, dest: Path) -> tuple[int, str, int] | None:
        """Stream `command`'s output into `dest`, hashing as it goes. The command
        is framed as `<cmd>; printf '<marker> %s.' "$?"`, so the bytes before the
        marker are the archive and the digits between the marker and the `.` are
        the command's exit status. Returns (bytes, sha256, exit_status), or None if
        the marker never arrived (a truncated transfer -> the caller ABORTS rather
        than launching another tar while this one may still run on the device)."""
        marker = secrets.token_hex(16).encode()
        keep = (
            len(marker) - 1
        )  # held back so a marker spanning recv() boundaries is caught
        sock = socket.create_connection((self.ip, self.port), timeout=10)
        sock.settimeout(self.idle_timeout)
        h = hashlib.sha256()
        total = 0
        pending = b""
        found = False
        tail = b""  # bytes after the marker: ` <status>.`
        end = time.monotonic() + self.deadline
        try:
            sock.sendall(command.encode() + b"; printf '" + marker + b' %s.\' "$?"\n')
            # Owner-only from creation: the archive holds IMEI, Wi-Fi keys and other
            # identifiers, so the temp file must never exist world-readable.
            fd = os.open(dest, os.O_WRONLY | os.O_CREAT | os.O_TRUNC, 0o600)
            os.fchmod(fd, 0o600)
            with os.fdopen(fd, "wb") as fh:
                while time.monotonic() < end:
                    try:
                        chunk = sock.recv(1 << 16)
                    except TimeoutError:
                        break
                    if not chunk:
                        break
                    if found:
                        tail += chunk
                        if b"." in tail:
                            break
                        continue
                    data = pending + chunk
                    idx = data.find(marker)
                    if idx >= 0:
                        fh.write(data[:idx])
                        h.update(data[:idx])
                        total += idx
                        found = True
                        tail = data[idx + len(marker) :]
                        if b"." in tail:
                            break
                    elif len(data) > keep:
                        emit, pending = data[:-keep], data[-keep:]
                        fh.write(emit)
                        h.update(emit)
                        total += len(emit)
                    else:
                        pending = data
        finally:
            sock.close()
        if not found:
            return None
        m = re.match(rb"\s*(\d+)\.", tail)
        status = int(m.group(1)) if m else -1  # no clean status -> treat as failure
        return (total, h.hexdigest(), status)


def validate_tar(path: Path) -> int:
    """Number of entries if `path` is a readable tar, else -1 (unreadable/corrupt)."""
    try:
        with tarfile.open(path, "r:") as tf:
            return sum(1 for _ in tf)
    except (tarfile.TarError, OSError):
        return -1


def backup_all(
    session: ShellTar,
    mounts: list[tuple[str, str, str]],
    targets: list[tuple[str, str, str]],
    out_dir: Path,
    allow_live: set[str],
) -> list[dict]:
    # Excludes are computed against EVERY mount (including virtual /proc, /sys,
    # /dev), not just the archived ones, or tar at `/` would descend into them.
    all_mps = [mp for mp, _, _ in mounts]
    results = []
    for mount_point, fstype, mode in targets:
        label = _label(mount_point)
        print(
            f"backing up {mount_point} ({fstype},{mode}) -> {label}.tar ...", flush=True
        )
        cmd = tar_command(mount_point, excludes_for(mount_point, all_mps))
        out_dir.mkdir(parents=True, exist_ok=True)
        tmp1 = out_dir / f".{label}.pass1.tmp"
        tmp2 = out_dir / f".{label}.pass2.tmp"
        final = out_dir / f"{label}.tar"

        # A missing end marker (or a dropped connection) means the transfer was cut
        # off while tar may still be running on the device: abort the whole run
        # rather than launch the next tar.
        reason = ""
        p1 = p2 = None
        size = status1 = entries1 = 0
        digest = ""
        try:
            p1 = session.dump(cmd, tmp1)
            if p1 is None:
                reason = "transfer truncated (no end marker)"
            else:
                size, digest, status1 = p1
                entries1 = validate_tar(tmp1)
                p2 = session.dump(cmd, tmp2)
                if p2 is None:
                    reason = "pass-2 transfer truncated"
        except OSError as e:
            reason = f"connection error ({e})"
        if reason:
            tmp1.unlink(missing_ok=True)
            tmp2.unlink(missing_ok=True)
            print(f"  ABORT  {mount_point}: {reason}; device may be unhealthy")
            results.append({"mount": mount_point, "fstype": fstype, "verdict": "ABORT"})
            break
        assert p1 is not None and p2 is not None  # guaranteed: `reason` was empty
        entries2 = validate_tar(tmp2)
        tmp2.unlink(missing_ok=True)

        # Both passes must be readable tars with a zero `tar` exit...
        readable = entries1 > 0 and entries2 > 0 and status1 == 0 and p2[2] == 0
        deterministic = (p1[0], p1[1]) == (p2[0], p2[1])
        # ...and the two passes must be byte-identical, UNLESS this is a read-write
        # mount the operator explicitly allowed to differ between passes (a live fs
        # such as /data). A read-only mount can never legitimately differ, so a
        # mismatch there is corruption and always FAILs. tar headers carry no
        # payload checksum, so the pass comparison is the only guard against a torn
        # read -- hence it must be able to fail.
        live_ok = mount_point in allow_live and mode == "rw"
        ok = readable and (deterministic or live_ok)
        if ok:
            tmp1.replace(final)
            final.chmod(0o600)
        else:
            tmp1.unlink(missing_ok=True)
        verdict = "ok" if ok and deterministic else "ok-live" if ok else "FAIL"
        results.append(
            {
                "mount": mount_point,
                "fstype": fstype,
                "size": size,
                "sha256": digest,
                "entries": entries1,
                "verdict": verdict,
            }
        )
        mark = {"ok": "ok   ", "ok-live": "ok*  ", "FAIL": "FAIL "}[verdict]
        if verdict == "ok-live":
            note = "  (live fs, passes differ, both readable -- allowed)"
        elif ok:
            note = ""
        else:
            why = (
                "hash mismatch"
                if readable
                else f"tar exit {status1}/{p2[2]}, entries {entries1}/{entries2}"
            )
            note = f"  ({why})"
        print(
            f"  {mark} {mount_point}: {size} bytes, {entries1} entries, sha256={digest[:16]}{note}"
        )
    return results


def selftest() -> int:
    bad = 0

    def check(cond: bool, msg: str) -> None:
        nonlocal bad
        if not cond:
            print(f"  FAIL  {msg}")
            bad += 1

    sample = (
        "rootfs / rootfs rw 0 0\nubi0:rootfs / ubifs rw 0 0\n"  # kernel lists `/` twice
        "proc /proc proc rw 0 0\ntmpfs /dev tmpfs rw 0 0\n"
        "ubi0:usrfs /data ubifs rw 0 0\n/dev/ubi1_0 /firmware ubifs ro 0 0\n"
    )
    mounts = parse_mounts(sample)
    check(
        mounts
        == [
            ("/", "ubifs", "rw"),  # last entry wins: active ubifs, not rootfs
            ("/proc", "proc", "rw"),
            ("/dev", "tmpfs", "rw"),
            ("/data", "ubifs", "rw"),
            ("/firmware", "ubifs", "ro"),
        ],
        f"parse_mounts keeps all with ro/rw, last-wins on dup /: {mounts}",
    )
    check(
        backup_targets(mounts)
        == [
            ("/", "ubifs", "rw"),
            ("/data", "ubifs", "rw"),
            ("/firmware", "ubifs", "ro"),
        ],
        "backup_targets skips virtual fs",
    )
    check(
        excludes_for("/", [mp for mp, _, _ in mounts])
        == ["data", "dev", "firmware", "proc"],
        "excludes_for includes virtual mounts",
    )
    check(excludes_for("/data", ["/", "/data"]) == [], "excludes_for leaf")
    check(_label("/") == "root" and _label("/data") == "data", "_label")
    cmd = tar_command("/", ["data", "proc"])
    check(
        cmd == "tar -C / --exclude=./data --exclude=./proc -cf - . 2>/dev/null",
        f"tar_command: {cmd}",
    )
    check(
        tar_command("/data", []) == "tar -C /data -cf - . 2>/dev/null",
        "tar_command no excludes",
    )
    for bad_mp in ["/etc; rm -rf /", "/..", "relative", "/a b"]:
        try:
            tar_command(bad_mp, [])
            check(False, f"unsafe mount accepted: {bad_mp!r}")
        except ValueError:
            pass
    for bad_ex in ["../etc", "a;b", "a b"]:
        try:
            tar_command("/", [bad_ex])
            check(False, f"unsafe exclude accepted: {bad_ex!r}")
        except ValueError:
            pass
    print("backup logic ok" if not bad else f"{bad} failures")
    return 1 if bad else 0


def main(argv: list[str]) -> int:
    ap = argparse.ArgumentParser(
        description="Back up and verify the RC400L writable filesystems (read-only on the device)."
    )
    ap.add_argument("--selftest", action="store_true")
    ap.add_argument("--proc-mounts", type=Path, help="a capture of /proc/mounts")
    ap.add_argument("--out-dir", type=Path)
    ap.add_argument("--ip", default="192.168.1.1")
    ap.add_argument("--shell-port", type=int, default=24)
    ap.add_argument("--timeout", type=int, default=1200, help="max seconds per archive")
    ap.add_argument(
        "--idle-timeout",
        type=int,
        default=45,
        help="abort if the stream stalls this long",
    )
    ap.add_argument(
        "--only", help="comma-separated mount points to back up (e.g. /,/data)"
    )
    ap.add_argument(
        "--allow-live",
        help="comma-separated rw mount points whose two passes MAY differ "
        "(a live fs such as /data); any other mount must be byte-identical",
    )
    args = ap.parse_args(argv)

    if args.selftest:
        return selftest()
    if not args.proc_mounts or not args.out_dir:
        ap.error("--proc-mounts and --out-dir are required")

    mounts = parse_mounts(args.proc_mounts.read_text(encoding="utf-8"))
    targets = backup_targets(mounts)
    known = {mp for mp, _, _ in targets}
    if args.only:
        want = {m.strip() for m in args.only.split(",")}
        unknown = want - known
        if unknown:  # a typo must fail loudly, not silently back up nothing
            ap.error(
                f"--only names mount(s) that are not backup targets: "
                f"{', '.join(sorted(unknown))} (known: {', '.join(sorted(known))})"
            )
        targets = [m for m in targets if m[0] in want]
    if not targets:
        ap.error("no filesystems selected")
    allow_live = (
        {m.strip() for m in args.allow_live.split(",")} if args.allow_live else set()
    )
    unknown_live = allow_live - known
    if unknown_live:
        ap.error(
            f"--allow-live names unknown mount(s): {', '.join(sorted(unknown_live))}"
        )

    session = ShellTar(args.ip, args.shell_port, args.idle_timeout, args.timeout)
    results = backup_all(session, mounts, targets, Path(args.out_dir), allow_live)

    def manifest_line(r: dict) -> str:
        if r["verdict"] == "ABORT":
            return f"{r['mount']} {r['fstype']} ABORT"
        return (
            f"{r['mount']} {r['fstype']} size={r['size']} entries={r['entries']} "
            f"sha256={r['sha256']} {r['verdict']}"
        )

    manifest = Path(args.out_dir) / "MANIFEST.txt"
    header = "# Orbic RC400L filesystem backup manifest (tar over the root shell)"
    # Merge, keyed by mount point: a `--only` run updates its filesystems' records
    # and PRESERVES the records of filesystems it did not touch, instead of
    # overwriting the whole manifest and losing them.
    records: dict[str, str] = {}
    if manifest.exists():
        for ln in manifest.read_text(encoding="utf-8").splitlines():
            if ln and not ln.startswith("#"):
                records[ln.split()[0]] = ln
    for r in results:
        records[r["mount"]] = manifest_line(r)
    # Write atomically via a temp file + os.replace so an interrupted write cannot
    # truncate an existing manifest; owner-only from creation (it lists each hash).
    tmp = manifest.with_suffix(".txt.tmp")
    mfd = os.open(tmp, os.O_WRONLY | os.O_CREAT | os.O_TRUNC, 0o600)
    os.fchmod(mfd, 0o600)
    with os.fdopen(mfd, "w", encoding="utf-8") as fh:
        fh.write(header + "\n")
        for mp in sorted(records):
            fh.write(records[mp] + "\n")
    os.replace(tmp, manifest)
    failed = [r for r in results if r["verdict"] not in ("ok", "ok-live")]
    print(
        f"\n{len(results) - len(failed)}/{len(results)} filesystems backed up; manifest: {manifest}"
    )
    return 1 if failed else 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
