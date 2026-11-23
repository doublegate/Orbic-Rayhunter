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
import os
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
# FIRST word of each simple command (tokenised with shlex, so quoting cannot
# disguise a program name), so a reader name that appears only as an argument
# (`grep nandwrite`, `cat /etc/passwd`) is judged on the real program.
#
# Deliberately EXCLUDED, though they can read: `awk`/`nawk`/`gawk` (`system()`,
# `"cmd" | getline`), `command`/`env` (run any program), `find` (`-exec`/`-delete`),
# `sed` (`-i`, `w` writes), `dd`/`nanddump` (write with `of=`/`-f`), `mount`, `xxd`
# (`xxd a b` writes `b`), `uniq` (`uniq in out` writes `out`), `file` (`file -C`
# compiles a magic cache), and the setters `sort` (`-o`), `date` (`-s`/operand),
# `hostname <name>`. Every remaining reader writes only to stdout and takes input
# operands only. The recon inventory needs none of the removed ones; arbitrary-
# execution, output-operand or setter tools have no place
# in a read-only gate guarding an unauthenticated root shell before a backup.
READ_COMMANDS = {
    "cat",
    "ls",
    "ps",
    "df",
    "free",
    "mountpoint",
    "uname",
    "head",
    "tail",
    "grep",
    "egrep",
    "fgrep",
    "wc",
    "md5sum",
    "sha1sum",
    "sha256sum",
    "cksum",
    "cmp",
    "diff",
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
    "stat",
    "readlink",
    "realpath",
    "od",
    "hexdump",
    "strings",
    "printenv",
    "getprop",
    "echo",
    "printf",
    "test",
    "true",
    "false",
    "basename",
    "dirname",
    "cut",
    "tr",
    "du",
    "pwd",
    "tty",
    "pgrep",
    "busybox",
    "expr",
    "seq",
}
# Command substitution and escapes: `$(...)`, backticks, and a backslash. Even
# with an allowlisted program, a substituted argument -- `cat $(reboot)`,
# `ls \`nandwrite ...\`` -- runs arbitrary code; a backslash can also confuse the
# lexer. Refused outright; recon never needs them.
_INJECTION = re.compile(r"\$\(|`|\\")
# A program token must be a bare name or live under a trusted system bin dir --
# never a `VAR=val` prefix, an `env`/`exec`/`nice`/`time` wrapper (which would run
# the next word, defeating the allowlist and, for `LD_PRELOAD=...`/`exec`, injecting
# code or replacing the shell), nor a path like `./cat` or `/tmp/cat` that runs a
# planted binary with an allowed basename.
_TRUSTED_PROG = re.compile(
    r"[A-Za-z0-9._][A-Za-z0-9._-]*|/(bin|sbin|usr/bin|usr/sbin)/[A-Za-z0-9._-]+"
)

# Operator tokens shlex (punctuation_chars=True) emits. With that mode a RUN of the
# chars `();<>|&` is returned as one token, so a combined operator like `;>` or a
# grouping token like `;(` is a token made ENTIRELY of these chars -- distinct from
# a quoted argument such as `fota|update`, which also has letters. The rule below
# rejects any all-operator token that is not an exact recognized operator.
_OP_CHARS = frozenset("();<>|&{}")
_CMD_SEP = {"|", "||", "&&", ";", "&", "|&", ";;"}
_OUT_FILE = {">", ">>", ">|", "&>", "&>>"}  # target must be /dev/null
_DUP = {">&", "<&"}  # target: a bare fd (2>&1) or /dev/null
_IN = {"<", "<<", "<<<"}  # input redirect (only plain `<` to a filename is allowed)
_KNOWN_OPS = _CMD_SEP | _OUT_FILE | _DUP | _IN | {"<>"}
_FD = re.compile(r"\d+")


def _split_simple(tokens: list[str]) -> list[list[str]] | None:
    """Split the token stream into simple commands at operator tokens, validating
    redirects: an output redirect to anything but /dev/null (or an fd dup like
    2>&1) makes the command a writer. Returns None (fail closed) on any such write
    or a malformed redirect."""
    simples: list[list[str]] = []
    cur: list[str] = []
    i, n = 0, len(tokens)
    while i < n:
        t = tokens[i]
        if t in _CMD_SEP:
            if cur:
                simples.append(cur)
                cur = []
            i += 1
        elif t in _OUT_FILE:
            if (tokens[i + 1] if i + 1 < n else "") != "/dev/null":
                return None
            i += 2
        elif t in _DUP:
            tgt = tokens[i + 1] if i + 1 < n else ""
            if not _FD.fullmatch(tgt) and tgt != "/dev/null":
                return None  # `>&/dev/mtd0` redirects to a file, not an fd dup
            i += 2
        elif t == "<>":
            return None  # read-write
        elif t in _IN:
            # Only plain `<` to a real filename is a safe read. Reject heredocs
            # (`<<`/`<<<`, which can swallow run_one's marker), and reject a `<`
            # whose next token is missing or is an operator rather than a filename.
            nxt = tokens[i + 1] if i + 1 < n else ""
            if t != "<" or not nxt or nxt in _KNOWN_OPS:
                return None
            i += 2
        else:
            cur.append(t)
            i += 1
    if cur:
        simples.append(cur)
    return simples


