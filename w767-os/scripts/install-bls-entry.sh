#!/bin/bash
# Install (or refresh) a GRUB BLS entry on the device.
#
# Intended to be invoked **on the device** after deploy-kernel.sh / deploy-initramfs.sh.
# Keeps iter-17's default entry intact. Writes a new entry under
# /boot/loader/entries/w767-<target>.conf and regenerates grub.cfg.
#
# Usage (run on device as root):
#   ./install-bls-entry.sh --target w767
#   ./install-bls-entry.sh --target w767-initramfs
set -euo pipefail

TARGET=""
while [[ $# -gt 0 ]]; do
    case "$1" in
        --target) TARGET="$2"; shift 2 ;;
        *) echo "Unknown: $1"; exit 1 ;;
    esac
done
[ -n "$TARGET" ] || { echo "Usage: $0 --target w767|w767-initramfs"; exit 1; }

case "$TARGET" in
    w767)
        TEMPLATE_SRC="/root/w767-os/grub/w767-phase1.conf"
        ;;
    w767-initramfs)
        TEMPLATE_SRC="/root/w767-os/grub/w767-phase2.conf"
        ;;
    *) echo "Unknown target: $TARGET"; exit 1 ;;
esac

if [ ! -r "$TEMPLATE_SRC" ]; then
    echo "ERROR: template missing at $TEMPLATE_SRC (rsync the w767-os/ tree to /root first)"
    exit 1
fi

# Fill in the root-fs UUID so the Phase 1 entry mounts iter-17's Fedora rootfs.
ROOT_UUID="$(findmnt -n -o UUID /)"
[ -n "$ROOT_UUID" ] || { echo "ERROR: could not resolve / UUID"; exit 1; }

DEST="/boot/loader/entries/w767-$TARGET.conf"
sed -e "s|@ROOT_UUID@|$ROOT_UUID|g" "$TEMPLATE_SRC" > "$DEST"
chmod 0644 "$DEST"
echo "Installed BLS entry: $DEST"
echo "  (root UUID filled in as $ROOT_UUID)"

# Regenerate GRUB menu so the entry shows up. Fedora's path.
if command -v grub2-mkconfig >/dev/null 2>&1; then
    grub2-mkconfig -o /boot/grub2/grub.cfg >/dev/null
    echo "  grub2-mkconfig: OK"
elif command -v grub-mkconfig >/dev/null 2>&1; then
    grub-mkconfig -o /boot/grub/grub.cfg >/dev/null
fi

echo ""
echo "Next reboot still picks iter-17 by default."
echo "At the GRUB menu, select 'w767 Phase 1' or 'w767 Phase 2' to try the new entry."
