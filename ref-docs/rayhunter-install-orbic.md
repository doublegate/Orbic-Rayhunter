# Rayhunter Install Procedure: Orbic RC400L

- Retrieved: 2026-11-22
- Upstream ref: v0.13.0 (cd462a1)
- Target: Orbic RC400L (Verizon), firmware ORB400L_V1.2.0_BVZRT, Qualcomm MDM9207
- Scope: the official install/update/uninstall procedure from `upstream/doc/*.md` and the
  structure of the installer in `upstream/installer/src/`. Community experience is in
  `community-installs.md`.

Nothing here was executed against a device; all behavior is read from the upstream source tree.

---

## 1. Installer generations (`doc/orbic.md`)

| Era | Command | Transport | Access path |
|---|---|---|---|
| < v0.3.0 | `install.sh` (shell + adb) | USB / ADB | USB mode switch + `/bin/rootshell` |
| v0.3.0 – v0.7.x | `./installer orbic` (Rust) | USB / ADB | same, reimplemented in Rust |
| v0.6.0 – v0.7.x | `./installer orbic-network` | WiFi or USB tethering, HTTP | web admin session |
| v0.8.0+ (current) | `./installer orbic` (the network installer; `orbic-network` is an alias). Old USB installer is now `./installer orbic-usb`. | network (HTTP) | web admin session |

Many third-party tutorials still describe the old routines. The v0.13.0 network installer refuses
to run without `--admin-password` and prints a message that any tutorial omitting it is outdated.

---

## 2. Prerequisites for a release install (`doc/installing-from-release.md`)

1. Download the `rayhunter-vX.Y.Z-<platform>.zip` asset (not "Source code (zip)", which has no
   prebuilt `installer`). Platforms: `linux-x64`, `linux-aarch64`, `linux-armv7`, `macos-intel`,
   `macos-arm`, `windows-x86_64`; each has a `.sha256`.
2. Unzip, `cd` into the folder. macOS: `xattr -d com.apple.quarantine installer`.
3. Power the Orbic on, connect the host via WiFi (hotspot) or USB tethering. You are on the right
   network when `http://192.168.1.1` loads the Orbic admin page.
4. Windows: use PowerShell, not CMD (quoting differences).

---

## 3. Network installer `./installer orbic` (default path)

### 3.1 Command line (`installer/src/lib.rs`, `OrbicNetworkArgs`)

```text
./installer orbic [--admin-ip <IP>] [--admin-username <USER>] --admin-password <PASS> \
                  [--reset-config] [--data-dir <PATH>]
```

| Flag | Default | Meaning |
|---|---|---|
| `--admin-ip` | `192.168.1.1` | Orbic web admin address |
| `--admin-username` | `admin` | Web admin user |
| `--admin-password` | required | Web admin password. On Verizon Orbic this is the WiFi password (device menu > 2.4 GHz WIFI Info). Kajeet/Smartspot default `$m@rt$p0tc0nf!g`. A hardware reset (hold the button under the back cover until restart) returns it to default. |
| `--reset-config` | off | Overwrite an existing `config.toml` on the device |
| `--data-dir` | `/data/rayhunter-data` | Real data directory. Must not be `/data/rayhunter` and must not contain `'`. |

Example invocations from the docs:

```bash
# Verizon Orbic
./installer orbic --admin-password 'mypassword'
# Kajeet/Smartspot
./installer orbic --admin-password '$m@rt$p0tc0nf!g'
```

### 3.2 How the installer gets a shell (`orbic_network.rs`, `orbic_auth.rs`)

At a high level, the network installer authenticates to the device's own web admin interface and
uses an authenticated admin request to start a temporary shell listener on the device, then drives
that shell to copy files. Key facts relevant to planning:

- It authenticates against `/goform/GetLoginInfo` and `/goform/login` using the Orbic web UI's own
  credential-hashing scheme (reimplemented in `orbic_auth.rs` using MD5 plus the server-provided
  `priKey`). A wrong password returns retcode 201 and the installer reports "incorrect password".
- After login it issues an authenticated admin request (to `/action/SetRemoteAccessCfg`) that
  causes the device to launch a BusyBox `nc`-based shell listener. Stock Verizon Orbic firmware has
  no telnetd, so `nc` is used; some Kajeet units have a password-protected telnetd on port 23, so
  the installer deliberately uses **port 24** (`TELNET_PORT = 24`).
- `wait_for_telnet` polls port 24 once a second for up to **60 seconds** for the shell to come up,
  then fails with a timeout.
