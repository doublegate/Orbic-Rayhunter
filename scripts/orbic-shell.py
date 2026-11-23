#!/usr/bin/env python3
"""Drive the RC400L root shell opened by upstream's network exploit (Sprint 1.2).

Upstream `installer util orbic-start-telnet` injects
`busybox nc -ll -p 24 -e /bin/sh` through `/action/SetRemoteAccessCfg`, leaving a
persistent raw root shell on TCP 24 (no login, no prompt). This tool connects to
that port and runs commands one at a time, each terminated by a unique marker so
the exact output and exit status are captured deterministically.

It is READ-ONLY by policy: every command is checked against a conservative
denylist (no redirection, no mtd/ubi/dd writers, no rm/mv/mkfs/flash_erase,
no reboot) before it is sent, so a recon run cannot modify the device. Opening
the shell itself is the one pre-backup action the backup plan allows
(docs/BACKUP-AND-RECOVERY.md step 1); this tool does not open it -- run
`installer util orbic-start-telnet --admin-password ...` first.

Usage:
    scripts/orbic-shell.py --selftest
    scripts/orbic-shell.py --check                       # one probe command
    scripts/orbic-shell.py --script FILE [--out-dir DIR] [--ip IP] [--port 24]
    scripts/orbic-shell.py --command 'cat /proc/mtd' [--out-dir DIR]

With --out-dir, each command's stdout+stderr is written as NNN-<slug>.txt. The
transcript is for recon only; scrub identifiers before committing anything out of
the gitignored backups/ tree.
"""

from __future__ import annotations

import argparse
import re
import shlex
import socket
import sys
import uuid
from pathlib import Path

# A command is refused if it matches any of these: this shell is for reading the
# device, never writing it. The backup (Sprint 1.3) must precede any write.
#
# The model is an ALLOWLIST that FAILS CLOSED: every simple command's program
# must be a known pure reader, or the whole command is refused. A denylist of
# writers would fail open -- an unlisted writer (`ubidetach`, `fw_setenv`,
# `insmod`, a device-specific tool) would slip through. Classification is by the
# FIRST word of each simple command, so a reader name that appears only as an
# argument (`grep nandwrite`, `cat /etc/passwd`) is judged on the real program.
READ_COMMANDS = {
    "cat",
    "ls",
    "ps",
    "df",
    "free",
    "mount",
    "mountpoint",
    "uname",
    "hostname",
    "head",
    "tail",
    "grep",
    "egrep",
    "fgrep",
    "awk",
    "nawk",
    "gawk",
    "sed",
    "wc",
    "md5sum",
    "sha1sum",
    "sha256sum",
    "cksum",
    "cmp",
    "diff",
    "nanddump",
    "dd",
    "ubinfo",
    "dmesg",
    "netstat",
    "ss",
    "lsof",
    "lsmod",
    "id",
    "whoami",
    "groups",
    "which",
    "type",
    "command",
    "find",
    "stat",
    "readlink",
    "realpath",
    "file",
    "od",
    "hexdump",
    "xxd",
    "strings",
    "env",
    "printenv",
    "getprop",
    "echo",
    "printf",
    "test",
    "true",
    "false",
    "date",
    "basename",
    "dirname",
    "cut",
    "sort",
    "uniq",
    "tr",
    "du",
    "pwd",
    "tty",
    "pgrep",
    "busybox",
    "expr",
    "seq",
}
# Dual-use readers that become writers in a specific FORM; refused when they do.
WRITER_SUBFORMS = re.compile(
    r"\bsed\b[^|;&]*\s-i|"  # sed -i (in-place)
    r"\bdd\b[^|;&]*\bof=|"  # dd of=<file>
    r"\bfind\b[^|;&]*\s-(delete|exec|execdir|fprint|fls|fprintf)\b|"  # find writers
    r"\bdmesg\b[^|;&]*\s-(c|C|-clear|-read-clear)\b|"  # dmesg clears the ring buffer
    r"\bsort\b[^|;&]*\s-o\b|"  # sort -o <file>
    r"\bmount\b[^|;&]*\b(remount|[\s,]rw\b)|\bmount\b\s+(-o\s+)?\brw\b|"  # remount rw
    r"\bbusybox\s+(nc|nandwrite|flash_erase\w*|ubi\w+|rm|rmdir|mv|cp|ln|touch|"
    r"mknod|chmod|chown|chgrp|mkfs\w*|dd|tee|reboot|halt|poweroff|kill\w*|"
    r"setprop|passwd|truncate|install|insmod|rmmod|fw_setenv)\b"
)
# Any redirection to a path other than /dev/null (so `2>/dev/null`, `2>&1` are fine).
REDIRECT_TO_FILE = re.compile(r"(?<![0-9&])>>?\s*(?!&|/dev/null\b)")


