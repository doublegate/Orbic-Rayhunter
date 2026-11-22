# device/

On-device overlay installed alongside upstream Rayhunter.

| Path | Installed to | Purpose |
|---|---|---|
| `config/config.orbic.toml` | `/data/rayhunter/config.toml` | Recommended configuration; every key exists upstream |

Init scripts, helper binaries and firmware tweaks are added here in later phases (see
`to-dos/`). Anything that replaces a stock file must ship with its restore procedure in
`docs/BACKUP-AND-RECOVERY.md`.
