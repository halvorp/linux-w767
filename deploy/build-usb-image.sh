#!/bin/bash
# Build a UEFI-bootable USB stick for the Samsung Galaxy Book S (W767).
#
# Layout produced on the target block device:
#   GPT partition table
#   p1: ESP, FAT32, ~512 MiB, contains:
#     /EFI/BOOT/BOOTAA64.EFI      <- systemd-bootaa64.efi (default UEFI fallback path)
#     /EFI/systemd/systemd-bootaa64.efi
#     /loader/loader.conf
#     /loader/entries/w767-initramfs.conf
#     /Image                      <- kernel
#     /sc8180x-samsung-w767.dtb   <- iter-19 DTB
#     /w767-initramfs.img         <- minimal initramfs distro
#
# Inputs (from prior build steps):
#   $W767_OS/kernel/out/w767-initramfs/Image
#   $W767_OS/kernel/out/w767-initramfs/dtb/qcom/sc8180x-samsung-w767.dtb
#   $W767_OS/kernel/out/w767-initramfs.img
#
# Usage:
#   ./build-usb-image.sh --device /dev/sdX             # writes directly to a block device (DESTRUCTIVE)
#   ./build-usb-image.sh --image  /tmp/w767.img        # writes a sparse image file (safer)

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
PROJECT_DIR="$(dirname "$SCRIPT_DIR")"      # w767-os/
REPO_ROOT="$(dirname "$PROJECT_DIR")"       # GalaxyBookS_Linux/

KERNEL_OUT="$PROJECT_DIR/kernel/out/w767-initramfs"
IMAGE="$KERNEL_OUT/Image"
DTB="$KERNEL_OUT/dtb/qcom/sc8180x-samsung-w767.dtb"
INITRAMFS="$PROJECT_DIR/kernel/out/w767-initramfs.img"
SD_BOOT="${SD_BOOT:-/tmp/sd-boot-aa64/usr/lib/systemd/boot/efi/systemd-bootaa64.efi}"

DEVICE=""
IMAGE_FILE=""
ESP_SIZE_MB=512

while [[ $# -gt 0 ]]; do
    case "$1" in
        --device) DEVICE="$2"; shift 2 ;;
        --image)  IMAGE_FILE="$2"; shift 2 ;;
        --esp-size) ESP_SIZE_MB="$2"; shift 2 ;;
        *) echo "Unknown: $1"; exit 1 ;;
    esac
done

[ -z "$DEVICE" ] && [ -z "$IMAGE_FILE" ] && {
    echo "ERROR: must pass --device /dev/sdX or --image /path/to/file"; exit 1; }
[ -n "$DEVICE" ] && [ -n "$IMAGE_FILE" ] && {
    echo "ERROR: pass --device OR --image, not both"; exit 1; }

# Verify input artifacts
for f in "$IMAGE" "$DTB" "$INITRAMFS" "$SD_BOOT"; do
    [ -f "$f" ] || { echo "ERROR: missing input: $f"; exit 1; }
done

echo "=== Inputs ==="
ls -la "$IMAGE" "$DTB" "$INITRAMFS" "$SD_BOOT"
echo ""

# Target: either a block device or a regular file
if [ -n "$IMAGE_FILE" ]; then
    TARGET="$IMAGE_FILE"
    echo "=== Creating sparse image file ($ESP_SIZE_MB MiB + slack) ==="
    truncate -s "$((ESP_SIZE_MB + 16))M" "$IMAGE_FILE"
    LOOP=$(sudo losetup -fP --show "$IMAGE_FILE")
    BLKDEV="$LOOP"
    echo "  loop: $LOOP"
else
    TARGET="$DEVICE"
    BLKDEV="$DEVICE"
    echo "=== TARGET BLOCK DEVICE: $DEVICE ==="
    echo "    Identity:"
    lsblk -o NAME,SIZE,TRAN,VENDOR,MODEL,LABEL,MOUNTPOINT "$DEVICE" | sed 's/^/    /'
    # Refuse to write to anything mounted, anywhere
    if findmnt -n "$DEVICE" >/dev/null 2>&1 || \
       lsblk -no MOUNTPOINT "$DEVICE" | grep -q . ; then
        echo "ERROR: $DEVICE or a partition of it is mounted. Unmount first."; exit 1
    fi
    # Refuse to write to the disk holding root
    ROOT_DISK="$(lsblk -no PKNAME "$(findmnt -no SOURCE /)" 2>/dev/null || true)"
    if [ -n "$ROOT_DISK" ] && [ "/dev/$ROOT_DISK" = "$DEVICE" ]; then
        echo "ERROR: $DEVICE is the system root disk. Refusing."; exit 1
    fi
    echo ""
    echo "    *** ABOUT TO WIPE $DEVICE IN 5 SECONDS *** (Ctrl-C to abort)"
    sleep 5
