#!/bin/sh
# Sprint 1.2 follow-up: the checklist rows not covered by phase1-inventory.sh.
# A read-only command list consumed by scripts/orbic-shell.py (--script); see
# phase1-inventory.sh for the format and the read-only guarantee.
cat /sys/devices/system/cpu/cpu0/cpufreq/scaling_available_frequencies
cat /sys/devices/system/cpu/cpu0/cpufreq/scaling_governor
dmesg | grep -i -E "nand|ubi|mtd"
sha256sum /etc/init.d/misc-daemon
cat /etc/init.d/misc-daemon
ls -l /etc/init.d
ls -l /usrdata/data/qcmap/
cat /usrdata/mode_tmp.cfg
cat /data/usb/boot_hsusb_composition
netstat -tlnp
ls -l /etc/init.d/*fota* /etc/init.d/*ota* /etc/init.d/*update* /etc/init.d/*dm* 2>/dev/null
ls -l /bin/dnsmasq_script.sh /sbin/fota* /usr/bin/*fota* 2>/dev/null
cat /proc/mtd
