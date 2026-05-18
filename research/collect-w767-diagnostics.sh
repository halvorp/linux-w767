#!/bin/sh
# W767 diagnostic collector
# Run from the busybox shell after the initramfs drops to /bin/sh:
#   mount /dev/sda1 /mnt/esp
#   sh /mnt/esp/collect.sh
# Output lands in /mnt/esp/collect-<timestamp>/

set -u

ESP=/mnt/esp
if [ ! -d "$ESP" ]; then
    mkdir -p "$ESP"
    mount -t vfat /dev/sda1 "$ESP" 2>/dev/null || true
fi

TS=$(cat /proc/uptime | cut -d. -f1)
OUT="$ESP/collect-$(date +%Y%m%d-%H%M%S 2>/dev/null || echo "ts$TS")"
mkdir -p "$OUT"

cd "$OUT"

# Boot / kernel / version
dmesg                                            > dmesg.txt           2>&1
cat /proc/cmdline                                > cmdline.txt
uname -a                                         > uname.txt
cat /proc/version                                > version.txt
cat /proc/uptime                                 > uptime.txt
cat /proc/loadavg                                > loadavg.txt
cat /proc/meminfo                                > meminfo.txt 2>&1

# Hardware enumeration
cat /proc/cpuinfo                                > cpuinfo.txt
cat /proc/interrupts                             > interrupts.txt
cat /proc/iomem                                  > iomem.txt
cat /proc/ioports                                > ioports.txt 2>&1
cat /proc/modules                                > modules.txt 2>&1
cat /proc/devices                                > devices.txt

# Bus enumeration
ls -laR /sys/bus/usb/devices/                    > usb-tree.txt        2>&1
ls -la  /sys/bus/i2c/devices/                    > i2c-devices.txt     2>&1
ls -la  /sys/bus/spi/devices/                    > spi-devices.txt     2>&1
ls -la  /sys/bus/platform/devices/               > platform-devices.txt 2>&1
ls -la  /sys/bus/platform/drivers/               > platform-drivers.txt 2>&1
ls -la  /sys/bus/auxiliary/devices/              > aux-devices.txt     2>&1

# Device classes (network, drm, remoteproc, etc.)
ls -la  /sys/class/net/                          > net.txt             2>&1
ls -la  /sys/class/drm/                          > drm.txt             2>&1
ls -la  /sys/class/typec/                        > typec.txt           2>&1
ls -la  /sys/class/remoteproc/                   > remoteproc.txt      2>&1
ls -la  /sys/class/block/                        > block.txt           2>&1
ls -la  /sys/class/mmc_host/                     > mmc.txt             2>&1
ls -la  /sys/class/spi_master/                   > spi.txt             2>&1
ls -la  /sys/class/sound/                        > sound.txt           2>&1
ls -la  /sys/class/iio/                          > iio.txt             2>&1
ls -la  /sys/class/input/                        > input.txt           2>&1
ls -la  /sys/class/hidraw/                       > hidraw.txt          2>&1

# Detailed remoteproc state
for rp in /sys/class/remoteproc/*/; do
    [ -d "$rp" ] || continue
    n=$(basename "$rp")
    {
        echo "=== $n ==="
        echo "name:     $(cat "$rp/name"     2>/dev/null)"
        echo "state:    $(cat "$rp/state"    2>/dev/null)"
        echo "firmware: $(cat "$rp/firmware" 2>/dev/null)"
        ls -la "$rp" 2>/dev/null
    } >> remoteproc-detail.txt
done

# Debugfs (best-effort; ramoops/regulator/devices_deferred)
mkdir -p debugfs
cat /sys/kernel/debug/devices_deferred           > debugfs/deferred.txt              2>/dev/null
cat /sys/kernel/debug/regulator/regulator_summary > debugfs/regulator-summary.txt    2>/dev/null
cat /sys/kernel/debug/clk/clk_summary            > debugfs/clk-summary.txt           2>/dev/null
cat /sys/kernel/debug/qrtr/nodes                 > debugfs/qrtr-nodes.txt            2>/dev/null
cat /sys/kernel/debug/clk/clk_orphan_summary     > debugfs/clk-orphan-summary.txt    2>/dev/null
ls -laR /sys/kernel/debug/qcom_aoss/             > debugfs/qcom-aoss.txt             2>/dev/null

# Daemon logs (iter-35+ initramfs writes these)
[ -d /var/log ] && cp -r /var/log var-log 2>/dev/null

# Process snapshot
ps                                               > ps.txt              2>&1
pgrep -l . 2>/dev/null                           > pgrep.txt           2>&1

# Mounts and filesystems
mount                                            > mount.txt
cat /proc/filesystems                            > filesystems.txt
cat /proc/partitions                             > partitions.txt
df                                               > df.txt              2>&1

# Network
cat /proc/net/dev                                > net-dev.txt 2>&1
ip link                                          > ip-link.txt        2>&1 || true
ip addr                                          > ip-addr.txt        2>&1 || true

# pstore (kernel-saved oops/panic from previous boot if any)
mkdir -p pstore
cp -r /sys/fs/pstore/* pstore/ 2>/dev/null
[ -z "$(ls pstore 2>/dev/null)" ] && rmdir pstore

# Sync and finish
sync
cd /
echo ""
echo "==============================================="
echo "  collect.sh DONE — output in $OUT"
echo "  $(ls "$OUT" | wc -l) files written"
echo "  power down + bring drive back for analysis"
echo "==============================================="
