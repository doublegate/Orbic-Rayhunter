#!/usr/bin/env python3
"""Read-only client for the RC400L stock web UI (GoAhead, ORB400L_V1.2.0_BVZRT).

Logs in exactly as upstream's installer does (`installer/src/orbic_auth.rs`,
`orbic_network.rs`: GetLoginInfo -> login with the encoded password), calls only
the allowlisted read endpoints below, then logs out. It never runs the UI's
JavaScript, and it refuses `/action/FotaGetVersionInfo`: the stock UI calls that
on every page load, which asks the device to check for an OTA update
(docs/DEVICE-PROFILE.md).

Usage:
    scripts/orbic-ui-read.py --selftest
    scripts/orbic-ui-read.py [--ip 192.168.1.1] [--password-file PATH] \
        [--out-dir DIR] ENDPOINT [ENDPOINT ...]

The admin password is read from --password-file (default
~/.claude-secrets/orbic-admin.txt, which must be mode 600), never from argv, and
is never printed. Raw JSON responses are written only with --out-dir, as
<endpoint>.json with mode 600; keep that under the gitignored backups/. Standard
output shows every field name, but the value of any identifier or secret field
(IMEI, ICCID, MDN, MEID, IMSI, MAC, serial, password, key, token) is replaced by
its length, so it can be pasted into a document.
"""

from __future__ import annotations

import argparse
import base64
import hashlib
import http.client
import json
import os
import re
import stat
import sys
import time
from pathlib import Path

# Read endpoints this tool may call. Each is a query the stock UI itself issues;
# none of them changes device state. Nothing outside this set is ever requested.
READ_ENDPOINTS = {
    "GetDeviceInfo": "/action/GetDeviceInfo",
    "GetRemoteAccessCfg": "/action/GetRemoteAccessCfg",
    "GetSimInfo": "/action/GetSimInfo",
    "GetLanCfgInfo": "/action/GetLanCfgInfo",
    "GetWanStatusInfo": "/action/GetWanStatusInfo",
    "GetBatteryInfo": "/action/GetBatteryInfo",
    "GetSimlockStatus": "/action/GetSimlockStatus",
}
# Named explicitly so that a future edit cannot add it by accident.
REFUSED = {"FotaGetVersionInfo": "asks the device to check for an OTA update"}

# Long, unambiguous tokens matched as a substring anywhere in the key.
SENSITIVE = re.compile(
    r"imei|iccid|mdn|meid|imsi|msisdn|bssid|serial|esn|passw|pwd|secret|token|"
    r"apn|gateway",
    re.IGNORECASE,
)
# Short, ambiguous tokens (`ip` is in "script"/"description", `sn` in "dns") are
# matched only as whole WORDS after splitting the key on camelCase, snake_case and
# digit boundaries -- so `wanIp`/`sim_pin`/`mac_addr` match but `description` does
# not.
_SENSITIVE_WORDS = {
    "sn",
    "mac",
    "key",
    "ssid",
    "phone",
    "pin",
    "puk",
    "ip",
    "ipv4",
    "ipv6",
    "addr",
    "address",
    "dns",
    "meid",
    "esn",
}
_KEY_SPLIT = re.compile(
    r"[_\-\s]+|(?<=[a-z])(?=[A-Z])|(?<=[A-Za-z])(?=\d)|(?<=\d)(?=[A-Za-z])"
)


def _key_is_sensitive(key: str) -> bool:
    if SENSITIVE.search(key):
        return True
    return any(p.lower() in _SENSITIVE_WORDS for p in _KEY_SPLIT.split(key) if p)


DEFAULT_PASSWORD_FILE = Path.home() / ".claude-secrets" / "orbic-admin.txt"


def swap_chars(text: str, pos1: int, pos2: int) -> str:
    chars = list(text)
    if pos1 < len(chars) and pos2 < len(chars):
        chars[pos1], chars[pos2] = chars[pos2], chars[pos1]
    return "".join(chars)


def apply_secret_swapping(text: str, secret_num: int) -> str:
    for i in range(4):
        byte = (secret_num >> (i * 8)) & 0xFF
        text = swap_chars(text, byte % len(text), i % len(text))
    return text


