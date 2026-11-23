# OTA / FOTA mechanism on ORB400L_V1.2.0_BVZRT (T-OR-020)

Read-only analysis of the backed-up stock firmware (no device interaction). Source:
`backups/2026-11-23-V1.2.0/fs/root.tar` (the verified Sprint 1.3 backup). Analyzed
2026-11-23. This note informs the OTA block (T-OR-018); it records the mechanism, not a
change applied to the device.

## Components (stock `root.tar`)

| Path | Role | SHA-256 (as backed up) |
|---|---|---|
| `/usr/bin/oma_dm` | OMA-DM client (ARM ELF, stripped) | `6b044dd61e8f59627bd44ee7c908d342b8d3f2d35d42f4f07046083d5cb7048d` |
| `/usr/bin/dmclient` | device-management client | `a273502b429e5248260387322e4c5894e02f1b6950d8a39ca3d6d71f4d2e3f29` |
| `/usr/bin/mdm-daemon` | modem DM daemon | `2aa41a632da12a19868b98a54966c7a25cca0f254ed8f9ad370fca7f5e83e7a1` |
| `/usr/bin/goahead` | stock web server (serves the `Fota*` endpoints) | `5d6ac5ba5dd4cd51f2f8a3a3b3c10a9a4ebb008e0f3c5e0ee6d0e81a882fa854` |
| `/etc/xml/fota/fota_cfg.xml` | FOTA policy config | (text, see below) |
| `/etc/xml/omadm/dmconfig.dat` | OMA-DM logging config only (`PRINT_MODE`/`PRINT_LEVEL`) | (text) |
| `/var/www/web/js/upgradefota.js` | UI OTA trigger (calls the `Fota*` goform endpoints) | (text) |

## Findings

1. **Auto-FOTA is OFF by default on this unit.** `fota_cfg.xml`:
   `<autocheck>0</autocheck>`, `<autodown>0</autodown>`, `<autoinstall>0</autoinstall>`
   (`forceInstallFlag=1`, `cirtime=1440`). The device does not poll for or auto-install
   updates on its own. The `<fota_srv_info>` URLs (`requesturl`, `reporturl`, `channel`,
   `brand`) are **empty** -- the FOTA server is provisioned dynamically, not statically.
2. **No persistent FOTA/DM daemon at boot.** Confirmed by the T-OR-014 recon process list
   and by the absence of any `/etc/init.d/*` or `/etc/rc*.d/` link referencing
   `oma_dm`/`dmclient`/`mdm-daemon`/`fota`. The updater runs only on demand.
3. **No static update URL in any binary.** `strings` over `oma_dm`, `dmclient`,
   `mdm-daemon`, `goahead` yields no `http(s)://`/carrier hostnames; and `dmconfig.dat`
   holds only logging keys. The DM/FOTA server address is provisioned at runtime (carrier
   OMA-DM), so there is **no fixed hostname to DNS-block** without observing a live DM
   session (which we deliberately do not trigger).
4. **Two trigger paths:**
   - **Stock web UI** -> `goahead` `/goform/Fota*` handlers. `upgradefota.js` calls
     `FotaGetVersionInfo`, `FotaGetCheckVersionInfo`, `FotaCheckNewVersion`, `FotaDownload`,
     `FotaUpgrade`, `FotaGetDownloadStatus`, `FotaGetUpgradeStatus`. Avoided by never
     invoking those UI actions; `scripts/orbic-ui-read.py` refuses the read-shaped ones.
   - **Carrier-pushed OMA-DM** -> Verizon initiates a DM session over the network, which can
     provision the FOTA URL and trigger download/install. Requires the SIM and Verizon
     network contact (matches upstream issue #163's `dmclient`).

## OTA-block plan (T-OR-018), derived

Auto-check already being off removes the polling risk; the residual risks are a manual UI
trigger and a carrier-pushed DM session. The block, in order of strength:

1. **Keep the SIM out** until the block is verified (no carrier network -> no OMA-DM push).
2. **Preserve `autocheck/autodown/autoinstall = 0`** in `fota_cfg.xml`; record its baseline
   SHA-256 and re-check it after any device work.
3. **Reversibly neutralize the on-demand updater** (a DEVICE WRITE, gated on the verified
   backup, done only after an explicit go-ahead): rename `oma_dm`, `dmclient`, `mdm-daemon`
   (and/or clear their exec bit), recording original name, path and hash so the change
   reverses exactly. Prefer rename over delete.
4. **Never invoke the UI `Fota*` actions**; do not open the stock UI in a browser (its page
   load historically triggers `FotaGetVersionInfo`).
5. **Verify:** SIM inserted, reboot, confirm no `oma_dm`/`dmclient`/`mdm-daemon` process,
   `fota_cfg.xml` unchanged, firmware string unchanged; re-check after several days.

Permanent suppression with a full documented reversal is T-OR-096. A raw image restore is not
available (NAND is UBI-backed; `nanddump` deadlocks -- see
[../../docs/BACKUP-AND-RECOVERY.md](../../docs/BACKUP-AND-RECOVERY.md)); recovery for this class
is a file-level restore of the renamed binaries from the backup.
