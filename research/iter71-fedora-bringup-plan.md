# iter-71 plan — Fedora 43 Workstation + our kernel: the second attempt

**Status of iter-70 (2026-05-18, end of long day):** Fedora rootfs successfully spliced onto sda2, our 6.6 kernel + DTB on sda1 ESP, whole-disk image dd'd onto the W767 drive. Boot started, then the screen went blank and the system locked up before any tty1/getty output, before journald flushed anything to disk. Drive came back to host; inspection of the mounted Fedora rootfs confirmed the boot reached systemd handoff but stalled before logs were written.

## Diagnostic from post-mortem on the host

- `/var/log/journal/` empty → systemd-journald never persisted any entries → boot stalled inside the first few seconds of PID 1 startup.
- `/etc/selinux/config` says `SELINUX=enforcing`, but we built the rootfs via `mkfs.ext4 -d` which does NOT carry security xattrs across, so every file has zero SELinux labels. enforcing-mode systemd against an unlabelled rootfs cascades into AVC denials and hangs (this is the documented failure mode for "copy a Fedora rootfs without relabel").
- No `/.autorelabel` marker present (would have triggered systemd's autorelabel flow).
- `/sbin/init -> /lib/systemd/systemd` — confirms init handoff target.
- `plymouthd` present — secondary suspect, but SELinux is the most likely primary.

## iter-71 changes

### 1. Disable SELinux for the first boot (priority 1)

Choose one approach:

**Option A — simplest, just disable enforcement**

Append to the kernel cmdline in the loader entry:

```
selinux=0
```

That tells the kernel to never even initialise the SELinux LSM. systemd doesn't try to enforce policy. Fedora boots in a permissive-by-construction mode. Trade-off: no SELinux protection ever for this install, but for a development bring-up that's fine.

**Option B — proper autorelabel**

Touch a file in the rootfs at build time:

```sh
sudo touch /tmp/iter70-root/.autorelabel
```

And on the cmdline:

```
enforcing=0
```

First boot: systemd sees `/.autorelabel`, walks the entire filesystem assigning the correct security context to every file (uses `/etc/selinux/targeted/contexts/files/file_contexts` to map paths → contexts). Takes 10-30 minutes on USB for ~150 k files. Then reboots itself with SELinux fully enforcing.

**Recommendation for iter-71:** Option A first. We get Fedora booting fast, validate that everything else works, then iter-72 can revisit Option B if we want SELinux back.

### 2. Disable plymouth boot splash (priority 2)

Plymouth grabs the framebuffer mid-boot to show the spinning Fedora logo. On our W767's freedreno path, this is a likely culprit for "screen blanks and never recovers" because plymouth assumes specific DRM behaviour. Append to cmdline:

```
plymouth.enable=0
```

Or alternatively `rd.plymouth=0 plymouth.enable=0` for both initramfs (we don't have one) and rootfs phases.

### 3. Force systemd verbose output to kmsg (priority 2)

If iter-71 still hangs, we want to know *where*. Add:

```
systemd.log_level=debug systemd.log_target=kmsg
```

This dumps every systemd transaction to the kernel log buffer, which is visible on tty0 (since we have `console=tty0`). When boot stalls, the last printk shows what systemd was doing.

### 4. Bigger kernel log buffer (just in case)

```
log_buf_len=4M
```

Default is too small for `systemd.log_level=debug`.

### 5. Drop quiet, keep loglevel=6 or bump to 7

We already have `loglevel=6 consoleblank=0`. Maybe bump to 7 for more kernel chatter.

## Proposed iter-71 cmdline (cumulative)

```
root=LABEL=W767ROOT rootfstype=ext4 rw rootflags=relatime,noatime
console=tty0 loglevel=7 consoleblank=0 log_buf_len=4M
earlycon=efifb keep_bootcon
net.ifnames=0
iommu.passthrough=0 iommu.strict=0
pcie_aspm.policy=powersupersave
clk_ignore_unused pd_ignore_unused
arm64.nopauth efi=noruntime
selinux=0 plymouth.enable=0
systemd.log_level=debug systemd.log_target=kmsg
```

## Build steps (re-use most of iter-70's flow)

1. EROFS FUSE mount still set up at `/tmp/erofs-fuse` (run `sudo erofsfuse -o allow_other <iso>/LiveOS/squashfs.img /tmp/erofs-fuse` to remount if it died).
2. Recycle `/home/peter/Documents/GalaxyBookS_Linux/iter70-stage/w767-fedora-full.img`. Just need to swap the loader.conf cmdline. Loopback mount the image's p1, edit `loader/entries/w767-initramfs.conf`, sync, detach.
3. dd to /dev/sda again.

**This avoids re-extracting the 7 GB rootfs.** ~5 min total work, then test boot.

## What to expect on iter-71 boot

- Same systemd-boot single-entry auto-pick (no menu) — that's NORMAL.
- Kernel boots verbosely (loglevel=7 means lots of probe messages).
- systemd starts, logs debug to kmsg → flooded screen with `[INFO]`-style lines.
- Plymouth disabled → no boot splash, just raw text.
- SELinux disabled → no LSM blockages.
- First-boot likely hits `systemd-firstboot.service` asking for locale/timezone/hostname/root pw/user. Need to TYPE answers on the keyboard.
- After firstboot, target is graphical.target → GDM starts → assuming msm/freedreno cooperates, Fedora 43 login screen appears.

## Plan B if iter-71 still locks up

Add `systemd.unit=emergency.target` to cmdline. Boots straight to a shell, no service start. We can inspect the rootfs from inside, then `systemctl default` to try the normal path step-by-step.

## Plan C if Plan B also locks up

Add `init=/bin/sh` instead of systemd. Then we have shell on tty0 directly with no init system. Confirms the kernel + DTB + USB + ext4 + Fedora rootfs all work; isolates the problem to systemd.

## Plan Z (the "give up gracefully")

If iter-71/B/C all fail to give us a working desktop, fall back to iter-69 Alpine (which we have working) and revisit Fedora later. The Alpine path adds GNOME via `apk add` once we configure that, no SELinux to worry about.

---

End-of-day state: drive is here on the host, sda1 = our ESP, sda2 = Fedora rootfs (unmodified from iter-70). Just need a cmdline swap for iter-71.
