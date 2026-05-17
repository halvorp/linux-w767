#!/bin/bash
# Rapid out-of-tree driver iteration loop.
#
# Given a module name (e.g. ath10k_pci, hid-samsung) and a driver-source path,
# this script:
#   1. Rebuilds the module out-of-tree against ../linux (cross aarch64).
#   2. rsyncs the resulting .ko to /lib/modules/<kver>/extra/ on the device.
#   3. Runs `depmod -a` remotely.
#   4. Calls w767_ctl_cli rmmod + modprobe via SSH.
#   5. Tails dmesg.
#
# Usage:
#   W767_HOST=root@10.0.0.12 ./iterate-driver.sh ath10k_pci drivers/net/wireless/ath/ath10k
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
PROJECT_DIR="$(dirname "$SCRIPT_DIR")"
REPO_ROOT="$(dirname "$PROJECT_DIR")"
LINUX_SRC="${LINUX_SRC:-$REPO_ROOT/linux}"

W767_HOST="${W767_HOST:-}"
SSH_OPTS="${SSH_OPTS:--o StrictHostKeyChecking=accept-new}"

if [ $# -lt 2 ]; then
    echo "Usage: W767_HOST=root@<ip> $0 <module-name> <drivers-path>"
    echo "  e.g.  W767_HOST=root@10.0.0.12 $0 ath10k_pci drivers/net/wireless/ath/ath10k"
    exit 1
fi

MODNAME="$1"
SRC_PATH="$2"

if [ -z "$W767_HOST" ]; then
    echo "error: set W767_HOST=user@ip"; exit 1
fi
if [ ! -d "$LINUX_SRC/$SRC_PATH" ]; then
    echo "error: $LINUX_SRC/$SRC_PATH missing"; exit 1
fi

# Find the running kernel version on the device.
KVER="$(ssh $SSH_OPTS "$W767_HOST" 'uname -r')"
echo "=== iterate-driver: $MODNAME (kver $KVER) ==="

# Build out-of-tree against the Phase-1 build dir (has the kernel's headers +
# Module.symvers + config).
BUILD_DIR="$PROJECT_DIR/kernel/out/w767"
if [ ! -f "$BUILD_DIR/Module.symvers" ]; then
    # Module.symvers is produced by the full build; symlink from linux tree
    # if build-kernel.sh hasn't copied it out yet.
    if [ -f "$LINUX_SRC/Module.symvers" ]; then
        cp "$LINUX_SRC/Module.symvers" "$BUILD_DIR/Module.symvers"
    fi
fi

export ARCH=arm64
export CROSS_COMPILE="aarch64-linux-gnu-"

echo "-- compiling $SRC_PATH --"
make -C "$LINUX_SRC" M="$SRC_PATH" modules -j"$(nproc)"

KO_FILES=$(find "$LINUX_SRC/$SRC_PATH" -maxdepth 1 -name '*.ko' -newer "$LINUX_SRC/Makefile")
if [ -z "$KO_FILES" ]; then
    echo "warning: no freshly built *.ko found under $SRC_PATH (maybe all cached?)"
    KO_FILES=$(find "$LINUX_SRC/$SRC_PATH" -maxdepth 1 -name '*.ko')
fi

echo "-- pushing modules --"
ssh $SSH_OPTS "$W767_HOST" "mkdir -p /lib/modules/$KVER/extra"
for ko in $KO_FILES; do
    rsync -e "ssh $SSH_OPTS" -a "$ko" "$W767_HOST:/lib/modules/$KVER/extra/"
done
ssh $SSH_OPTS "$W767_HOST" "depmod -a $KVER"

echo "-- reloading $MODNAME --"
ssh $SSH_OPTS "$W767_HOST" "/opt/w767/bin/w767_ctl_cli rmmod $MODNAME --force || true"
ssh $SSH_OPTS "$W767_HOST" "/opt/w767/bin/w767_ctl_cli modprobe $MODNAME"
echo "-- dmesg tail --"
ssh $SSH_OPTS "$W767_HOST" "/opt/w767/bin/w767_ctl_cli dmesg-tail 80"
