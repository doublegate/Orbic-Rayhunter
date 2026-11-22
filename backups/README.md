# backups/

Local-only destination for device backups (partition dumps, `/data` archives, stock config).
Everything in this directory except this README is gitignored: dumps contain proprietary
firmware and device identifiers (IMEI, MAC addresses, carrier provisioning) and must never be
committed or uploaded.

Keep a second copy off this machine. Procedure and verification:
[docs/BACKUP-AND-RECOVERY.md](../docs/BACKUP-AND-RECOVERY.md).
