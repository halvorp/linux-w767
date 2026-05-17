#!/bin/bash
# Build a w767-os kernel for the Samsung Galaxy Book S (SM-W767, SC8180X, aarch64).
#
# Usage:
#   ./kernel/build-kernel.sh --target w767            [--linux-src /path/to/linux]
#   ./kernel/build-kernel.sh --target w767-initramfs  [--linux-src /path/to/linux]
#
# Process:
#   1. Start from allnoconfig (everything off)
#   2. Copy our board DTS into the kernel tree
#   3. Merge base-arm64.config + {target}.config via scripts/kconfig/merge_config.sh
#   4. Run olddefconfig to resolve dependencies
#   5. Build Image + dtbs (+ modules if CONFIG_MODULES=y)
#   6. Install to kernel/out/{target}/ (Image, dtb, modules/, config, version)
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
PROJECT_DIR="$(dirname "$SCRIPT_DIR")"

# Defaults
TARGET=""
LINUX_SRC="$(dirname "$PROJECT_DIR")/linux"
JOBS=$(nproc)
CROSS_COMPILE_DEFAULT="aarch64-linux-gnu-"
CROSS_COMPILE="${CROSS_COMPILE:-$CROSS_COMPILE_DEFAULT}"
ARCH=arm64

REPO_ROOT="$(dirname "$PROJECT_DIR")"
DTS_SRC="$REPO_ROOT/dts-stage-v2/sc8180x-samsung-w767.dts"
DTS_BOARD_NAME="sc8180x-samsung-w767"
KERNEL_DTS_DIR_REL="arch/arm64/boot/dts/qcom"

# Parse args
while [[ $# -gt 0 ]]; do
    case "$1" in
        --target)      TARGET="$2"; shift 2 ;;
        --linux-src)   LINUX_SRC="$2"; shift 2 ;;
        -j)            JOBS="$2"; shift 2 ;;
        --cross)       CROSS_COMPILE="$2"; shift 2 ;;
        *)             echo "Unknown option: $1"; exit 1 ;;
    esac
done

if [ -z "$TARGET" ]; then
    echo "Usage: $0 --target w767|w767-initramfs [--linux-src /path/to/linux]"
    exit 1
fi

# Validate
BASE_CONFIG="$SCRIPT_DIR/base-arm64.config"
TARGET_CONFIG="$SCRIPT_DIR/${TARGET}.config"
OUT_DIR="$SCRIPT_DIR/out/$TARGET"

for f in "$BASE_CONFIG" "$TARGET_CONFIG"; do
    if [ ! -f "$f" ]; then
        echo "Error: config fragment not found at $f"
        exit 1
    fi
done
if [ ! -d "$LINUX_SRC" ]; then
    echo "Error: Linux source tree not found at $LINUX_SRC"
    echo "  clone with:  git clone --depth 1 --branch v7.0 \\"
    echo "                https://git.kernel.org/pub/scm/linux/kernel/git/torvalds/linux.git $LINUX_SRC"
    exit 1
fi
if [ ! -f "$LINUX_SRC/scripts/kconfig/merge_config.sh" ]; then
    echo "Error: not a valid Linux source tree (missing scripts/kconfig/merge_config.sh)"
    exit 1
fi
if [ ! -f "$DTS_SRC" ]; then
    echo "Error: board DTS not found at $DTS_SRC"
    exit 1
fi
if ! command -v "${CROSS_COMPILE}gcc" >/dev/null 2>&1; then
    echo "Error: cross compiler ${CROSS_COMPILE}gcc not in PATH"
    exit 1
fi

echo "=== w767-os kernel build ==="
echo "  Target:        $TARGET"
echo "  Linux src:     $LINUX_SRC"
echo "  ARCH:          $ARCH"
echo "  CROSS_COMPILE: $CROSS_COMPILE"
echo "  Jobs:          $JOBS"
echo "  Output:        $OUT_DIR"
echo ""

cd "$LINUX_SRC"
export ARCH CROSS_COMPILE

# Step 1: copy our DTS into the kernel tree (kernel expects it under arch/arm64/boot/dts/qcom/)
echo "=== Step 1: staging board DTS ==="
mkdir -p "$KERNEL_DTS_DIR_REL"
cp "$DTS_SRC" "$KERNEL_DTS_DIR_REL/${DTS_BOARD_NAME}.dts"
# Ensure the Makefile references our DTB. Append once if missing.
if ! grep -q "${DTS_BOARD_NAME}.dtb" "$KERNEL_DTS_DIR_REL/Makefile" 2>/dev/null; then
    echo "dtb-\$(CONFIG_ARCH_QCOM) += ${DTS_BOARD_NAME}.dtb" >> "$KERNEL_DTS_DIR_REL/Makefile"
    echo "  appended ${DTS_BOARD_NAME}.dtb to $KERNEL_DTS_DIR_REL/Makefile"
fi

# Step 2: allnoconfig baseline
echo ""
echo "=== Step 2: allnoconfig ==="
make allnoconfig

# Step 3: merge config fragments
echo ""
echo "=== Step 3: merge base-arm64.config + ${TARGET}.config ==="
KCONFIG_CONFIG=.config "$LINUX_SRC/scripts/kconfig/merge_config.sh" \
    -m .config "$BASE_CONFIG" "$TARGET_CONFIG"

