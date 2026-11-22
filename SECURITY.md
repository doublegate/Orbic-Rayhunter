# Security Policy

## Reporting a vulnerability

Report vulnerabilities privately through GitHub's
[private vulnerability reporting](https://github.com/doublegate/Orbic-Rayhunter/security/advisories/new).
Do not open a public issue. Expect an acknowledgement within 7 days.

If the issue is in upstream Rayhunter rather than in this project's patches or tooling, report it
to EFF as described in the
[upstream security policy](https://github.com/EFForg/rayhunter/blob/main/SECURITY.md); we will
coordinate if it affects both.

## Scope

In scope: `crates/`, `scripts/`, `patches/`, `device/` configuration, CI workflows, and the
install/backup procedures documented in `docs/`.

Notable risk areas: the root shell used for installation, the device web UI and its
authentication, handling of captured identifiers (IMSI/IMEI) and the update path.
The threat model is in [docs/SECURITY-MODEL.md](docs/SECURITY-MODEL.md).

## Supported versions

Only the latest release and `main` receive fixes before v1.0.0.