def encode_password(
    password: str, secret: str, timestamp: str, timestamp_start: int, now: int
) -> str:
    """Port of upstream `encode_password` (installer/src/orbic_auth.rs, v0.13.0).

    `now - timestamp_start` is the time elapsed since GetLoginInfo; upstream reads
    the clock for `now`, and the u32 addition wraps as it does in a release build.
    """
    spliced = "a7" + hashlib.md5(password.encode("utf-8")).hexdigest()
    secret_num = int(secret, 16)
    spliced = apply_secret_swapping(spliced, secret_num)
    time_delta = (int(timestamp, 16) + (now - timestamp_start)) & 0xFFFFFFFF
    message = f"6137x{time_delta:x}:{spliced}"
    result = base64.b64encode(message.encode("utf-8")).decode("ascii")
    return apply_secret_swapping(result, secret_num)


# Reference vectors produced by upstream's own `encode_password` (built from
# orbic_auth.rs with md5 0.7.0 and base64_light 0.1.5, current time = start).
SELFTEST_VECTORS = [
    (
        "hunter22",
        "1a2b3c4d",
        "00ff00ff",
        "mMzEN3hmZjAwZNY6YTVhNzk1MDz1YzQzNmZlOTc2ZWIEOGU0NTRiNTM3MjAzjg==",
    ),
    (
        "correct-horse",
        "0",
        "5f5e1000",
        "zNjEN3g1ZjVlMTAwMDplYTcyYjYyZDE3MWJhODQ5MWRiNDk2MDgxYThiMjQ3Mzhh",
    ),
    (
        "x",
        "9f8e7d6c",
        "deadbeef",
        "N2VxN3hkZWFkYmElZjplNmE3ZDQ5NDYzMjY4YzgwMzRmNWM4NWQ0ZTE1NWM2NjE2",
    ),
    (
        "pässwörd",
        "ffffffff",
        "1",
        "yNjEN3gxOmZhNzE4NDFlNGJhNWUzN2QzMmJmYzc4NDU4YzY3MTRhZGU=",
    ),
]


def selftest() -> int:
    bad = 0
    for n, (password, secret, ts, want) in enumerate(SELFTEST_VECTORS, 1):
        got = encode_password(password, secret, ts, 1_700_000_000, 1_700_000_000)
        ok = got == want
        bad += not ok
        # Only the vector number is printed: the inputs are fixtures, but nothing here
        # should print anything shaped like a credential.
        print(
            f"  {'ok  ' if ok else 'FAIL'}  encode_password matches upstream (vector {n})"
        )
    for name in REFUSED:
        assert name not in READ_ENDPOINTS, name
    # Redaction: known-sensitive keys, and identifier-shaped values in ANY field,
    # are masked; ordinary status/version fields stay visible.
    red = safe_view(
        {
            "imei": "490154203237518",
            "softwareVersion": "ORB400L_V1.2.0_BVZRT",
            "retcode": 0,
            "note": "357812340123456",  # identifier shape, innocuous key
            "lanMac": "02:00:00:00:00:01",
            "deviceName": "RC400L",
            "meid": 490154203237518,  # integer identifier under a sensitive key
            "ports": 8080,  # short integer, kept
            "clients": ["02:00:00:00:00:01", "ok"],  # MAC inside a list, innocuous key
            "wanIp": "100.72.14.9",  # IPv4 value, camelCase key
            "apn": "vzwinternet",  # sensitive key
            "phone": "+1-555-123-4567",  # formatted phone
            "simPin": "1234",  # short value, camelCase sensitive key
            "lanIpv6": "2001:db8::1",  # compressed IPv6
            "hwVer": "V1.1",  # kept
            "upTime": "12:34:56",  # kept (not an identifier)
            "description": "a script to zip",  # kept (no sensitive word boundary)
        }
    )
    assert isinstance(red, dict)
    checks = [
        red["imei"].startswith("<redacted"),
        red["note"].startswith("<redacted"),
        red["lanMac"].startswith("<redacted"),
        red["softwareVersion"] == "ORB400L_V1.2.0_BVZRT",
        red["retcode"] == 0,
        red["deviceName"] == "RC400L",
        red["meid"].startswith("<redacted"),
        red["ports"] == 8080,
        red["clients"][0].startswith("<redacted") and red["clients"][1] == "ok",
        red["wanIp"].startswith("<redacted"),
        red["apn"].startswith("<redacted"),
        red["phone"].startswith("<redacted"),
        red["simPin"].startswith("<redacted"),
        red["lanIpv6"].startswith("<redacted"),
        red["hwVer"] == "V1.1",
        red["upTime"] == "12:34:56",
        red["description"] == "a script to zip",
    ]
    if not all(checks):
        print(f"  FAIL  safe_view redaction: {red}")
        bad += 1
    else:
        print("  ok    redaction masks identifiers (key denylist + value shape)")
    print(
        "all vectors match upstream"
        if not bad
        else f"{bad} vector(s) differ from upstream"
    )
    return 1 if bad else 0


