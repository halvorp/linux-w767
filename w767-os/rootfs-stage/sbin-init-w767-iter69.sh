#!/bin/sh
# w767 iter-69 /sbin/init for the Alpine rootfs first boot.
#
# iter-68 used getty + login + an empty-password root account. That broke
# two ways:
#   1. busybox login refuses lastchange=0 / empty-password combos
#      ("Password must be changed").
#   2. When login kept failing, init's `exec getty` line meant init itself
#      exited, the kernel panic'd with "Attempted to kill init".
#
# iter-69 fix: skip getty + login entirely for first boot. Mount pseudo-fs,
# open /dev/tty0 as a controlling terminal via setsid, then respawn a shell
# in an infinite loop so init never exits.

mount -t proc     proc     /proc    2>/dev/null
mount -t sysfs    sysfs    /sys     2>/dev/null
mount -t devtmpfs devtmpfs /dev     2>/dev/null
mount -t tmpfs    tmpfs    /run     2>/dev/null
mount -t tmpfs    tmpfs    /tmp     2>/dev/null
mkdir -p /dev/pts && mount -t devpts devpts /dev/pts 2>/dev/null
mount -t pstore   none     /sys/fs/pstore 2>/dev/null

hostname w767
export PATH=/sbin:/usr/sbin:/bin:/usr/bin
export HOME=/root
export TERM=linux

# Make /dev/tty0 our controlling terminal so job control + signals work.
# setsid -c (create new session and make the program the controlling
# terminal owner) is the standard idiom for "shell on a virtual console".
# We loop forever so init never exits -- even Ctrl-D or shell crash just
# restarts the shell.

while true; do
    echo
    echo "================================================"
    echo "  W767 Alpine rootfs  (iter-69 autoshell)"
    echo "  Kernel: $(uname -r)   Arch: $(uname -m)"
    echo "  / = $(findmnt -no SOURCE /) (W767ROOT)"
    echo
    echo "  wlan0 status:"
    ip -br link show wlan0 2>/dev/null || echo "    (wlan0 not visible -- check ip link)"
    echo
    echo "  Useful first commands:"
    echo "    cat /etc/alpine-release"
    echo "    apk update"
    echo "    apk add wpa_supplicant wifi-tools dhcpcd"
    echo "    apk add mesa-dri-gallium kmscube"
    echo "    apk add sway foot                   # Wayland desktop"
    echo "================================================"
    echo

    setsid -c /bin/sh -l < /dev/tty0 > /dev/tty0 2>&1

    # If the shell ever exits, log + restart. Real systems would respawn
    # via inittab; we have no inittab.
    echo "[init.w767] shell exited, respawning in 2s ..." > /dev/kmsg 2>/dev/null
    sleep 2
done