_ASSIGN_OR_WRAP = re.compile(r"^([A-Za-z_][A-Za-z0-9_]*=|exec$|env$|nice$|time$)")


def _first_words(command: str) -> list[str]:
    """The leading program of each simple command. Tokenised with shlex so that a
    pipe/semicolon INSIDE quotes (`grep 'a|b'`) is an argument, not an operator;
    a lexing error (unbalanced quotes, exotic syntax) fails closed as []."""
    lexer = shlex.shlex(command, posix=True, punctuation_chars=True)
    lexer.whitespace_split = True
    try:
        tokens = list(lexer)
    except ValueError:
        return []
    words: list[str] = []
    expect_command = True
    for tok in tokens:
        if tok in ("|", "||", "&&", ";", "&", "|&", ";;", "\n"):
            expect_command = True
            continue
        if not expect_command:
            continue
        # skip leading wrappers and VAR=val assignments, then take the program
        if _ASSIGN_OR_WRAP.match(tok):
            continue
        words.append(tok.rsplit("/", 1)[-1])
        expect_command = False
    return words


def is_read_only(command: str) -> bool:
    """True only when every simple command is a known reader in a read form.
    Fails closed: an unknown program, an unparseable fragment, a writer subform,
    or a redirection to a file all return False."""
    if REDIRECT_TO_FILE.search(command) or WRITER_SUBFORMS.search(command):
        return False
    words = _first_words(command)
    return bool(words) and all(w in READ_COMMANDS for w in words)


def run_one(
    sock_factory, ip: str, port: int, command: str, timeout: float = 20.0
) -> tuple[str, int]:
    """Send one command, return (output, exit_status). A fresh connection each
    time: busybox `nc -ll` re-listens after every client disconnect."""
    marker = f"__ORX_{uuid.uuid4().hex}__"
    # `exec 2>&1` folds stderr into the stream; the marker line carries the status.
    payload = f"exec 2>&1; {command}\nprintf '\\n{marker} %s\\n' \"$?\"\nexit\n"
    conn = sock_factory(ip, port, timeout)
    try:
        conn.sendall(payload.encode("utf-8", "surrogateescape"))
        chunks: list[bytes] = []
        needle = marker.encode()
        while True:
            data = conn.recv(4096)
            if not data:
                break
            chunks.append(data)
            if needle in b"".join(chunks[-2:]):
                break
    finally:
        conn.close()
    raw = b"".join(chunks).decode("utf-8", "replace")
    m = re.search(re.escape(marker) + r" (-?\d+)", raw)
    status = int(m.group(1)) if m else -1
    output = raw[: m.start()].rstrip("\n") if m else raw
    # Strip the echoed command line the shell may prepend.
    lines = output.split("\n")
    if lines and command.strip().endswith(lines[0].strip().split(";")[-1].strip()):
        lines = lines[1:]
    return "\n".join(lines), status


def tcp_connect(ip: str, port: int, timeout: float) -> socket.socket:
    s = socket.create_connection((ip, port), timeout=timeout)
    s.settimeout(timeout)
    return s