def read_password(path: Path) -> str:
    mode = stat.S_IMODE(path.stat().st_mode)
    if mode & 0o077:
        raise SystemExit(
            f"refusing {path}: mode {mode:o} lets others read it (chmod 600)"
        )
    password = path.read_text(encoding="utf-8").rstrip("\r\n")
    if not password:
        raise SystemExit(f"{path} is empty")
    return password


class Session:
    """One HTTP connection per request: the GoAhead server mishandles keep-alive."""

    def __init__(self, ip: str) -> None:
        self.ip = ip
        self.cookie = ""

    def request(
        self, method: str, path: str, body: bytes | None = None
    ) -> tuple[int, dict]:
        conn = http.client.HTTPConnection(self.ip, 80, timeout=15)
        headers = {"Content-Type": "application/json", "Connection": "close"}
        if self.cookie:
            headers["Cookie"] = self.cookie
        try:
            conn.request(method, path, body=body, headers=headers)
            resp = conn.getresponse()
            set_cookie = resp.getheader("Set-Cookie")
            if set_cookie:
                self.cookie = set_cookie.split(";", 1)[0]
            raw = resp.read()
        finally:
            conn.close()
        try:
            return resp.status, json.loads(raw.decode("utf-8", "replace") or "{}")
        except json.JSONDecodeError:
            return resp.status, {"_non_json_bytes": len(raw)}

    def login(self, username: str, password: str) -> None:
        status, info = self.request("GET", "/goform/GetLoginInfo")
        if not isinstance(info, dict):
            raise SystemExit("GetLoginInfo did not return a JSON object")
        if status != 200 or info.get("retcode") != 0:
            raise SystemExit(
                f"GetLoginInfo failed (HTTP {status}, retcode {info.get('retcode')})"
            )
        parts = str(info.get("priKey", "")).split("x")
        if len(parts) != 2 or not all(re.fullmatch(r"[0-9A-Fa-f]+", p) for p in parts):
            raise SystemExit("GetLoginInfo returned an unexpected priKey format")
        secret, timestamp = parts
        start = int(time.time())
        body = json.dumps(
            {
                "username": hashlib.md5(username.encode("utf-8")).hexdigest(),
                "password": encode_password(
                    password, secret, timestamp, start, int(time.time())
                ),
            }
        ).encode("utf-8")
        status, result = self.request("POST", "/goform/login", body)
        if not isinstance(result, dict) or result.get("retcode") != 0:
            code = result.get("retcode") if isinstance(result, dict) else None
            raise SystemExit(
                "login failed: incorrect password"
                if code == 201
                else f"login failed (retcode {code})"
            )

    def logout(self) -> None:
        self.request("POST", "/action/logout", b"{}")


# A value that LOOKS like a device identifier, redacted even when its key name is
# not on the denylist (so an identifier in an unexpected field cannot leak): a
# long digit run (IMEI/ICCID/MDN/MEID), a MAC, or a long hex/token string.
# Searched (not anchored) so an identifier EMBEDDED in a longer string is caught.
IDENTIFIER_VALUE = re.compile(
    r"\d{10,}"  # IMEI/ICCID/MDN/MEID and other long digit runs
    r"|([0-9A-Fa-f]{2}[:-]){5}[0-9A-Fa-f]{2}"  # MAC/BSSID
    r"|\b\d{1,3}(\.\d{1,3}){3}\b"  # IPv4
    r"|[0-9A-Fa-f]{0,4}::[0-9A-Fa-f:]*"  # compressed IPv6 (fe80::1, 2001:db8::1)
    r"|([0-9A-Fa-f]{1,4}:){3,}[0-9A-Fa-f]{1,4}"  # full IPv6 (>= 4 colon groups)
    r"|\+\d[\d -]{6,}\d"  # international phone (+1-555-123-4567)
    r"|\b\d{3}[-.\s]\d{3}[-.\s]\d{4}\b"  # NANP phone (555-123-4567)
    r"|\b[0-9A-Fa-f]{8,}\b"  # 8+ hex: serial/ESN/hash
    r"|[0-9A-Za-z+/_-]{20,}"  # long opaque token
)


