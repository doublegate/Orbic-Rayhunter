#!/bin/sh
# Read-only device inventory for Phase 1 Sprint 1.2 (T-OR-014).
#
# This is a command list, one read-only command per line, consumed by
# scripts/orbic-shell.py (--script), which runs each line over the device root
# shell and skips comment lines. It is also a valid POSIX sh script: run
# directly it simply reports the HOST, which is harmless. orbic-shell.py refuses
# any writer, so a recon run cannot modify the device before the Sprint 1.3
# backup.
cat /proc/mtd
cat /proc/mounts
df -h
ls -l /dev/mtd* /dev/mtdblock* /dev/ubi*
ubinfo -a
cat /proc/cmdline
cat /proc/version
cat /proc/cpuinfo
cat /sys/devices/soc0/machine
cat /sys/devices/soc0/soc_id
cat /sys/devices/soc0/build_id
cat /proc/meminfo
free
ps w
cat /proc/mounts
ls -la /etc/init.d
ls -la /etc/rc5.d
ls -la /data
ls -la /usrdata
cat /usrdata/mode.cfg
busybox | head -1
busybox --list
ls -la /bin /sbin /usr/bin /usr/sbin
cat /etc/version
cat /etc/ver
cat /proc/net/tcp
cat /proc/net/udp
ls -la /dev/*diag* 2>/dev/null
cat /sys/class/power_supply/*/uevent 2>/dev/null
cat /sys/kernel/chg_info/level 2>/dev/null
ls -la /sys/class/net
ls -la /tmp /var/run 2>/dev/null
cat /etc/passwd
# shellcheck disable=SC2009  # the device busybox has no pgrep; keep full ps output
ps w | grep -iE 'fota|update|ota|atfwd|goahead|qcmap|dnsmasq'
ls -la /etc/rc.d /etc/rcS.d 2>/dev/null
cat /proc/mtd