def _simple_ok(tokens: list[str], depth: int = 0) -> bool:
    """One simple command is read-only: its program is an allowed reader and it is
    not in a setter/writer form. `busybox <applet> ...` is validated by recursing
    on the applet, so `busybox sort -o f` or `busybox busybox rm` cannot smuggle a
    writer past the `busybox`-level checks."""
    if depth > 4 or not tokens:
        return False  # busybox busybox busybox ... / empty -> fail closed
    raw = tokens[0]
    # No VAR=val prefix or env/exec/nice/time wrapper is skipped: such a token is
    # simply the program, and since it is not an allowed reader it is refused. The
    # program must also be a bare name or a trusted system path, never `/tmp/cat`.
    if not _TRUSTED_PROG.fullmatch(raw):
        return False
    prog = raw.rsplit("/", 1)[-1]
    args = tokens[1:]
    if prog not in READ_COMMANDS:
        return False
    if prog == "busybox":
        applet_at = next((j for j, a in enumerate(args) if not a.startswith("-")), None)
        if applet_at is None:
            # no applet: only the read-only informational flags are allowed
            # (`--install`/`--mkdir`/... write, so a bare flag is not assumed safe)
            return all(a in ("--list", "--help") for a in args)
        if any(a not in ("--list", "--help") for a in args[:applet_at]):
            return False
        return _simple_ok(args[applet_at:], depth + 1)  # validate the applet itself
    # Any dmesg flag can clear or mutate console state (`-c`, `-n`, `-D`, `-E`);
    # the inventory pipes bare `dmesg`, so refuse dmesg with any option.
    return not (prog == "dmesg" and any(a.startswith("-") for a in args))


