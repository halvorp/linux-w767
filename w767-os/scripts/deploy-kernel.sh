#!/bin/bash
# Deploy a built kernel image + DTB + modules to the W767 over SSH.
#
# Runs from the host; assumes SSH access to the device (typically via
# USB-Ethernet + the Fedora iter-17 NetworkManager config, i.e. root@<ip>).
#
# Usage:
#   W767_HOST=root@10.0.0.12 ./deploy-kernel.sh --target w767
#   W767_HOST=root@10.0.0.12 ./deploy-kernel.sh --target w767-initramfs
#
# What gets copied and where:
#   Image     -> /boot/vmlinuz-<target>
#   DTB       -> /boot/dtb-<target>/qcom/sc8180x-samsung-w767.dtb
#   modules/  -> /lib/modules/<kernel-version>/
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
PROJECT_DIR="$(dirname "$SCRIPT_DIR")"

TARGET="${TARGET:-}"
W767_HOST="${W767_HOST:-}"
SSH_OPTS="${SSH_OPTS:--o StrictHostKeyChecking=accept-new}"

while [[ $# -gt 0 ]]; do
    case "$1" in
        --target)  TARGET="$2"; shift 2 ;;
        --host)    W767_HOST="$2"; shift 2 ;;
        *)         echo "Unknown option: $1"; exit 1 ;;
    esac
done

if [ -z "$TARGET" ] || [ -z "$W767_HOST" ]; then
    echo "Usage: W767_HOST=root@<ip> $0 --target w767|w767-initramfs"
    exit 1
fi

OUT_DIR="$PROJECT_DIR/kernel/out/$TARGET"
if [ ! -f "$OUT_DIR/Image" ]; then
    echo "ERROR: $OUT_DIR/Image missing — run ./kernel/build-kernel.sh --target $TARGET"
    exit 1
fi

KVER="$(cat "$OUT_DIR/version")"
echo "=== Deploying $TARGET kernel $KVER to $W767_HOST ==="

# Copy Image
echo "-> /boot/vmlinuz-$TARGET"
rsync -e "ssh $SSH_OPTS" -a "$OUT_DIR/Image" "$W767_HOST:/boot/vmlinuz-$TARGET"

# Copy DTB (keeps iter-17's DTB at /boot/dtb-7.0.0-62.fc45.aarch64/... untouched)
echo "-> /boot/dtb-$TARGET/qcom/"
ssh $SSH_OPTS "$W767_HOST" "mkdir -p /boot/dtb-$TARGET/qcom"
rsync -e "ssh $SSH_OPTS" -a "$OUT_DIR/dtb/qcom/" "$W767_HOST:/boot/dtb-$TARGET/qcom/"

# Copy modules
if [ -d "$OUT_DIR/lib/modules/$KVER" ]; then
    echo "-> /lib/modules/$KVER/  (rsync -a with depmod on the device)"
    ssh $SSH_OPTS "$W767_HOST" "mkdir -p /lib/modules/$KVER"
    rsync -e "ssh $SSH_OPTS" -a --delete \
        "$OUT_DIR/lib/modules/$KVER/" \
        "$W767_HOST:/lib/modules/$KVER/"
    ssh $SSH_OPTS "$W767_HOST" "depmod -a $KVER"
fi

# Copy config for reference
rsync -e "ssh $SSH_OPTS" -a "$OUT_DIR/config" "$W767_HOST:/boot/config-$TARGET"

echo ""
echo "=== Done. Install a BLS entry on the device to boot this kernel. ==="
echo "   -> ./install-bls-entry.sh  (runs remotely via SSH)"