- This path never touches USB or ADB. It does not install `/bin/rootshell`; the shell listener
  already runs with sufficient privilege to write the needed files.

This is the device's own vendor authentication plus a known command-injection weakness in the
Orbic web firmware; it is the mechanism EFF ships, not something this project adds. It is
firmware-dependent (see section 7).

### 3.3 What the installer writes (`orbic_network.rs::setup_rayhunter`, `connection.rs`)

Over the shell, in order:

1. `mount -o remount,rw /dev/ubi0_0 /` (only strictly needed on Moxee, harmless on Orbic).
2. `setup_data_directory`: create `--data-dir` (default `/data/rayhunter-data`) and symlink
   `/data/rayhunter -> <data-dir>`. If `/data/rayhunter` was already a real directory or a symlink
   to a different target (older installs), it stops the daemon and migrates existing data with
   `mv` before creating the new symlink. Refuses if both old dir and new dir exist.
3. `mkdir -p /data/rayhunter/scripts /data/rayhunter/bin`.
4. Push the daemon to `/data/rayhunter/rayhunter-daemon`.
5. `install_wifi_tools`: push `wpa_supplicant`, `wpa_cli`, `iw` to `/data/rayhunter/bin/` only if
   each is not already present on the device (`command -v`). Skipped entirely if the installer was
   built without the wpa tools (debug builds).
6. `install_config`: write `/data/rayhunter/config.toml` from `dist/config.toml.in` with
   `device = "orbic"`, **only if it does not already exist** unless `--reset-config` is given.
7. Push `/etc/init.d/rayhunter_daemon` (from `dist/scripts/rayhunter_daemon`).
8. Push `/etc/init.d/misc-daemon` (from `dist/scripts/misc-daemon`) — this is the boot-persistence
   hook (it calls `rayhunter_daemon start` at boot).
9. `chmod +x` the daemon, `chmod 755` both init scripts.
10. `shutdown -r -t 1 now` to reboot.

File transfer method (`util.rs::telnet_send_file`): the device runs `nc -l -p 8081 > <file>.tmp`,
the installer connects to port 8081 and streams the bytes, waits ~1 s for the device to flush
(unflushed `nc` buffers are a known cause of corrupt transfers), verifies with `md5sum` on the
device against the expected checksum, then `mv <file>.tmp <file>`. Transfer timeout is
`30 s + 2 s/MB`. Connect retries use exponential backoff, max 4 attempts.

After reboot the web UI is at `http://192.168.1.1:8080`. A green line flashing along the top of the
device display confirms it is recording.

### 3.4 Obtaining a shell later

```bash
./installer util orbic-shell --admin-password <PASS>   # network, no ADB
```

`util orbic-start-telnet` just starts the listener without installing. Both take the same
`--admin-ip/--admin-username/--admin-password` arguments. The interactive shell is limited and may
not echo a prompt. For pre-0.7.0 or `orbic-usb` installs, use `adb shell` then `/bin/rootshell`.

---

## 4. USB/ADB installer `./installer orbic-usb` (not recommended)

`installer/src/orbic.rs`. The docs warn it can brick devices (especially on Windows) and is only
for users who specifically want persistent ADB. Outline:

1. `enable_command_mode`: a USB vendor control transfer (VID `0x05c6`, switch PID `0xf626`,
   request `0xa0`) puts the modem into a mode that exposes the serial/ADB interface (runtime PID
   `0xf601`, RNDIS variants `0xf622`). A USB stall during the reboot is treated as success.
2. `get_adb` waits (up to ~30 s per reconnect) for the ADB device and runs an echo test.
3. Waits for `atfwd_daemon` to start (`pgrep atfwd_daemon`) — AT commands are routed through it.
4. `setup_rootshell`: push `rootshell` to `/bin/rootshell`, then via `AT+SYSCMD=...`
   `chown root` and `chmod 4755` it (setuid root), and verify `rootshell -c id` reports `uid=0`.
5. `setup_rayhunter`: same file set as the network path (daemon, config, init scripts, wifi tools),
   but file pushes go through `adb push` to `/tmp` then `AT+SYSCMD=mv` into place, each verified by
   `sha256sum` (retried up to ~5 times); commands run through `/bin/rootshell -c`.
6. Reboot via `AT+SYSCMD=shutdown -r -t 1 now`, wait for shutdown then reconnect.
7. `test_rayhunter`: `wget http://localhost:8080/index.html` on the device, up to 10 tries, 3 s
   apart.