def is_read_only(command: str) -> bool:
    """True only when every simple command is a known reader in a read form.
    Fails closed: a control char (newline injection), command substitution, a
    combined/grouping operator token, an unknown or arbitrary-execution program, a
    writer flag, or an output redirection to a file all return False."""
    if any(c in command for c in ("\n", "\r", "\x00")) or _INJECTION.search(command):
        return False
    lexer = shlex.shlex(command, posix=True, punctuation_chars=True)
    lexer.whitespace_split = True
    # shlex treats `#` as a comment and drops the rest of the line, but /bin/sh
    # only does so when `#` starts a word -- `cat /etc/x#; nandwrite ...` is NOT a
    # comment to sh, so the device would run the writer while the gate saw only the
    # `cat`. Disable comment handling so the tokens match what the device executes.
    lexer.commenters = ""
    try:
        tokens = list(lexer)
    except ValueError:
        return False  # unbalanced quotes / exotic syntax -> fail closed
    # Reject any token made entirely of operator/grouping chars that is not an exact
    # recognized operator: this catches combined operators (`;>`, `&&>`), subshells
    # and process substitution (`;(`, `<(`), brace groups (`{`, `}`) and function
    # definitions (`()`), each of which would otherwise ride as an argument. A
    # quoted arg like `fota|update` is NOT all-operator-chars, so it is unaffected.
    if any(t and set(t) <= _OP_CHARS and t not in _KNOWN_OPS for t in tokens):
        return False
    simples = _split_simple(tokens)
    if not simples:
        return False
    return all(_simple_ok(toks) for toks in simples)


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
        # The status arrives AFTER the marker (`printf '\n<marker> %s\n' "$?"`), so
        # stop only once the COMPLETE `<marker> <status>\n` line is present -- not on
        # the bare marker, which may split from its status across TCP reads -- or on
        # EOF (the trailing `exit` closes the shell).
        done = re.compile(re.escape(marker).encode() + rb" -?\d+\n")
        while True:
            data = conn.recv(4096)
            if not data:
                break
            chunks.append(data)
            if done.search(b"".join(chunks[-2:])):
                break
    finally:
        conn.close()
    raw = b"".join(chunks).decode("utf-8", "replace")
    m = re.search(re.escape(marker) + r" (-?\d+)", raw)
    status = int(m.group(1)) if m else -1
    # Everything before the marker is the command's own output. The device shell
    # (`nc -e /bin/sh`, no PTY) does not echo its input, so there is no command line
    # to strip -- an earlier heuristic that tried to could drop a real first line.
    output = raw[: m.start()].rstrip("\n") if m else raw
    return output, status


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
        "cat /dev/mtd0 2>/dev/null",  # stderr to the bit bucket is fine
        "ps w 2>&1 | grep x",  # fd dup is fine
        "busybox --list",
        "busybox | head -1",
        "busybox cat /proc/mtd",  # applet is itself an allowed reader
        "dmesg | grep -i -E 'nand|ubi|mtd'",
        "ps w | grep -iE 'fota|update|ota'",
        "sha256sum /etc/init.d/misc-daemon",
        "netstat -tlnp",
        "ls -la /dev/*diag* 2>/dev/null",
        "grep -c '2>/etc/passwd' /proc/mtd",  # a quoted `>` is a search arg, not a redirect
        "echo a#b",  # a mid-word `#` is a literal char to /bin/sh, not a comment
    ]
    writes = [
        "cat x > y",
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
        "cat /proc/mtd; nandwrite /dev/mtd0 x",  # a writer hiding after a reader
        "&& rm -rf /",  # operator-only / unparseable fragment
        # fd-prefixed and bash redirects
        "echo foo 1>/etc/passwd",
        "cat /etc/version 2>/etc/init.d/misc-daemon",
        "cat /proc/mtd &>/dev/mtd0",
        "ls / >>/data/x",
        "cat /proc/mtd >&/dev/mtd0",  # >&file redirects to a file, not an fd dup
        "cat >",  # dangling redirect, no target
        # command substitution / escapes: an allowed reader runs arbitrary code
        "cat $(reboot)",
        "ls `nandwrite /dev/mtd0 x`",
        "cat /etc/\\passwd",
        # newline injection: shlex folds \n to whitespace, so reject control chars
        "cat /proc/mtd\nflash_erase /dev/mtd0 0 0",
        # arbitrary-execution programs removed from the allowlist
        "awk 'BEGIN{system(\"reboot\")}'",
        "command rm -rf /",
        "busybox 'rm' -rf /",  # quoting cannot hide the busybox applet
        'dd if=/dev/zero "of"=/dev/mtd0',
        "sed '-i' s/a/b/ /etc/passwd",
        "sed 'w /etc/passwd' /proc/mtd",
        "nanddump -f /etc/passwd /dev/mtd0",
        "mount /dev/ubi0_0 /mnt",
        "find / -name x -delete",
        # setter FORMS of dual-use readers, caught on tokens despite quoting
        "dmesg '-c'",
        "dmesg -rc",  # bundled clear flag
        # setters/writers now removed from the allowlist entirely
        "sort -o /etc/passwd /proc/mtd",
        "sort -o/etc/passwd x",
        "date -s '2020-01-01'",
        "date 010100002020",  # busybox date sets the clock from a bare operand
        "hostname newname",
        "xxd /proc/mtd /etc/passwd",  # xxd writes its second operand
        "uniq /proc/mtd /etc/passwd",  # uniq writes its second (output) operand
        "busybox uniq a b",
        "file -C -m /proc/mtd",  # file -C compiles a writable magic cache
        "env rm -rf /",
        "/usr/bin/env rm -rf /",
        # shell grouping/definition/substitution syntax riding on an allowed program
        "cat() (reboot); cat",
        "cat <(reboot)",
        "{ reboot; }",
        "cat >(nandwrite /dev/mtd0 x)",
        "true ;(rm -rf /)",
        # combined-operator tokens that must not fall through as arguments
        "cat /proc/mtd;>/etc/passwd",
        "echo x&&>/etc/passwd",
        # a mid-word `#` is not a comment to /bin/sh, so the writer after it runs
        "cat /etc/foo#; nandwrite /dev/mtd0 x",
        # VAR=val / wrapper prefixes and planted-path programs
        "LD_PRELOAD=/tmp/x.so cat /proc/mtd",
        "env PATH=/tmp cat /proc/mtd",
        "exec cat /proc/mtd",
        "/tmp/cat /proc/mtd",
        "./cat /proc/mtd",
        # `<` redirect must take a filename, not an operator or heredoc
        "cat < ; rm -rf /",
        "cat < > /etc/passwd",
        "cat <<EOF",
        # dmesg console-state flags beyond clear
        "dmesg -n 1",
        "dmesg -D",
        # busybox applet bypasses (validated by recursing on the applet)
        "busybox sort -o /etc/passwd /proc/mtd",
        "busybox busybox rm -rf /",
        "busybox env rm -rf /",
        "busybox --install",
        "busybox dmesg -c",
        "busybox date -s 2020-01-01",
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
            # A transcript can capture /proc/cmdline (androidboot.serialno),
            # /etc/passwd and any cat'd secret, so create it owner-only from the
            # start rather than at the umask default.
            target = args.out_dir / f"{n:03d}-{slug(command)}.txt"
            fd = os.open(target, os.O_WRONLY | os.O_CREAT | os.O_TRUNC, 0o600)
            os.fchmod(fd, 0o600)
            with os.fdopen(fd, "w", encoding="utf-8") as fh:
                fh.write(header + f"\n# exit={status}\n")
        if status != 0:
            rc = 1
    return rc


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
