# Upstream install footprint on the RC400L (T-OR-020)

Read-only diff of the backed-up stock `/etc/init.d/misc-daemon`
(`backups/2026-11-23-V1.2.0/fs/root.tar`, SHA-256
`8f9bb4ce79eb878af49b366af2f784a44127736a62cce7992e4ab1c8fe3f1cf9`, 92 lines) against
upstream `dist/scripts/misc-daemon` at the pinned `v0.13.0` (SHA-256 `dc8a6e99...`, 102 lines).
Analyzed 2026-11-23; no device interaction.

## What the stock `installer orbic` changes

The installer replaces `/etc/init.d/misc-daemon` with upstream's, which differs only by hooking
the Rayhunter daemon into the existing start/stop paths:

```text
start):  + if [ -f /etc/init.d/rayhunter_daemon ]; then /etc/init.d/rayhunter_daemon start; fi
stop):   + if [ -f /etc/init.d/rayhunter_daemon ]; then /etc/init.d/rayhunter_daemon stop;  fi
```

Everything else in the script is byte-identical. The full install footprint is therefore:

1. `/etc/init.d/misc-daemon` -> upstream version (the two hook blocks above). Reversible by
   restoring the backed-up stock file (hash above).
2. `/etc/init.d/rayhunter_daemon` -> new init script (start/stop the daemon).
3. The `rayhunter-daemon` binary + config under `/data/rayhunter/` (writable `usrfs`).

All three live on the writable `ubifs` volumes captured in the Sprint 1.3 backup (`/` and
`/data`), so the install is fully reversible at file level: restore stock `misc-daemon`, remove
`rayhunter_daemon` and `/data/rayhunter/`, reboot. This is the basis for the Phase 2 install
(T-OR-022) and `orx uninstall`/rescue (T-OR-041). No bootloader/modem/partition write is
involved.
