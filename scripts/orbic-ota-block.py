#!/usr/bin/env python3
"""Reversibly disable the RC400L's on-demand OTA/FOTA updater (Sprint 1.4, T-OR-018).

This is the first tool that WRITES to the device, so it is deliberately narrow and
reversible. It renames a FIXED set of updater binaries to `<path>.orx-disabled`,
so a carrier-pushed OMA-DM session or a stray UI trigger cannot exec the updater.
It changes nothing else; auto-FOTA is already off in `/etc/xml/fota/fota_cfg.xml`
(see re/notes/ota-mechanism.md).

Safety model:
- The targets are a hardcoded allowlist under /usr/bin; no path comes from the
  caller, so there is no arbitrary-write surface.
- `--status` (the default) only READS. A write needs `--apply` AND
  `--backup-manifest <MANIFEST.txt>` from a verified backup whose rootfs (`/`,
  where /usr/bin lives) is `ok`/`ok-live` -- the pre-write gate.
- `--apply` records an undo manifest (each path, its `.orx-disabled` name and the
  device SHA-256 read before the rename) under the backup dir, prints an audit
  line, and verifies each rename. `--undo <undo.json>` reverses it exactly.

Recovery if anything goes wrong: `--undo`, or restore the binaries from the
Sprint 1.3 backup's `root.tar`. No bootloader/modem/partition write is involved.

Prerequisite: the root shell must be open on TCP 24 (BACKUP-AND-RECOVERY.md step 1).
"""

from __future__ import annotations

import argparse
import hashlib
import json
import re
import secrets
import socket
import sys
import time
from datetime import UTC, datetime
from pathlib import Path

SUFFIX = ".orx-disabled"
# The on-demand OTA/FOTA updater binaries (RE: re/notes/ota-mechanism.md).
TARGETS = ("/usr/bin/oma_dm", "/usr/bin/dmclient", "/usr/bin/mdm-daemon")
_SAFE_TARGET = re.compile(r"^/usr/bin/[A-Za-z0-9_-]+$")


def _connect(ip: str, port: int, timeout: float) -> socket.socket:
    s = socket.create_connection((ip, port), timeout=10)
    s.settimeout(timeout)
    return s


def run(
    sock_factory, ip: str, port: int, command: str, timeout: float = 20.0
) -> tuple[str, int]:
    """Send one command over the root shell, return (stdout+stderr, exit_status).
    Framed by a random marker followed by the status, read until the status
    terminator; a lost status reads as -1."""
    marker = secrets.token_hex(12)
    done = re.compile(re.escape(marker).encode() + rb" (-?\d+)\.")
    conn = sock_factory(ip, port, timeout)
    buf = b""
    try:
        conn.sendall(f"{{ {command} ; }} 2>&1; printf '{marker} %s.' \"$?\"\n".encode())
        end = time.monotonic() + timeout
        while time.monotonic() < end:
            try:
                chunk = conn.recv(4096)
            except TimeoutError:
                break
            if not chunk:
                break
            buf += chunk
            if done.search(buf):
                break
    finally:
        conn.close()
    m = done.search(buf)
    status = int(m.group(1)) if m else -1
    out = (
        buf[: m.start()].decode("utf-8", "replace").rstrip("\n")
        if m
        else buf.decode("utf-8", "replace")
    )
    return out, status


def _device_sha(sock_factory, ip, port, path: str) -> str | None:
    out, st = run(sock_factory, ip, port, f"sha256sum {path}")
    if st != 0:
        return None
    return out.split()[0] if out.split() else None


def _exists(sock_factory, ip, port, path: str) -> bool:
    _out, st = run(sock_factory, ip, port, f"test -e {path}")
    return st == 0


def status(sock_factory, ip: str, port: int) -> list[dict]:
    rows = []
    for t in TARGETS:
        present = _exists(sock_factory, ip, port, t)
        disabled = _exists(sock_factory, ip, port, t + SUFFIX)
        state = (
            "disabled"
            if (disabled and not present)
            else "active"
            if present
            else "absent"
        )
        rows.append(
            {"path": t, "present": present, "disabled": disabled, "state": state}
        )
        print(f"  {state:9} {t}{'  (' + t + SUFFIX + ' present)' if disabled else ''}")
    return rows


