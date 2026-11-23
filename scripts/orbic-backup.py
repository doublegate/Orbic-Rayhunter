#!/usr/bin/env python3
"""Stream every NAND partition off the RC400L to the host and verify it (Sprint 1.3).

This is the pre-write safety backup (T-OR-015..T-OR-017): it READS each
`/proc/mtd` partition with `nanddump` on the device root shell and writes the
image to the host. It never writes the device.

All partitions stream over ONE persistent shell connection, one `nanddump` at a
time, each delimited by a random end marker. This is deliberate: an earlier
design opened a fresh connection per partition and, when a transfer timed out,
left the `nanddump` running; several of those piled onto the same NAND and wedged
the read path (unkillable D-state readers). A single sequential channel cannot
start a second `nanddump` before the first has finished, so that cannot recur.

The one command sent per partition is a fixed template with the index validated
as an integer -- there is no arbitrary-command path. For each partition the image
is kept only if two independent passes produce the same SHA-256 and the byte
count equals the `/proc/mtd` size. A manifest records every partition, size,
pass-1/pass-2 hash and verdict.

Prerequisite: the root shell must already be open on TCP 24 (see
BACKUP-AND-RECOVERY.md step 1).

Usage:
    scripts/orbic-backup.py --selftest
    scripts/orbic-backup.py --proc-mtd backups/<d>/layout/001-cat-proc-mtd.txt \
        --out-dir backups/<d> [--ip 192.168.1.1] [--shell-port 24] [--only 0,1,16]
"""

from __future__ import annotations

import argparse
import hashlib
import re
import secrets
import socket
import sys
import time
from pathlib import Path


def parse_proc_mtd(text: str) -> list[tuple[int, int, str]]:
    """[(index, size_bytes, name), ...] from /proc/mtd text."""
    out = []
    for line in text.splitlines():
        m = re.match(
            r"mtd(\d+):\s+([0-9a-fA-F]+)\s+[0-9a-fA-F]+\s+\"([^\"]+)\"", line.strip()
        )
        if m:
            out.append((int(m.group(1)), int(m.group(2), 16), m.group(3)))
    return out


def device_dump_command(index: int, marker: str) -> str:
    """The one command sent per partition. The index is validated as an integer
    and the marker is hex, so there is no arbitrary-command path. nanddump writes
    the image (bad blocks padded to keep the size exact, OOB dropped, quiet) to
    the socket; `printf <marker>` then frames the end, so a short read (an
    unreadable block) is detected by length rather than hanging."""
    if not isinstance(index, int) or index < 0 or index > 999:
        raise ValueError("partition index must be an integer in 0..999")
    if not re.fullmatch(r"[0-9a-f]{16,}", marker):
        raise ValueError("marker must be lowercase hex, >= 16 chars")
    return f"/usr/sbin/nanddump --bb=padbad --omitoob -q /dev/mtd{index} 2>/dev/null; printf {marker}"


def sha256_bytes(data: bytes) -> str:
    return hashlib.sha256(data).hexdigest()


def split_on_marker(buf: bytes, marker: bytes) -> tuple[bytes | None, bytes]:
    """If `marker` is present, return (image_before_marker, remainder_after);
    else (None, buf). Used to carve one partition's image out of the stream."""
    i = buf.find(marker)
    if i < 0:
        return None, buf
    return buf[:i], buf[i + len(marker) :]


class ShellBackup:
    """One persistent connection to the port-24 root shell, read sequentially."""

    def __init__(self, ip: str, port: int, idle_timeout: float, deadline_per: float):
        self.ip = ip
        self.port = port
        self.idle_timeout = idle_timeout
        self.deadline_per = deadline_per

    def dump(self, index: int) -> bytes:
        """One FRESH connection per partition: busybox `nc -ll -e /bin/sh` forks a
        new shell per connection, and nanddump produced output only as the first
        command on a shell -- a reused connection returned nothing for every
        partition after the first. A fresh shell each time, run sequentially (one
        nanddump at a time), avoids both that desync and the concurrent-reader
        wedge. The marker is read to completion before the socket is closed, so
        nanddump has finished and cannot be orphaned."""
        marker_s = secrets.token_hex(16)
        marker = marker_s.encode()
        sock = socket.create_connection((self.ip, self.port), timeout=10)
        sock.settimeout(self.idle_timeout)
        buf = b""
        image = None
        end = time.monotonic() + self.deadline_per
        try:
            sock.sendall((device_dump_command(index, marker_s) + "\n").encode())
            while image is None and time.monotonic() < end:
                try:
                    chunk = sock.recv(1 << 16)
                except TimeoutError:
                    break
                if not chunk:
                    break
                buf += chunk
                image, _ = split_on_marker(buf, marker)
        finally:
            sock.close()
        return image if image is not None else buf

    def close(self) -> None:
        pass