# Step 4: resolve dependencies
echo ""
echo "=== Step 4: olddefconfig ==="
make olddefconfig

# Config summary
echo ""
echo "=== Config summary ==="
BUILTIN=$(grep -c '=y' .config || true)
MODULES=$(grep -c '=m' .config || true)
DISABLED=$(grep -c 'is not set' .config || true)
echo "  Built-in: $BUILTIN"
echo "  Modules:  $MODULES"
echo "  Disabled: $DISABLED"

verify_on() {
    local opt="$1"
    if grep -q "CONFIG_${opt}=y" .config || grep -q "CONFIG_${opt}=m" .config; then
        echo "  OK: $opt"
    else
        echo "  MISSING: $opt"
    fi
}

echo ""
echo "=== Verifying critical configs ==="
# Architecture + platform
for opt in ARM64 SMP ARCH_QCOM PINCTRL_SC8180X COMMON_CLK_QCOM \
           QCOM_SCM QCOM_SMEM QCOM_RPMH QCOM_RPMHPD REMOTEPROC QCOM_Q6V5_PAS \
           ARM_SMMU SPMI_MSM_PMIC_ARB; do
    verify_on "$opt"
done
# Storage + USB + input
for opt in SCSI_UFS_QCOM USB_DWC3_QCOM USB_HID INPUT_EVDEV TTY VT; do
    verify_on "$opt"
done
# Display + framebuffer (for bootlog)
for opt in DRM_MSM DRM_PANEL_EDP FB FB_CORE FRAMEBUFFER_CONSOLE \
           BACKLIGHT_CLASS_DEVICE PHY_QCOM_EDP; do
    verify_on "$opt"
done
# Boot path
for opt in BINFMT_ELF BLK_DEV_INITRD EFI EFI_STUB EXT4_FS DEVTMPFS DEVTMPFS_MOUNT; do
    verify_on "$opt"
done
# Networking
for opt in NET INET NETDEVICES PACKET UNIX; do
    verify_on "$opt"
done
# Firmware + debug + pstore
for opt in FW_LOADER FW_LOADER_COMPRESS PSTORE PSTORE_RAM DYNAMIC_DEBUG KALLSYMS; do
    verify_on "$opt"
done

# Target-specific
if [ "$TARGET" = "w767" ]; then
    # Phase 1: iteration drivers must be built (=m or =y)
    for opt in ATH10K_SNOC USB_USBNET USB_NET_CDCETHER HID_SAMSUNG; do
        verify_on "$opt"
    done
    if grep -q "CONFIG_MODULES=y" .config; then
        echo "  OK: MODULES=y (Phase 1 needs modules for iteration)"
    else
        echo "  WARNING: MODULES is OFF (Phase 1 should keep =m drivers iterable)"
    fi
fi
if [ "$TARGET" = "w767-initramfs" ]; then
    # Phase 2: USB Ethernet should be =y (no module-load race)
    if grep -q "CONFIG_USB_USBNET=y" .config; then
        echo "  OK: USB_USBNET=y (Phase 2 initramfs)"
    else
        echo "  WARNING: USB_USBNET is not =y — initramfs may lose network before mdev fires"
    fi
fi

# Step 5: build Image + dtbs
echo ""
echo "=== Step 5: building Image ==="
make -j"$JOBS" Image

echo ""
echo "=== Step 5b: building dtbs ==="
make -j"$JOBS" dtbs

if grep -q "CONFIG_MODULES=y" .config; then
    echo ""
    echo "=== Step 5c: building modules ==="
    make -j"$JOBS" modules
fi

# Step 6: install to output directory
echo ""
echo "=== Step 6: installing to $OUT_DIR ==="
mkdir -p "$OUT_DIR" "$OUT_DIR/dtb/qcom"

cp arch/arm64/boot/Image "$OUT_DIR/Image"
echo "  Image:  $(du -h "$OUT_DIR/Image" | cut -f1)"

cp "arch/arm64/boot/dts/qcom/${DTS_BOARD_NAME}.dtb" "$OUT_DIR/dtb/qcom/${DTS_BOARD_NAME}.dtb"
echo "  DTB:    $(du -h "$OUT_DIR/dtb/qcom/${DTS_BOARD_NAME}.dtb" | cut -f1)"

KVER=$(make -s kernelrelease)
echo "  Version: $KVER"
echo "$KVER" > "$OUT_DIR/version"

if grep -q "CONFIG_MODULES=y" .config; then
    echo "  Installing modules..."
    rm -rf "$OUT_DIR/lib"
    make -j"$JOBS" INSTALL_MOD_PATH="$OUT_DIR" modules_install
    rm -f "$OUT_DIR/lib/modules/$KVER/build"
    rm -f "$OUT_DIR/lib/modules/$KVER/source"
    MOD_COUNT=$(find "$OUT_DIR/lib/modules" -name '*.ko' | wc -l)
    echo "  Modules: $MOD_COUNT"
fi

cp .config "$OUT_DIR/config"

echo ""
echo "=== Done ==="
echo "  Image:  $OUT_DIR/Image"
echo "  DTB:    $OUT_DIR/dtb/qcom/${DTS_BOARD_NAME}.dtb"
echo "  Config: $OUT_DIR/config"