def _backup_gate(manifest: Path) -> None:
    """Refuse to write unless a VERIFIED backup of rootfs (/) exists: /usr/bin lives
    on `/`, so a rootfs restore is the recovery path for a bad rename. It is not
    enough that the manifest says `ok` -- the `root.tar` beside it must exist and
    match the size and SHA-256 the manifest records (a manifest can be stale, hand
    edited, or point at a deleted/truncated archive). Tokens are parsed by key, so
    both the Python (`size= entries= sha256=`) and Rust (`size= sha256=`) manifest
    layouts are accepted."""
    if not manifest.is_file():
        raise SystemExit(f"backup manifest not found: {manifest} (run a backup first)")
    root = None
    for ln in manifest.read_text(encoding="utf-8").splitlines():
        if ln.startswith("#"):
            continue
        f = ln.split()
        if len(f) >= 3 and f[0] == "/":
            root = f
    if root is None:
        raise SystemExit(f"{manifest} has no rootfs '/' entry; refusing to write")
    state = root[-1]
    if state not in ("ok", "ok-live"):
        raise SystemExit(
            f"{manifest}: rootfs '/' state is {state!r}, not ok/ok-live; refusing to write"
        )
    toks = dict(t.split("=", 1) for t in root if "=" in t)
    tar = manifest.parent / "root.tar"
    if not tar.is_file():
        raise SystemExit(
            f"{tar} is missing; the manifest is not backed by a rootfs archive"
        )
    size = tar.stat().st_size
    if size < 1024:
        raise SystemExit(f"{tar} is only {size} bytes; not a valid archive")
    if "size" in toks and toks["size"] != str(size):
        raise SystemExit(
            f"{tar} size {size} != manifest {toks['size']}; refusing to write"
        )
    if "sha256" in toks:
        h = hashlib.sha256()
        with open(tar, "rb") as fh:
            for chunk in iter(lambda: fh.read(1 << 20), b""):
                h.update(chunk)
        if h.hexdigest() != toks["sha256"]:
            raise SystemExit(
                f"{tar} SHA-256 does not match the manifest; refusing to write"
            )


def apply_block(sock_factory, ip: str, port: int, out_dir: Path) -> int:
    undo: list[dict] = []
    ts = datetime.now(UTC).strftime("%Y%m%dT%H%M%SZ")
    out_dir.mkdir(parents=True, exist_ok=True)
    manifest = out_dir / f"ota-block-{ts}.json"

    def _persist_undo() -> None:
        # Always record what WAS disabled -- even on a partial failure -- so the
        # device is never mutated without an undo manifest for --undo.
        if not undo:
            return
        rec = {"action": "ota-block", "ts": ts, "targets": undo}
        manifest.write_text(json.dumps(rec, indent=2) + "\n", encoding="utf-8")
        print(
            f"\nAUDIT ota-block {ts}: disabled {len(undo)} updater binary(ies); undo manifest {manifest}"
        )

    try:
        for t in TARGETS:
            assert _SAFE_TARGET.match(t)  # fixed list; defense in depth
            if not _exists(sock_factory, ip, port, t):
                print(f"  skip   {t} (already absent or disabled)")
                continue
            # Never overwrite an existing saved binary: if <t>.orx-disabled is
            # already present, a prior block's original would be clobbered.
            if _exists(sock_factory, ip, port, t + SUFFIX):
                print(
                    f"  FAIL   {t}: {t}{SUFFIX} already exists; refusing to overwrite the saved binary"
                )
                return 1
            sha = _device_sha(sock_factory, ip, port, t)
            out, st = run(sock_factory, ip, port, f"mv {t} {t}{SUFFIX}")
            if (
                st != 0
                or _exists(sock_factory, ip, port, t)
                or not _exists(sock_factory, ip, port, t + SUFFIX)
            ):
                print(f"  FAIL   {t}: rename did not take (status {st}): {out}")
                return 1
            undo.append({"path": t, "disabled": t + SUFFIX, "sha256": sha})
            print(f"  disabled  {t} -> {t}{SUFFIX}  sha256={(sha or '?')[:16]}")
        return 0
    finally:
        _persist_undo()


def undo_block(sock_factory, ip: str, port: int, manifest: Path) -> int:
    rec = json.loads(manifest.read_text(encoding="utf-8"))
    for e in rec.get("targets", []):
        path, disabled = e["path"], e["disabled"]
        if not (_SAFE_TARGET.match(path) and disabled == path + SUFFIX):
            print(f"  FAIL   refusing unexpected undo entry: {e}")
            return 1
        if not _exists(sock_factory, ip, port, disabled):
            print(f"  skip   {disabled} (not present)")
            continue
        # Refuse to clobber a binary that is unexpectedly present at the original
        # path -- restoring over it would destroy whatever is there.
        if _exists(sock_factory, ip, port, path):
            print(f"  FAIL   {path}: already present; refusing to overwrite on restore")
            return 1
        _out, st = run(sock_factory, ip, port, f"mv {disabled} {path}")
        if st != 0 or not _exists(sock_factory, ip, port, path):
            print(f"  FAIL   {path}: restore did not take (status {st})")
            return 1
        want = e.get("sha256")
        if want:
            got = _device_sha(sock_factory, ip, port, path)
            if got != want:
                print(
                    f"  WARN   {path}: restored SHA-256 {(got or '?')[:16]} != recorded {want[:16]}"
                )
        print(f"  restored  {disabled} -> {path}")
    print(f"\nAUDIT ota-unblock: restored from {manifest}")
    return 0