fi

echo ""
echo "=== Partitioning ==="
sudo sgdisk --zap-all "$BLKDEV"
sudo sgdisk \
    --new=1:0:+${ESP_SIZE_MB}MiB \
    --typecode=1:EF00 \
    --change-name=1:"W767-ESP" \
    "$BLKDEV"
sudo partprobe "$BLKDEV" 2>/dev/null || sudo blockdev --rereadpt "$BLKDEV" 2>/dev/null || true
sleep 1

# Resolve partition node (handles /dev/sda1 vs /dev/loop0p1)
if [ -n "$IMAGE_FILE" ]; then
    PART="${BLKDEV}p1"
else
    case "$BLKDEV" in
        /dev/nvme*|/dev/mmcblk*|/dev/loop*) PART="${BLKDEV}p1" ;;
        *)                                   PART="${BLKDEV}1"  ;;
    esac
fi
[ -b "$PART" ] || { echo "ERROR: $PART not found after partprobe"; exit 1; }

echo ""
echo "=== Formatting ESP ($PART) as FAT32 ==="
sudo mkfs.vfat -F32 -n "W767-ESP" "$PART"

echo ""
echo "=== Populating ESP ==="
MNT="$(mktemp -d)"
sudo mount "$PART" "$MNT"
trap 'sudo umount "$MNT" 2>/dev/null || true; rmdir "$MNT" 2>/dev/null || true' EXIT

sudo mkdir -p "$MNT/EFI/BOOT" "$MNT/EFI/systemd" "$MNT/loader/entries"

# systemd-boot in both the standard and fallback paths
sudo cp "$SD_BOOT" "$MNT/EFI/systemd/systemd-bootaa64.efi"
sudo cp "$SD_BOOT" "$MNT/EFI/BOOT/BOOTAA64.EFI"

# Kernel + DTB + initramfs at ESP root
sudo cp "$IMAGE" "$MNT/Image"
sudo cp "$DTB"   "$MNT/sc8180x-samsung-w767.dtb"
sudo cp "$INITRAMFS" "$MNT/w767-initramfs.img"

# systemd-boot loader.conf
sudo tee "$MNT/loader/loader.conf" > /dev/null <<'EOF'
default w767-initramfs
timeout 3
editor  yes
EOF

# BLS entry. Cmdline merges pmOS-canonical SC8180X family quirks + Phase 2
# initramfs-distro essentials.
sudo tee "$MNT/loader/entries/w767-initramfs.conf" > /dev/null <<'EOF'
title    Samsung Galaxy Book S (W767) — Phase 2 initramfs distro
version  iter-21
linux    /Image
initrd   /w767-initramfs.img
devicetree /sc8180x-samsung-w767.dtb
options  console=tty0 loglevel=7 rdinit=/init net.ifnames=0 panic=10 \
         earlycon=efifb keep_bootcon \
         clk_ignore_unused pd_ignore_unused arm64.nopauth efi=noruntime \
         iommu.passthrough=0 iommu.strict=0 pcie_aspm.policy=powersupersave
EOF

echo ""
echo "=== ESP contents ==="
sudo find "$MNT" -type f -printf '  %p  %s bytes\n' | sed "s|$MNT|/|"

echo ""
echo "=== Checksums ==="
( cd "$MNT" && sudo md5sum Image w767-initramfs.img sc8180x-samsung-w767.dtb EFI/BOOT/BOOTAA64.EFI )

echo ""
echo "=== Sync ==="
sync
sudo umount "$MNT"
rmdir "$MNT"
trap - EXIT

if [ -n "$IMAGE_FILE" ]; then
    sudo losetup -d "$LOOP"
    echo ""
    echo "=== Image file ready: $IMAGE_FILE ==="
    ls -lh "$IMAGE_FILE"
    echo "Flash with: sudo dd if=$IMAGE_FILE of=/dev/sdX bs=4M status=progress conv=fsync"
else
    echo ""
    echo "=== USB ready: $DEVICE ==="
    lsblk -o NAME,SIZE,FSTYPE,LABEL "$DEVICE"
fi