def _looks_like_identifier(value: object) -> bool:
    """A leaf value shaped like a device identifier (IMEI, MAC, IP, phone, hex
    serial, long token -- matched anywhere in the string), or an integer with 10+
    digits. Redacting too eagerly is acceptable here: stdout is meant to be
    pasteable into docs, so privacy fails closed."""
    if isinstance(value, bool):
        return False
    if isinstance(value, int):
        return len(str(abs(value))) >= 10
    return isinstance(value, str) and IDENTIFIER_VALUE.search(value) is not None


def _redacted(value: object) -> str:
    return f"<redacted, {len(str(value))} chars>"


def safe_view(obj: object, key_sensitive: bool = False) -> object:
    """Field names kept; a value is replaced by its length when its KEY is
    sensitive (denylist of key names) or its VALUE looks like an identifier
    (value-shape backstop). Key-sensitivity propagates into nested lists/dicts,
    and the value-shape check runs on every leaf -- including numbers and strings
    inside lists under an innocuous key -- so an identifier cannot hide in a
    container."""
    if isinstance(obj, dict):
        return {
            k: safe_view(v, key_sensitive or _key_is_sensitive(k))
            for k, v in obj.items()
        }
    if isinstance(obj, list):
        return [safe_view(v, key_sensitive) for v in obj]
    if (key_sensitive and obj not in ("", None)) or _looks_like_identifier(obj):
        return _redacted(obj)
    return obj


def main(argv: list[str]) -> int:
    ap = argparse.ArgumentParser(
        description="Read-only client for the RC400L stock web UI."
    )
    ap.add_argument(
        "--selftest",
        action="store_true",
        help="check the password encoding against upstream vectors",
    )
    ap.add_argument("--ip", default="192.168.1.1")
    ap.add_argument("--username", default="admin")
    ap.add_argument("--password-file", type=Path, default=DEFAULT_PASSWORD_FILE)
    ap.add_argument(
        "--out-dir",
        type=Path,
        help="write raw JSON here (mode 600); keep it under backups/",
    )
    ap.add_argument(
        "endpoints",
        nargs="*",
        metavar="ENDPOINT",
        help=", ".join(sorted(READ_ENDPOINTS)),
    )
    args = ap.parse_args(argv)

    if args.selftest:
        return selftest()
    if not args.endpoints:
        ap.error("name at least one ENDPOINT, or use --selftest")
    for name in args.endpoints:
        if name in REFUSED:
            ap.error(f"{name} is refused: it {REFUSED[name]}")
        if name not in READ_ENDPOINTS:
            ap.error(
                f"{name} is not an allowlisted read endpoint ({', '.join(sorted(READ_ENDPOINTS))})"
            )

    password = read_password(args.password_file)
    session = Session(args.ip)
    session.login(args.username, password)
    del password
    try:
        for name in args.endpoints:
            status, data = session.request("POST", READ_ENDPOINTS[name], b"{}")
            if args.out_dir:
                args.out_dir.mkdir(parents=True, exist_ok=True)
                target = args.out_dir / f"{name}.json"
                fd = os.open(target, os.O_WRONLY | os.O_CREAT | os.O_TRUNC, 0o600)
                os.fchmod(fd, 0o600)  # tighten a pre-existing file with a looser mode
                with os.fdopen(fd, "w", encoding="utf-8") as fh:
                    json.dump(data, fh, indent=2, sort_keys=True)
            print(f"== {name} (HTTP {status})")
            print(json.dumps(safe_view(data), indent=2, sort_keys=True))
    finally:
        try:
            session.logout()
        except OSError:
            pass  # a logout failure must not mask an earlier error
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