def slug(command: str) -> str:
    base = re.sub(r"[^a-z0-9]+", "-", command.lower()).strip("-")
    return (base[:48] or "cmd").rstrip("-")


def selftest() -> int:
    bad = 0
    reads = [
        "cat /proc/mtd",
        "ps w",
        "ls -l /dev/mtd*",
        "df -h",
        "uname -a",
        "ubinfo -a",
    ]
    writes = [
        "cat x > y",
        "dd if=/dev/mtd0 of=/tmp/x",
        "rm -rf /data",
        "flash_erase /dev/mtd0 0 0",
        "reboot",
        "busybox nc -ll -p 23 -e /bin/sh",
        "mount -o remount,rw /",
        "echo hi >> /etc/passwd",
        # writers an older denylist would have missed (fail-closed allowlist catches them):
        "ubidetach -p /dev/mtd16",
        "fw_setenv bootcmd x",
        "insmod /lib/x.ko",
        "ubiupdatevol /dev/ubi0_0 img",
        "install -m0755 x /bin/y",
        "find / -name x -delete",
        "dmesg -c",
        "cat /proc/mtd; nandwrite /dev/mtd0 x",  # a writer hiding after a reader
        "&& rm -rf /",  # operator-only / unparseable fragment
    ]
    for c in reads:
        if not is_read_only(c):
            print(f"  FAIL  read command refused: {c}")
            bad += 1
    for c in writes:
        if is_read_only(c):
            print(f"  FAIL  write command allowed: {c}")
            bad += 1
    print(
        "all read/write classifications correct" if not bad else f"{bad} misclassified"
    )
    return 1 if bad else 0


def main(argv: list[str]) -> int:
    ap = argparse.ArgumentParser(
        description="Drive the RC400L root shell on TCP 24 (read-only)."
    )
    ap.add_argument("--selftest", action="store_true")
    ap.add_argument("--ip", default="192.168.1.1")
    ap.add_argument("--port", type=int, default=24)
    ap.add_argument("--check", action="store_true", help="run one `id; uname -a` probe")
    ap.add_argument("--command", help="run a single command")
    ap.add_argument(
        "--script", type=Path, help="run each non-comment line as a command"
    )
    ap.add_argument("--out-dir", type=Path, help="write NNN-<slug>.txt per command")
    args = ap.parse_args(argv)

    if args.selftest:
        return selftest()

    commands: list[str]
    if args.check:
        commands = ["id; uname -a; cat /proc/mtd"]
    elif args.command:
        commands = [args.command]
    elif args.script:
        commands = [
            ln.rstrip()
            for ln in args.script.read_text(encoding="utf-8").splitlines()
            if ln.strip() and not ln.lstrip().startswith("#")
        ]
    else:
        ap.error("use --selftest, --check, --command or --script")

    # Hard gate: this tool only ever sends read-only commands. There is no
    # override flag -- a write path belongs in a separate, post-backup tool.
    refused = [c for c in commands if not is_read_only(c)]
    if refused:
        for c in refused:
            print(f"refusing (not read-only): {c}", file=sys.stderr)
        return 2

    if args.out_dir:
        args.out_dir.mkdir(parents=True, exist_ok=True)
    rc = 0
    for n, command in enumerate(commands, 1):
        try:
            output, status = run_one(tcp_connect, args.ip, args.port, command)
        except OSError as e:
            print(
                f"error: shell connection to {args.ip}:{args.port} failed: {e}",
                file=sys.stderr,
            )
            return 3
        header = f"$ {command}\n{output}"
        print(f"== [{status}] {command}")
        print(output)
        if args.out_dir:
            (args.out_dir / f"{n:03d}-{slug(command)}.txt").write_text(
                header + f"\n# exit={status}\n"
            )
        if status != 0:
            rc = 1
    return rc


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
