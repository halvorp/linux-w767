#!/bin/bash
# Push the Phase-2 initramfs to the device.
#
# Usage:
#   W767_HOST=root@10.0.0.12 ./deploy-initramfs.sh
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
PROJECT_DIR="$(dirname "$SCRIPT_DIR")"

W767_HOST="${W767_HOST:-}"
SSH_OPTS="${SSH_OPTS:--o StrictHostKeyChecking=accept-new}"
IMG="$PROJECT_DIR/kernel/out/w767-initramfs.img"

while [[ $# -gt 0 ]]; do
    case "$1" in
        --host)  W767_HOST="$2"; shift 2 ;;
        --img)   IMG="$2"; shift 2 ;;
        *)       echo "Unknown option: $1"; exit 1 ;;
    esac
done

if [ -z "$W767_HOST" ]; then
    echo "Usage: W767_HOST=root@<ip> $0"; exit 1
fi
if [ ! -f "$IMG" ]; then
    echo "ERROR: $IMG missing — run ./initramfs/build-initramfs.sh"; exit 1
fi

echo "=== Deploying initramfs $(du -h "$IMG" | cut -f1) to $W767_HOST ==="
rsync -e "ssh $SSH_OPTS" -a "$IMG" "$W767_HOST:/boot/w767-initramfs.img"
echo "Done."
