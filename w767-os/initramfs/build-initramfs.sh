#!/bin/bash
# Build the Phase-2 initramfs for w767-os.
#
# The initramfs is cpio+gzip and contains:
#   - /init               — shell stub that mounts /proc /sys /dev and exec's w767_init
#   - /opt/w767/bin/      — w767_init, w767_ctl, w767_ctl_cli, w767_netd_lite
#                           + busybox and dropbear (provisioned separately)
#   - /etc/...            — passwd/group/shadow + dropbear config
#   - /lib/firmware/...   — copied from firmware-stage/
#   - /lib/modules/...    — copied from kernel/out/w767-initramfs/lib/modules/
#
# Usage:
#   ./build-initramfs.sh                # build w767-initramfs.img
#   ./build-initramfs.sh --fetch-busybox  # download pre-built static aarch64 busybox
#   ./build-initramfs.sh --kernel-out ../kernel/out/w767-initramfs
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
PROJECT_DIR="$(dirname "$SCRIPT_DIR")"
REPO_ROOT="$(dirname "$PROJECT_DIR")"

LAYOUT="$SCRIPT_DIR/layout"
OUT="$PROJECT_DIR/kernel/out/w767-initramfs.img"
KERNEL_OUT="$PROJECT_DIR/kernel/out/w767-initramfs"
RUST_BIN="$PROJECT_DIR/rust/target/aarch64-unknown-linux-musl/release"
FIRMWARE_SRC="$REPO_ROOT/firmware-stage/lib/firmware"

BUSYBOX_BIN="$LAYOUT/opt/w767/bin/busybox"
DROPBEAR_BIN="$LAYOUT/opt/w767/bin/dropbear"
DROPBEARKEY_BIN="$LAYOUT/opt/w767/bin/dropbearkey"
SSH_PUBKEY="${SSH_PUBKEY:-$HOME/.ssh/id_ed25519.pub}"

FETCH_USERSPACE=false
while [[ $# -gt 0 ]]; do
    case "$1" in
        --fetch-userspace|--fetch-busybox)  FETCH_USERSPACE=true; shift ;;
        --kernel-out)     KERNEL_OUT="$2"; shift 2 ;;
        --out)            OUT="$2"; shift 2 ;;
        --ssh-pubkey)     SSH_PUBKEY="$2"; shift 2 ;;
        *)                echo "Unknown option: $1"; exit 1 ;;
    esac
done

echo "=== w767-os initramfs builder ==="
echo "  Layout:    $LAYOUT"
echo "  Output:    $OUT"
echo "  Rust bin:  $RUST_BIN"
echo "  Kernel:    $KERNEL_OUT"
echo "  Firmware:  $FIRMWARE_SRC"
echo ""

