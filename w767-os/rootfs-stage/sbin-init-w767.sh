#!/bin/sh
# w767 iter-68 minimal /sbin/init for the Alpine rootfs first boot.
# Plain busybox sh — Alpine minirootfs has busybox + apk, no openrc yet.
# Stage 1: mount pseudo-filesystems
# Stage 2: hand control to getty on tty0 so the screen shows a login prompt.

mount -t proc     proc     /proc    2>/dev/null
mount -t sysfs    sysfs    /sys     2>/dev/null
mount -t devtmpfs devtmpfs /dev     2>/dev/null
mount -t tmpfs    tmpfs    /run     2>/dev/null
mount -t tmpfs    tmpfs    /tmp     2>/dev/null
mkdir -p /dev/pts && mount -t devpts devpts /dev/pts 2>/dev/null
mount -t pstore   none     /sys/fs/pstore 2>/dev/null

hostname w767

# Symlink /etc/resolv.conf to /run for live updates (NetworkManager style later)
[ -L /etc/resolv.conf ] || { rm -f /etc/resolv.conf; ln -s /run/resolv.conf /etc/resolv.conf 2>/dev/null; }
# Keep the static 1.1.1.1 fallback if /run/resolv.conf doesn't exist yet
echo "nameserver 1.1.1.1" > /run/resolv.conf 2>/dev/null

echo
echo "================================================"
echo "  W767 Alpine rootfs — iter-68 first boot"
echo "  /dev/sda2 mounted as /"
echo "  /lib/modules/6.6.0 staged from initramfs"
echo "  /lib/firmware staged (98 MB W767 blobs)"
echo
echo "  wlan0 is still UP from the initramfs (kernel netdev"
echo "  persists across switch_root). wpa_supplicant from the"
echo "  initramfs is dead though, so the association will drop"
echo "  after a few minutes. To get wifi back from Alpine:"
echo "    apk update              # needs working dns first"
echo "    apk add wpa_supplicant wifi-tools dhcpcd"
echo
echo "  Next iters (69+):"
echo "    apk add openrc mesa-dri-gallium kmscube"
echo "    apk add sway foot                  # tiling Wayland"
echo "================================================"
echo

# Greet on tty0 then hand the controlling terminal to getty so login works.
# -L = local, no carrier detect. 0 = autodetect baud (no-op for KMS console).
exec /sbin/getty -L tty0 0 vt100
