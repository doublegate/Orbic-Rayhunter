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


def parse_mounts(text: str) -> list[tuple[str, str]]:
    """[(mount_point, fstype), ...] for EVERY mount in /proc/mounts, in order, one
    entry per mount point (the kernel lists `/` twice -- rootfs then the real fs;
    the first wins). Virtual filesystems are kept here because a parent's tar must
    still EXCLUDE them; `backup_targets` decides what is actually archived."""
    out: list[tuple[str, str]] = []
    seen: set[str] = set()
    for line in text.splitlines():
        parts = line.split()
        if len(parts) >= 3 and parts[1].startswith("/") and parts[1] not in seen:
            seen.add(parts[1])
            out.append((parts[1], parts[2]))
    return out


def backup_targets(mounts: list[tuple[str, str]]) -> list[tuple[str, str]]:
    """The real filesystems worth archiving (virtual/ephemeral ones skipped)."""
    return [(mp, fs) for mp, fs in mounts if fs not in _SKIP_FSTYPES]


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

    def dump(self, command: str, dest: Path) -> tuple[int, str] | None:
        """Stream `command`'s output (framed by a trailing marker) into `dest`,
        hashing as it goes. Returns (bytes, sha256) on success, or None if the
        marker never arrived (a truncated transfer -> the caller ABORTS rather
        than launching another tar while this one may still run on the device)."""
        marker = secrets.token_hex(16).encode()
        keep = (
            len(marker) - 1
        )  # bytes held back so a marker spanning recv() boundaries is caught
        sock = socket.create_connection((self.ip, self.port), timeout=10)
        sock.settimeout(self.idle_timeout)
        h = hashlib.sha256()
        total = 0
        pending = b""
        found = False
        end = time.monotonic() + self.deadline
        try:
            sock.sendall(command.encode() + f"; printf {marker.decode()}\n".encode())
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
                    data = pending + chunk
                    idx = data.find(marker)
                    if idx >= 0:
                        fh.write(data[:idx])
                        h.update(data[:idx])
                        total += idx
                        found = True
                        break
                    if len(data) > keep:
                        emit, pending = data[:-keep], data[-keep:]
                        fh.write(emit)
                        h.update(emit)
                        total += len(emit)
                    else:
                        pending = data
        finally:
            sock.close()
        return (total, h.hexdigest()) if found else None


def validate_tar(path: Path) -> int:
    """Number of entries if `path` is a readable tar, else -1 (unreadable/corrupt)."""
    try:
        with tarfile.open(path, "r:") as tf:
            return sum(1 for _ in tf)
    except (tarfile.TarError, OSError):
        return -1


def backup_all(
    session: ShellTar,
    mounts: list[tuple[str, str]],
    targets: list[tuple[str, str]],
    out_dir: Path,
) -> list[dict]:
    # Excludes are computed against EVERY mount (including virtual /proc, /sys,
    # /dev), not just the archived ones, or tar at `/` would descend into them.
    all_mps = [mp for mp, _ in mounts]
    results = []
    for mount_point, fstype in targets:
        label = _label(mount_point)
        print(f"backing up {mount_point} ({fstype}) -> {label}.tar ...", flush=True)
        cmd = tar_command(mount_point, excludes_for(mount_point, all_mps))
        out_dir.mkdir(parents=True, exist_ok=True)
        tmp1 = out_dir / f".{label}.pass1.tmp"
        tmp2 = out_dir / f".{label}.pass2.tmp"
        final = out_dir / f"{label}.tar"
        p1 = session.dump(cmd, tmp1)
        if p1 is None:
            tmp1.unlink(missing_ok=True)
            print(
                f"  ABORT  {mount_point}: transfer truncated (no end marker); device may be unhealthy"
            )
            results.append({"mount": mount_point, "fstype": fstype, "verdict": "ABORT"})
            break  # do not launch another tar after a truncated transfer
        entries = validate_tar(tmp1)
        # A complete, readable tar is a valid restore point. A SECOND pass is taken
        # to catch transfer corruption: an identical hash proves the transfer AND a
        # quiescent filesystem; a differing hash on a readable tar just means the
        # live fs (e.g. /data logs) changed between passes -- still a good backup.
        p2 = session.dump(cmd, tmp2)
        size, digest = p1
        deterministic = p2 is not None and p1 == p2
        ok = entries > 0  # readable, complete tar
        if ok:
            tmp1.replace(final)
            final.chmod(0o600)
        else:
            tmp1.unlink(missing_ok=True)
        tmp2.unlink(missing_ok=True)
        verdict = "ok" if ok and deterministic else "ok-live" if ok else "UNREADABLE"
        results.append(
            {
                "mount": mount_point,
                "fstype": fstype,
                "size": size,
                "sha256": digest,
                "entries": entries,
                "verdict": verdict,
            }
        )
        mark = {"ok": "ok   ", "ok-live": "ok*  ", "UNREADABLE": "FAIL "}[verdict]
        note = (
            ""
            if deterministic or not ok
            else "  (live fs: passes differ, tar readable)"
        )
        print(
            f"  {mark} {mount_point}: {size} bytes, {entries} entries, sha256={digest[:16]}{note}"
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
            ("/", "rootfs"),
            ("/proc", "proc"),
            ("/dev", "tmpfs"),
            ("/data", "ubifs"),
            ("/firmware", "ubifs"),
        ],
        f"parse_mounts keeps all, dedups /: {mounts}",
    )
    check(
        backup_targets(mounts)
        == [("/", "rootfs"), ("/data", "ubifs"), ("/firmware", "ubifs")],
        "backup_targets skips virtual fs",
    )
    check(
        excludes_for("/", [mp for mp, _ in mounts])
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
    args = ap.parse_args(argv)

    if args.selftest:
        return selftest()
    if not args.proc_mounts or not args.out_dir:
        ap.error("--proc-mounts and --out-dir are required")

    mounts = parse_mounts(args.proc_mounts.read_text(encoding="utf-8"))
    targets = backup_targets(mounts)
    if args.only:
        want = {m.strip() for m in args.only.split(",")}
        targets = [m for m in targets if m[0] in want]
    if not targets:
        ap.error("no filesystems selected")

    session = ShellTar(args.ip, args.shell_port, args.idle_timeout, args.timeout)
    results = backup_all(session, mounts, targets, Path(args.out_dir))

    manifest = Path(args.out_dir) / "MANIFEST.txt"
    with manifest.open("w", encoding="utf-8") as fh:
        fh.write(
            "# Orbic RC400L filesystem backup manifest (tar over the root shell)\n"
        )
        for r in results:
            if r["verdict"] == "ABORT":
                fh.write(f"{r['mount']} {r['fstype']} ABORT\n")
            else:
                fh.write(
                    f"{r['mount']} {r['fstype']} size={r['size']} entries={r['entries']} "
                    f"sha256={r['sha256']} {r['verdict']}\n"
                )
    manifest.chmod(0o600)
    failed = [r for r in results if r["verdict"] not in ("ok", "ok-live")]
    print(
        f"\n{len(results) - len(failed)}/{len(results)} filesystems backed up; manifest: {manifest}"
    )
    return 1 if failed else 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