def selftest() -> int:
    bad = 0
    # target allowlist
    for t in TARGETS:
        if not _SAFE_TARGET.match(t):
            print(f"  FAIL target not safe: {t}")
            bad += 1
    for bad_t in [
        "/usr/bin/../../etc/x",
        "/etc/passwd",
        "/usr/bin/a b",
        "/usr/bin/a;rm",
    ]:
        if _SAFE_TARGET.match(bad_t):
            print(f"  FAIL unsafe target accepted: {bad_t}")
            bad += 1
    # backup gate: verifies the rootfs archive, not just the manifest text.
    import tempfile

    payload = b"x" * 4096  # >= the 1 KiB floor
    good_sha = hashlib.sha256(payload).hexdigest()

    def _gate_raises(m: Path) -> bool:
        try:
            _backup_gate(m)
            return False
        except SystemExit:
            return True

    with tempfile.TemporaryDirectory() as d:
        dp = Path(d)
        m = dp / "MANIFEST.txt"
        # (1) no rootfs line -> raise
        m.write_text("# h\n/data ubifs size=1 sha256=x ok-live\n", encoding="utf-8")
        if not _gate_raises(m):
            print("  FAIL gate passed without a rootfs line")
            bad += 1
        # (2) rootfs ok but no root.tar beside it -> raise
        m.write_text(f"# h\n/ ubifs size=4096 sha256={good_sha} ok\n", encoding="utf-8")
        if not _gate_raises(m):
            print("  FAIL gate passed with no root.tar")
            bad += 1
        # (3) root.tar present but hash mismatch -> raise
        (dp / "root.tar").write_bytes(b"y" * 4096)
        if not _gate_raises(m):
            print("  FAIL gate passed with a mismatched root.tar")
            bad += 1
        # (4) root.tar present and size+hash match -> pass (both ok and ok-live)
        (dp / "root.tar").write_bytes(payload)
        if _gate_raises(m):
            print("  FAIL gate rejected a verified rootfs backup")
            bad += 1
        m.write_text(
            f"# h\n/ rootfs size=4096 entries=3 sha256={good_sha} ok-live\n",
            encoding="utf-8",
        )
        if _gate_raises(m):
            print("  FAIL gate rejected a verified ok-live rootfs backup")
            bad += 1

    # run() must reassemble the marker + status even when TCP splits them: a fake
    # socket captures the random marker run() sends, then delivers output and the
    # `<marker> <status>.` terminator across separate reads.
    class _Fake:
        def __init__(self) -> None:
            self.parts: list[bytes] = []

        def settimeout(self, _t):
            pass

        def sendall(self, b: bytes):
            mk = re.search(rb"printf '([0-9a-f]+) %s", b)
            if mk:
                self.parts = [b"hello\n", mk.group(1) + b" 0", b"."]

        def recv(self, _n):
            return self.parts.pop(0) if self.parts else b""

        def close(self):
            pass

    fake = _Fake()
    out, st = run(lambda _ip, _port, _timeout: fake, "x", 0, "echo hello")
    if (out, st) != ("hello", 0):
        print(f"  FAIL run() framing: {out!r} {st}")
        bad += 1
    print("ota-block logic ok" if not bad else f"{bad} failures")
    return 1 if bad else 0


def main(argv: list[str]) -> int:
    ap = argparse.ArgumentParser(
        description="Reversibly disable the RC400L OTA/FOTA updater (first device write)."
    )
    ap.add_argument("--selftest", action="store_true")
    ap.add_argument(
        "--apply",
        action="store_true",
        help="perform the rename (requires --backup-manifest)",
    )
    ap.add_argument(
        "--undo", type=Path, metavar="UNDO.json", help="reverse a previous --apply"
    )
    ap.add_argument(
        "--backup-manifest",
        type=Path,
        help="a verified backup MANIFEST.txt (rootfs ok/ok-live)",
    )
    ap.add_argument(
        "--out-dir",
        type=Path,
        help="where to write the undo manifest (default: alongside --backup-manifest)",
    )
    ap.add_argument("--ip", default="192.168.1.1")
    ap.add_argument("--port", type=int, default=24)
    args = ap.parse_args(argv)

    if args.selftest:
        return selftest()
    if args.undo:
        return undo_block(_connect, args.ip, args.port, args.undo)
    if not args.apply:
        print("OTA/FOTA updater status (read-only; pass --apply to disable):")
        status(_connect, args.ip, args.port)
        return 0
    if not args.backup_manifest:
        ap.error("--apply requires --backup-manifest from a verified backup")
    _backup_gate(args.backup_manifest)
    out_dir = args.out_dir or args.backup_manifest.parent
    return apply_block(_connect, args.ip, args.port, out_dir)


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
