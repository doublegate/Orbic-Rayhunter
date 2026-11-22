# Performance

Metrics to baseline before any optimization (v1.4 rule: baseline first, T-OR-069), the method for
each, and the results tables. No result is entered without the command, build, date and
conditions that produced it. Empty cells mean "not measured yet"; nothing here is estimated.

## Why these metrics

The RC400L has a single Cortex-A7 core, very little free RAM (2-4 MB free was reported with
Rayhunter and Tailscale running), raw NAND flash, and a 3000 mAh battery whose telemetry is five
coarse buckets ([ref-docs/orbic-rc400l-hardware.md](../ref-docs/orbic-rc400l-hardware.md)
section 1, [ref-docs/rayhunter-upstream-overview.md](../ref-docs/rayhunter-upstream-overview.md)
section 3.1). Storage exhaustion has wedged the daemon and the hotspot (#1192). The metrics follow
those constraints.

## Metrics and method

| Metric | Unit | Method |
|---|---|---|
| Daemon CPU | percent of one core, mean and p95 over 10 min | `top -b -n 600 -d 1 \| grep rayhunter-daemon` in the shell, or sample `/proc/<pid>/stat` utime+stime once per second; report idle-LTE and active-LTE separately |
| Daemon RSS | KiB, steady state and peak | `grep -E 'VmRSS\|VmHWM' /proc/$(pidof rayhunter-daemon)/status` every 60 s for 1 h |
| Free RAM | KiB | `grep MemAvailable /proc/meminfo` (fall back to `MemFree` + `Cached` if absent) with and without the daemon running |
| Storage growth | MB per hour | `du -sk /data/rayhunter/qmdl` at t0 and t0+1 h, or sizes from `/api/qmdl-manifest`; also `df -k /data`; idle and active LTE separately |
| Log growth | KB per hour | `ls -l /data/rayhunter/rayhunter.log` at t0 and t0+1 h |
| Battery drain | percent per hour (bucketed) | unplugged from full; poll `/sys/kernel/chg_info/level` every 60 s and record the time of each bucket transition (100, 75, 50, 25, 10); drain per hour is derived from transition times, so report the bucket span used |
| Boot-to-recording | seconds | power button press to the first new `qmdl-manifest` entry; also `cat /proc/uptime` versus the daemon's first log line timestamp in `rayhunter.log` |
| Flash write volume | MB per hour | difference of `/sys/block/*/stat` or `/proc/diskstats` write sectors if UBI devices appear there (TBD (on-device)); otherwise storage growth as a lower bound |

Conditions recorded with every row: build (daemon SHA-256), config differences from
[CONFIGURATION.md](CONFIGURATION.md) (`ui_level`, `wifi_enabled`, analyzers), SIM state (none,
inactive, active data), number of hotspot clients, signal (RSRP from the recording if available),
ambient temperature if known.

Rules: three runs minimum per condition; report the median and the spread; change one variable at
a time; no optimization patch without a before and after row in the same change.

## Results: runtime

Baseline target: the v1.0.0 build (T-OR-043), then re-baselined on the v1.3 build (T-OR-069).

| Metric | Condition | Build | Date | Runs | Result | Command / notes |
|---|---|---|---|---|---|---|
| Daemon CPU | idle LTE, 0 clients | | | | | |
| Daemon CPU | active LTE, 1 client | | | | | |
| Daemon RSS | idle LTE | | | | | |
| Daemon RSS | active LTE | | | | | |
| Free RAM | daemon stopped | | | | | |
| Free RAM | daemon running | | | | | |
| Storage growth | idle LTE | | | | | |
| Storage growth | active LTE | | | | | |
| Log growth | idle LTE | | | | | |
| Battery drain | `ui_level = 1`, 0 clients | | | | | |
| Battery drain | `ui_level = 0`, 0 clients | | | | | |
| Battery drain | daemon not installed (stock reference) | | | | | |
| Boot-to-recording | cold boot | | | | | |
| Flash write volume | active LTE | | | | | |

## Results: build artifacts

Reproducibility comparison for T-OR-026 ([BUILD.md](BUILD.md)). The first row is a local
measurement of the artifacts currently in `dist/`.

| Artifact | Profile | Source | Size (bytes) | Stripped | SHA-256 | Date |
|---|---|---|---|---|---|---|
| `rayhunter-daemon` | `firmware-devel` | this repo, empty patch series, v0.13.0 | 16291376 | no | `1bad88b8...a091c` | 2026-11-22 |
| `rootshell` | `firmware-devel` | this repo, empty patch series, v0.13.0 | 561124 | no | `754f9858...4b08d` | 2026-11-22 |
| `rayhunter-daemon` | `firmware` | this repo, empty patch series, v0.13.0 | | | | |
| `rayhunter-daemon` | `firmware` | upstream release v0.13.0 (from the installer zip) | | | | |

Method: `ls -l dist/`, `file dist/*`, `sha256sum dist/*`; for the comparison also
`arm-linux-musleabihf-readelf -S` (or `llvm-readelf -S`) section sizes and `cmp` for byte
identity. Byte identity is expected only if toolchain, flags, paths and the embedded web assets
match; any difference is explained in the T-OR-026 record.

## Results: soak

Filled by T-OR-102 (72 h continuous recording, hourly samples of every runtime metric).

| Hour | CPU | RSS | Free RAM | `/data` free | Battery bucket | Notes |
|---|---|---|---|---|---|---|
| | | | | | | |