Because this path uses AT commands through `atfwd_daemon` and setuid `rootshell`, it is more
firmware- and timing-sensitive than the network path. It can also leave USB tethering disabled
(see section 6).

---

## 5. Updating (`doc/updating-rayhunter.md`)

Updating is identical to installing: re-run the installer with a newer release. The config is
preserved because `install_config` does not overwrite an existing `config.toml` unless
`--reset-config` is passed. `setup_data_directory` migrates data if the layout changed between
versions. The dev-loop equivalent is `make.sh` (build web, `cargo build-daemon-firmware-devel`,
stop daemon via rootshell, `adb push` the daemon, reboot) — ADB only, for `orbic-usb`-style access.

---

## 6. Uninstalling and tethering (`doc/uninstalling.md`, `doc/faq.md`)

There is no automated uninstaller. Manual Orbic routine:

```bash
./installer util orbic-shell --admin-password <PASS>
# inside the device shell:
echo 3 > /usrdata/mode.cfg   # only if previously installed via the ADB/USB installer
rm -rf /data/rayhunter /etc/init.d/rayhunter_daemon /bin/rootshell
reboot
```

Note that this removes the `/data/rayhunter` symlink but not the real `--data-dir`
(`/data/rayhunter-data`) created by the network installer; that directory is left behind and should
be removed too.

USB tethering state is controlled by `/usrdata/mode.cfg`: `echo 9 > /usrdata/mode.cfg; reboot`
re-enables tethering, `echo 3` disables it. `/etc/init.d/misc-daemon` was overwritten by the
installer, so a full cleanup should also restore the stock `misc-daemon` if tethering or other boot
services were affected. USB modes are listed in `/data/usb/boot_hsusb_composition`.

Hotspot WiFi can be disabled from the shell by editing
`/usrdata/data/usr/wlan/wlan_conf_6174.xml` (QCA6174 radio).

---

## 7. Firmware-version-specific notes (relevant to ORB400L_V1.2.0_BVZRT)

Upstream does not record per-firmware compatibility for the Orbic in the v0.13.0 tree. Observations
that matter for planning on V1.2.0_BVZRT:

- The network installer depends on two firmware-provided facts: the web admin auth scheme at
  `/goform/GetLoginInfo` + `/goform/login`, and the command-injection weakness reached through
  `/action/SetRemoteAccessCfg`. If a firmware build changes either the web API or patches the
  injection, the network installer stops working with no fallback other than `orbic-usb`. Confirm
  these endpoints behave as expected on V1.2.0_BVZRT before relying on the installer.
- The `orbic-usb` path depends on the USB mode-switch control transfer (PID `0xf626`, request
  `0xa0`), the presence of `atfwd_daemon`, and `AT+SYSCMD` being honored. These are firmware
  internals and can differ across builds.
- Device-facing paths assumed by the daemon on Orbic firmware: `/dev/diag`, `/dev/fb0` (128x128
  RGB565), `/dev/input/event0`, `/sys/kernel/chg_info/level` and `/chg_en` (battery),
  `/etc/init.d/`, `/data`, `/usrdata/mode.cfg`, `/usrdata/data/usr/wlan/wlan_conf_6174.xml`.
  If V1.2.0_BVZRT relocates any of these, the corresponding feature breaks (battery, display,
  input, tethering toggle).
- The Verizon admin password defaulting to the WiFi password is a Verizon-firmware convention; a
  carrier image could differ.
- Record the exact firmware string (ORB400L_V1.2.0_BVZRT) and the installer version used for every
  install attempt. The community notes in `community-installs.md` collect firmware-specific
  reports, including whether newer firmware blocks the exploit or ADB.

---

## Sources

- `upstream/doc/orbic.md`, `installation.md`, `installing-from-release.md`, `installing-from-source.md`, `updating-rayhunter.md`, `uninstalling.md`, `faq.md`, `using-rayhunter.md`, `supported-devices.md`
- `upstream/installer/src/lib.rs` (CLI definitions, `run`), `main.rs`, `orbic_network.rs`, `orbic_auth.rs`, `orbic.rs`, `connection.rs`, `util.rs`, `build.rs`
- `upstream/dist/config.toml.in`, `upstream/dist/scripts/rayhunter_daemon`, `upstream/dist/scripts/misc-daemon`
- `upstream/make.sh`
- <https://efforg.github.io/rayhunter/orbic.html>
