#!/bin/bash
# Rootless iter-29 USB image builder. Same approach as
# /tmp/build-w767-image-rootless.sh but reads iter-29 initramfs and writes
# iter-29 BLS entry.

set -euo pipefail

REPO=/home/peter/Documents/linux-w767
KOUT="$REPO/w767-os/kernel/out/w767-initramfs"
IMAGE_KERN="$KOUT/Image"
DTB="$KOUT/dtb/qcom/sc8180x-samsung-w767.dtb"
INITRAMFS="$REPO/w767-os/kernel/out/w767-initramfs-iter29.img"
SD_BOOT="/tmp/sd-boot-aa64/usr/lib/systemd/boot/efi/systemd-bootaa64.efi"
OUT="${1:-/tmp/w767-iter29.img}"
ESP_MB=768   # larger ESP to hold ~19 MB initramfs + kernel + dumps

for f in "$IMAGE_KERN" "$DTB" "$INITRAMFS" "$SD_BOOT"; do
  [ -f "$f" ] || { echo "missing: $f"; exit 1; }
done

echo "=== Creating sparse image $OUT ($((ESP_MB + 16)) MiB) ==="
rm -f "$OUT"
truncate -s "$((ESP_MB + 16))M" "$OUT"

echo "=== sgdisk: GPT + ESP partition ==="
sgdisk --zap-all "$OUT" >/dev/null
sgdisk --new=1:2048:+${ESP_MB}MiB --typecode=1:EF00 --change-name=1:"W767-ESP" "$OUT" >/dev/null

SECT=$(sgdisk -p "$OUT" 2>/dev/null | awk '/Sector size \(logical\)/ {print $4}')
FIRST=$(sgdisk -i 1 "$OUT" 2>/dev/null | awk '/First sector:/ {print $3}')
LAST=$(sgdisk -i 1 "$OUT"  2>/dev/null | awk '/Last sector:/  {print $3}')
PART_SECTORS=$((LAST - FIRST + 1))
PART_BYTES=$((PART_SECTORS * SECT))
echo "  partition 1: first=$FIRST last=$LAST  ($PART_SECTORS sectors)"

echo "=== mkfs.vfat --offset=$FIRST ==="
mkfs.vfat -F32 -n "W767-ESP" --offset="$FIRST" "$OUT" $((PART_BYTES / 1024)) >/dev/null

echo "=== Populating ESP with mtools ==="
export MTOOLS_SKIP_CHECK=1
MCONF=$(mktemp)
cat > "$MCONF" <<EOF
drive z:
  file="$OUT"
  offset=$((FIRST * SECT))
  mtools_skip_check=1
EOF
export MTOOLSRC="$MCONF"

mmd  z:/EFI
mmd  z:/EFI/BOOT
mmd  z:/EFI/systemd
mmd  z:/loader
mmd  z:/loader/entries

mcopy "$SD_BOOT" z:/EFI/systemd/systemd-bootaa64.efi
mcopy "$SD_BOOT" z:/EFI/BOOT/BOOTAA64.EFI
mcopy "$IMAGE_KERN" z:/Image
mcopy "$DTB" z:/sc8180x-samsung-w767.dtb
mcopy "$INITRAMFS" z:/w767-initramfs.img

LOADER=$(mktemp)
cat > "$LOADER" <<'EOF'
default w767-initramfs
timeout 3
editor  yes
EOF
mcopy "$LOADER" z:/loader/loader.conf

BLS=$(mktemp)
cat > "$BLS" <<'EOF'
title    Samsung Galaxy Book S (W767) — iter-29 WiFi + USB-storage + shell
version  iter-29
linux    /Image
initrd   /w767-initramfs.img
devicetree /sc8180x-samsung-w767.dtb
options  console=tty0 loglevel=8 consoleblank=0 nomodeset rdinit=/init earlycon=efifb keep_bootcon net.ifnames=0 panic=10 clk_ignore_unused pd_ignore_unused arm64.nopauth efi=noruntime iommu.passthrough=0 iommu.strict=0 pcie_aspm.policy=powersupersave
EOF
mcopy "$BLS" z:/loader/entries/w767-initramfs.conf

echo "=== ESP contents ==="
mdir -/ z:/ | tail -20

rm -f "$LOADER" "$BLS" "$MCONF"
unset MTOOLSRC

echo "=== Checksums ==="
md5sum "$IMAGE_KERN" "$INITRAMFS" "$DTB" "$SD_BOOT"

echo ""
echo "=== Image ready: $OUT ==="
ls -lh "$OUT"
echo ""
echo "Flash with:"
echo "  sudo dd if=$OUT of=/dev/sda bs=4M status=progress conv=fsync && sync"