# -------------------------------------------------------------------------
# Provision userspace binaries (busybox + dropbear) from Alpine Linux apks.
#
# Alpine publishes musl-static aarch64 packages at a stable CDN URL. We fetch
# the latest 'main' repo apks, which are gzip'd tarballs with a known layout:
#   busybox-static-*.apk  -> ./bin/busybox.static
#   dropbear-*.apk        -> ./usr/bin/dropbear, ./usr/bin/dropbearkey
#
# The fetch happens once per developer machine; the extracted binaries are
# .gitignore'd under initramfs/layout/opt/w767/bin/.
# -------------------------------------------------------------------------
if [ "$FETCH_USERSPACE" = "true" ]; then
    echo "=== Fetching userspace binaries (Alpine aarch64 apks) ==="
    mkdir -p "$(dirname "$BUSYBOX_BIN")"
    TMP="$(mktemp -d)"
    trap 'rm -rf "$TMP"' EXIT
    ALPINE_VER="v3.21"
    ALPINE_BASE="https://dl-cdn.alpinelinux.org/alpine/${ALPINE_VER}/main/aarch64"
    INDEX_URL="${ALPINE_BASE}/APKINDEX.tar.gz"

    # Resolve the current filenames from APKINDEX (package names alone are
    # untagged; apk filenames include the version).
    echo "  indexing ${ALPINE_BASE}"
    curl -fsSL "$INDEX_URL" -o "$TMP/APKINDEX.tar.gz"
    tar -xzf "$TMP/APKINDEX.tar.gz" -C "$TMP" APKINDEX
    # APKINDEX is a sequence of newline-delimited records with single-char prefixes:
    # 'P:' = package name, 'V:' = version. Joined by blank lines.
    # Resolve versions for busybox-static, dropbear, and musl (the dropbear
    # binary is dynamically linked against musl libc, so we ship the loader).
    for pkg in busybox-static dropbear musl; do
        eval "${pkg//-/_}_VER=\$(awk -v RS= '\$0 ~ /(^|\\n)P:${pkg}(\$|\\n)/ {
            for (i=1;i<=NF;i++) if (\$i ~ /^V:/) { sub(\"V:\",\"\",\$i); print \$i; exit }
        }' \"$TMP/APKINDEX\")"
    done
    [ -n "${busybox_static_VER:-}" ] || { echo "ERROR: busybox-static not in APKINDEX"; exit 1; }
    [ -n "${dropbear_VER:-}" ]       || { echo "ERROR: dropbear not in APKINDEX";       exit 1; }
    [ -n "${musl_VER:-}" ]           || { echo "ERROR: musl not in APKINDEX";           exit 1; }
    echo "  busybox-static-${busybox_static_VER}"
    echo "  dropbear-${dropbear_VER}"
    echo "  musl-${musl_VER}"

    curl -fsSL "${ALPINE_BASE}/busybox-static-${busybox_static_VER}.apk" -o "$TMP/bbx.apk"
    curl -fsSL "${ALPINE_BASE}/dropbear-${dropbear_VER}.apk"             -o "$TMP/drb.apk"
    curl -fsSL "${ALPINE_BASE}/musl-${musl_VER}.apk"                     -o "$TMP/musl.apk"

    mkdir -p "$TMP/bbx-root" "$TMP/drb-root" "$TMP/musl-root"
    tar -xzf "$TMP/bbx.apk"  -C "$TMP/bbx-root"  2>/dev/null || true
    tar -xzf "$TMP/drb.apk"  -C "$TMP/drb-root"  2>/dev/null || true
    tar -xzf "$TMP/musl.apk" -C "$TMP/musl-root" 2>/dev/null || true

    install -m 0755 "$TMP/bbx-root/bin/busybox.static"  "$BUSYBOX_BIN"
    install -m 0755 "$TMP/drb-root/usr/sbin/dropbear"   "$DROPBEAR_BIN"
    install -m 0755 "$TMP/drb-root/usr/bin/dropbearkey" "$DROPBEARKEY_BIN"

    # Install the musl loader so the dynamic dropbear binary runs.
    # Alpine symlinks /usr/lib/libc.musl-aarch64.so.1 → /lib/ld-musl-aarch64.so.1.
    mkdir -p "$LAYOUT/lib" "$LAYOUT/usr/lib"
    if [ -f "$TMP/musl-root/lib/ld-musl-aarch64.so.1" ]; then
        cp -a "$TMP/musl-root/lib/ld-musl-aarch64.so.1" "$LAYOUT/lib/"
    else
        # Some Alpine releases ship the loader only as a symlink. Dereference it.
        real=$(readlink -f "$TMP/musl-root/lib/ld-musl-aarch64.so.1" 2>/dev/null || true)
        if [ -n "$real" ] && [ -f "$real" ]; then
            cp -aL "$real" "$LAYOUT/lib/ld-musl-aarch64.so.1"
        fi
    fi
    # libc.musl-aarch64.so.1 is typically a symlink to /lib/ld-musl-aarch64.so.1.
    (cd "$LAYOUT/usr/lib" && ln -sf ../../lib/ld-musl-aarch64.so.1 libc.musl-aarch64.so.1)

    file "$BUSYBOX_BIN"  | head -1
    file "$DROPBEAR_BIN" | head -1
    file "$LAYOUT/lib/ld-musl-aarch64.so.1" | head -1
    trap - EXIT
    rm -rf "$TMP"
fi

# -------------------------------------------------------------------------
# Sanity checks
# -------------------------------------------------------------------------
for b in w767_init w767_ctl w767_ctl_cli w767_netd_lite; do
    if [ ! -x "$RUST_BIN/$b" ]; then
        echo "ERROR: missing $RUST_BIN/$b — run: cargo build --release" >&2
        exit 1
    fi
done
if [ ! -x "$BUSYBOX_BIN" ]; then
    echo "ERROR: busybox not at $BUSYBOX_BIN" >&2
    echo "  hint: ./build-initramfs.sh --fetch-busybox" >&2
    echo "  or drop a static aarch64 busybox at $BUSYBOX_BIN" >&2
    exit 1
fi
if [ ! -x "$DROPBEAR_BIN" ]; then
    echo "ERROR: dropbear not at $DROPBEAR_BIN" >&2
    echo "  hint: extract Alpine 'dropbear' apk for aarch64 and place dropbear + dropbearkey here"
    exit 1
fi
if [ ! -d "$KERNEL_OUT/lib/modules" ]; then
    echo "ERROR: kernel modules missing at $KERNEL_OUT/lib/modules"
    echo "  hint: ./kernel/build-kernel.sh --target w767-initramfs"
    exit 1
fi
if [ ! -d "$FIRMWARE_SRC" ]; then
    echo "ERROR: firmware staging missing at $FIRMWARE_SRC"
    exit 1
fi
if [ ! -r "$SSH_PUBKEY" ]; then
    echo "ERROR: SSH pubkey not readable: $SSH_PUBKEY"
    echo "  hint: export SSH_PUBKEY=/path/to/id_ed25519.pub or --ssh-pubkey <path>"
    exit 1
fi

# -------------------------------------------------------------------------
# Prepare layout
# -------------------------------------------------------------------------
echo "=== Preparing layout ==="
install -m 0755 "$RUST_BIN/w767_init"      "$LAYOUT/opt/w767/bin/w767_init"
install -m 0755 "$RUST_BIN/w767_ctl"       "$LAYOUT/opt/w767/bin/w767_ctl"
install -m 0755 "$RUST_BIN/w767_ctl_cli"   "$LAYOUT/opt/w767/bin/w767_ctl_cli"
install -m 0755 "$RUST_BIN/w767_netd_lite" "$LAYOUT/opt/w767/bin/w767_netd_lite"
echo "  Rust binaries installed"

# /init stub
cat > "$LAYOUT/init" <<'SH'
#!/bin/sh
# w767-os initramfs /init
# First program after the kernel. Sets up /proc /sys /dev, busybox shortcuts,
# then exec's the Rust PID 1. If that fails, drops into a rescue shell so the
# user isn't stranded.

/opt/w767/bin/busybox --install -s /opt/w767/bin

/opt/w767/bin/busybox mount -t proc     proc      /proc
/opt/w767/bin/busybox mount -t sysfs    sys       /sys
/opt/w767/bin/busybox mount -t devtmpfs devtmpfs  /dev 2>/dev/null
/opt/w767/bin/busybox mkdir -p /dev/pts /run /tmp /var/log
/opt/w767/bin/busybox mount -t devpts   devpts    /dev/pts 2>/dev/null
/opt/w767/bin/busybox mount -t tmpfs    tmpfs     /run
/opt/w767/bin/busybox mount -t tmpfs    tmpfs     /tmp

export PATH=/opt/w767/bin:/bin:/sbin:/usr/bin:/usr/sbin
echo "w767-os /init handing off to w767_init" > /dev/kmsg || true

exec /opt/w767/bin/w767_init

# If w767_init falls through (it shouldn't — PID 1 must never return),
# drop to a rescue shell so we can diagnose.
echo "w767_init returned — rescue shell" > /dev/kmsg || true
exec /opt/w767/bin/busybox sh
SH
chmod +x "$LAYOUT/init"

# /etc/passwd + /etc/group + /etc/shadow
mkdir -p "$LAYOUT/etc"
cat > "$LAYOUT/etc/passwd" <<'EOF'
root:x:0:0:root:/root:/opt/w767/bin/busybox
nobody:x:65534:65534:Nobody:/:/opt/w767/bin/busybox
EOF
cat > "$LAYOUT/etc/group" <<'EOF'
root:x:0:
nogroup:x:65534:
EOF
# Empty password hash = password login disabled (dropbear runs with -s anyway).
cat > "$LAYOUT/etc/shadow" <<'EOF'
root:*:19000:0:99999:7:::
nobody:!::::::::
EOF
chmod 0600 "$LAYOUT/etc/shadow"

# /etc/hostname + /etc/resolv.conf (DHCP may overwrite later)
echo "w767" > "$LAYOUT/etc/hostname"
cat > "$LAYOUT/etc/resolv.conf" <<'EOF'
# Overwritten by w767_netd_lite after DHCP. Fallback resolvers below.
nameserver 1.1.1.1
nameserver 8.8.8.8
EOF

# /root
mkdir -p "$LAYOUT/root"
cat > "$LAYOUT/root/.profile" <<'EOF'
export PATH=/opt/w767/bin:/bin:/sbin:/usr/bin:/usr/sbin
alias ll='busybox ls -la --color=auto'
echo "w767-os ready. Use w767_ctl_cli for device control."
EOF

# Dropbear config: authorized_keys + host key placeholder (generated at boot)
mkdir -p "$LAYOUT/etc/dropbear"
install -m 0600 "$SSH_PUBKEY" "$LAYOUT/root/.ssh/authorized_keys" 2>/dev/null || {
    mkdir -p "$LAYOUT/root/.ssh"
    install -m 0600 "$SSH_PUBKEY" "$LAYOUT/root/.ssh/authorized_keys"
}
# Dropbear -R auto-generates host keys at /etc/dropbear/dropbear_{rsa,ed25519}_host_key
# on first run. We leave the directory empty and mode 0700 so keys land safely.
chmod 0700 "$LAYOUT/etc/dropbear"

# /etc/w767/ctl.token (random 32-byte hex)
mkdir -p "$LAYOUT/etc/w767"
if [ ! -s "$LAYOUT/etc/w767/ctl.token" ]; then
    # 32 bytes of entropy → 64 hex chars. Use od (coreutils) so we don't
    # depend on vim-common's xxd being installed on the build host.
    head -c 32 /dev/urandom | od -An -v -tx1 | tr -d ' \n' > "$LAYOUT/etc/w767/ctl.token"
    echo "" >> "$LAYOUT/etc/w767/ctl.token"
fi
chmod 0600 "$LAYOUT/etc/w767/ctl.token"

# /etc/w767/services.toml (placeholder — w767_init's DAG is compile-time)
cat > "$LAYOUT/etc/w767/services.toml" <<'EOF'
# Informational. w767_init's DAG is compile-time constant; edit main.rs to change.
daemons = ["w767_netd", "dropbear", "w767_ctl"]
EOF

# /lib/firmware — copy (hardlink where possible to save memory)
echo "=== Copying firmware ==="
rm -rf "$LAYOUT/lib/firmware"
mkdir -p "$LAYOUT/lib/firmware"
cp -a "$FIRMWARE_SRC/." "$LAYOUT/lib/firmware/"
FW_COUNT=$(find "$LAYOUT/lib/firmware" -type f | wc -l)
echo "  firmware: $FW_COUNT files"

# /lib/modules — copy depmod'd tree
echo "=== Copying kernel modules ==="
rm -rf "$LAYOUT/lib/modules"
mkdir -p "$LAYOUT/lib/modules"
cp -a "$KERNEL_OUT/lib/modules/." "$LAYOUT/lib/modules/"
MOD_COUNT=$(find "$LAYOUT/lib/modules" -name '*.ko*' | wc -l)
echo "  modules:  $MOD_COUNT .ko files"

# -------------------------------------------------------------------------
# Build cpio.gz
# -------------------------------------------------------------------------
echo "=== Building cpio ==="
mkdir -p "$(dirname "$OUT")"
(
    cd "$LAYOUT"
    # List every file (including empty dirs via `-type d`), newline-separated,
    # pass to cpio's newc format, then gzip. Owner/perm is taken from the
    # layout; running the script as root is NOT required (cpio treats
    # filesystem owners as canonical which is fine for files we own).
    find . -print | cpio --quiet -o -H newc -R 0:0 | gzip -9 > "$OUT"
)
SIZE=$(du -h "$OUT" | cut -f1)
echo ""
echo "=== Done ==="
echo "  $OUT  ($SIZE)"