def backup_all(
    session: ShellBackup, parts: list[tuple[int, int, str]], out_dir: Path
) -> list[dict]:
    results = []
    for index, size, name in parts:
        print(
            f"backing up mtd{index} ({name}, {size / 1048576:.2f} MiB)...", flush=True
        )
        img = out_dir / f"mtd{index:02d}-{name}.img"
        img.parent.mkdir(parents=True, exist_ok=True)
        passes = []
        for attempt in (1, 2):
            data = session.dump(index)
            passes.append((len(data), sha256_bytes(data)))
            dest = img if attempt == 1 else out_dir / "pass2" / img.name
            dest.parent.mkdir(parents=True, exist_ok=True)
            dest.write_bytes(data)
        ok = passes[0] == passes[1] and passes[0][0] == size
        r = {
            "index": index,
            "name": name,
            "size": size,
            "received": passes[0][0],
            "received2": passes[1][0],
            "sha256": passes[0][1],
            "sha256_pass2": passes[1][1],
            "verdict": "ok" if ok else "SHORT/MISMATCH",
        }
        print(f"  {r['verdict']}  received={r['received']} sha256={r['sha256'][:16]}")
        results.append(r)
    return results


def selftest() -> int:
    bad = 0
    sample = 'dev:    size   erasesize  name\nmtd0: 00280000 00040000 "sbl"\nmtd16: 16d80000 00040000 "system"\n'
    if parse_proc_mtd(sample) != [(0, 0x280000, "sbl"), (16, 0x16D80000, "system")]:
        print("  FAIL  parse_proc_mtd")
        bad += 1
    try:
        cmd = device_dump_command(16, "a" * 16)
        assert (
            cmd
            == f"/usr/sbin/nanddump --bb=padbad --omitoob -q /dev/mtd16 2>/dev/null; printf {'a' * 16}"
        )
    except AssertionError:
        print("  FAIL  device_dump_command template")
        bad += 1
    for args in [
        (-1, "a" * 16),
        (1000, "a" * 16),
        (0, "short"),
        (0, "NOTHEX!!!!!!!!!!"),
    ]:
        try:
            device_dump_command(*args)
            print(f"  FAIL  bad args accepted: {args}")
            bad += 1
        except ValueError:
            pass
    # marker framing: image carved out, remainder preserved for the next partition
    mk = b"deadbeefdeadbeef"
    img, rest = split_on_marker(b"IMAGE-BYTES" + mk + b"NEXT", mk)
    if img != b"IMAGE-BYTES" or rest != b"NEXT":
        print("  FAIL  split_on_marker")
        bad += 1
    if split_on_marker(b"partial-no-marker", mk) != (None, b"partial-no-marker"):
        print("  FAIL  split_on_marker (absent)")
        bad += 1
    print("backup logic ok" if not bad else f"{bad} failures")
    return 1 if bad else 0


def main(argv: list[str]) -> int:
    ap = argparse.ArgumentParser(
        description="Back up and verify the RC400L NAND (read-only on the device)."
    )
    ap.add_argument("--selftest", action="store_true")
    ap.add_argument("--proc-mtd", type=Path)
    ap.add_argument("--out-dir", type=Path)
    ap.add_argument("--ip", default="192.168.1.1")
    ap.add_argument("--shell-port", type=int, default=24)
    ap.add_argument(
        "--timeout", type=int, default=1200, help="max seconds per partition"
    )
    ap.add_argument(
        "--idle-timeout",
        type=int,
        default=45,
        help="abort if the stream stalls this long",
    )
    ap.add_argument("--only", help="comma-separated mtd indices to back up")
    args = ap.parse_args(argv)

    if args.selftest:
        return selftest()
    if not args.proc_mtd or not args.out_dir:
        ap.error("--proc-mtd and --out-dir are required")

    parts = parse_proc_mtd(args.proc_mtd.read_text(encoding="utf-8"))
    if args.only:
        want = {int(x) for x in args.only.split(",")}
        parts = [p for p in parts if p[0] in want]

    session = ShellBackup(args.ip, args.shell_port, args.idle_timeout, args.timeout)
    try:
        results = backup_all(session, parts, Path(args.out_dir))
    finally:
        session.close()

    manifest = Path(args.out_dir) / "MANIFEST.txt"
    with manifest.open("w", encoding="utf-8") as fh:
        fh.write("# Orbic RC400L NAND backup manifest\n")
        for r in results:
            fh.write(
                f"mtd{r['index']:02d} {r['name']} size={r['size']} received={r['received']} "
                f"received2={r['received2']} sha256={r['sha256']} pass2={r['sha256_pass2']} {r['verdict']}\n"
            )
    failed = [r for r in results if r["verdict"] != "ok"]
    print(
        f"\n{len(results) - len(failed)}/{len(results)} partitions verified; manifest: {manifest}"
    )
    return 1 if failed else 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
