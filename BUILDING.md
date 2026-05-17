# Building the W767 boot image

Operational checklist to go from a clean checkout of this repo to a bootable
USB stick for the Samsung Galaxy Book S (SM-W767). This is the condensed
action list distilled from `research/2026-05-17-claude-pre-boot-audit.md` —
read that document for the reasoning behind each step and the full list of
findings.

> **Audience:** the Linux-side build host (this repo's intended target). The
> Windows-side recon work that produced the chip-identification and DSDT
> evidence is captured under `research/` and `windows-extracts/`.

## Prerequisites

Build host: Linux ARM64 (native, ideal) or x86_64 with an aarch64
cross-compiler.

```
# 1. Linux kernel source v7.0 cloned as a SIBLING of this repo
cd ..
git clone --depth 1 --branch v7.0 \
    https://git.kernel.org/pub/scm/linux/kernel/git/torvalds/linux.git linux

# 2. Cross compiler — Fedora/RHEL family
sudo dnf install -y gcc-aarch64-linux-gnu binutils-aarch64-linux-gnu \
                    device-tree-compiler cpio gzip cargo rust \
                    sgdisk dosfstools systemd-boot-unsigned

# (On native aarch64, skip the cross-compiler and pass --cross '' to build-kernel.sh)

# 3. systemd-boot binary at the path build-usb-image.sh expects
mkdir -p /tmp/sd-boot-aa64
rpm2cpio /var/cache/dnf/.../systemd-boot-unsigned-*.aarch64.rpm | \
    ( cd /tmp/sd-boot-aa64 && cpio -idmv )
# Sanity: /tmp/sd-boot-aa64/usr/lib/systemd/boot/efi/systemd-bootaa64.efi
```

Plus an SSH pubkey at `~/.ssh/id_ed25519.pub` (embedded into the initramfs
distro's `/root/.ssh/authorized_keys`) — or pass `--ssh-pubkey <path>` to
`build-initramfs.sh`.

## Firmware staging

The DTS references firmware at `qcom/samsung/w767/<file>.mbn`. The repo
does **not** ship redistributable Samsung/Qualcomm/Cirrus blobs; you must
stage them at `firmware-stage/lib/firmware/qcom/samsung/w767/` before
`build-initramfs.sh` runs (otherwise it aborts at the "firmware staging
missing" check).

**Minimum set required for boot:**

| File | Used by |
|------|---------|
| `qcadsp8180.mbn` | ADSP remoteproc |
| `qccdsp8180.mbn` | CDSP remoteproc |
| `qcdxkmsuc8180.mbn` | GPU zap-shader (loaded by msm/adreno) |
| `qcmpss8180_XEF.mbn` | Modem PSS (also hosts WiFi MAC) |
| `wlanmdsp.mbn` | WiFi firmware payload on MPSS |

**Recommended additions (non-fatal if missing):**

| File | Used by |
|------|---------|
| `qcslpi8180.mbn` | Sensor DSP |
| `qcvss8180.mbn` | Venus video codec |
| `qcwdsp8180.mbn` | WCD audio sub-DSP |
| `*.acdb` (Speaker, Headset, Handset, General, Bluetooth) | Audio calibration |
| `*.jsn` (adspr, adspua, battmgr, cdspr, modemuw) | pd-mapper descriptors |
| `storsec.mbn` | UFS secure storage |
| `dxhdcp2.mbn`, `hdcp{1,2p2,srm}.mbn`, `pr_3_wp.mbn` | HDCP / DRM trustlets |

**GPU firmware** (kernel falls back gracefully if missing):

| File | Path | Note |
|------|------|------|
| `a680_gmu.bin` | `qcom/` | Adreno 680 GMU — falls back to `a640_gmu.bin` |
| `a680_sqe.fw` | `qcom/` | Adreno 680 SQE — falls back to `a630_sqe.fw` |

**Bluetooth firmware** (for once BT module loads):

| File | Path |
|------|------|
| `crnv01.bin` | `qca/` |
| `crbtfw01.tlv` | `qca/` |

**Sources, in priority order:**

1. `gitlab.com/jenneron/firmware-samsung-galaxy-book-s` — community
   collection, canonical for this device.
2. Extract from a Windows install under
   `C:\Windows\System32\DriverStore\FileRepository\`. The
   2026-05-17 recon session captured the SHA256-verified set from this
   exact W767 — see `research/2026-05-17-claude-recon-emuec-chip-id.md`
   for the methodology and the recon-side `lib-firmware/qcom/sc8180x/`
   directory for the actual bytes (note: that staging used the
   `sc8180x/` path; you need to copy the same files to `samsung/w767/`).
3. Upstream `linux-firmware` for GPU (`qcom/a6*_*`) and Bluetooth
   common-case blobs.

## Build sequence

```bash
# 1. Build the Rust userspace (one-time per checkout)
( cd w767-os/rust && cargo build --release --target aarch64-unknown-linux-musl )

# 2. Fetch static busybox + dropbear from Alpine apks (one-time)
./w767-os/initramfs/build-initramfs.sh --fetch-userspace

# 3. Build the Phase-2 kernel + DTB + modules + initramfs
./w767-os/kernel/build-kernel.sh --target w767-initramfs
./w767-os/initramfs/build-initramfs.sh

# 4. Produce the bootable USB image
./deploy/build-usb-image.sh --image /tmp/w767-usb.img

# 5. Flash to a USB stick (DESTRUCTIVE — verify the device first)
sudo dd if=/tmp/w767-usb.img of=/dev/sdX bs=4M status=progress conv=fsync
```

For a native aarch64 build host, prefix step 3 with `CROSS_COMPILE= ` or
pass `--cross ''` to `build-kernel.sh`.

## Boot procedure on the W767

1. Insert the USB.
2. Hold **Volume-Down + Power** at power-on to enter the Samsung UEFI
   boot menu.
3. Select the USB stick.
4. systemd-boot menu appears with a 3-second timeout, default
   `w767-initramfs`.

## What to expect on first boot

| Stage | What should happen | Time |
|-------|-------------------|------|
| Samsung splash | Firmware logo from Samsung UEFI | ~2s |
| systemd-boot menu | 3s timeout, kernel selected | ~3s |
| Kernel takes over | GOP framebuffer stays visible via simpledrm | immediately |
| Early init prints | Visible on screen (`PRINTK_TIME=y`, `console=tty0`) | 0-5s |
| DRM_MSM binds | `eDP-1 connected`, `dp_aux_backlight` registered | ~5-10s |
| busybox shell | Login prompt on the panel | ~15-30s |

## Common first-boot failures

| Symptom | Likely cause | Where to look |
|---------|--------------|---------------|
| Black screen the instant the kernel takes over | simpledrm/efifb handover failed | Verify `CONFIG_SYSFB=y`, `CONFIG_DRM_SIMPLEDRM=y`, `CONFIG_FB_EFI=y` made it into the built `.config` (grep `kernel/out/w767-initramfs/config`) |
| Kernel hangs before login | A SC8180X quirk got dropped from cmdline | `cat /proc/cmdline` once you have any console; should include `clk_ignore_unused pd_ignore_unused arm64.nopauth efi=noruntime iommu.passthrough=0 iommu.strict=0` |
| Touchpad doesn't enumerate at i2c 0x49 | `vreg_l4c_3v3` is commented out (DTS line 289-295) — touch IC may need 3.3V analog supply | Try uncommenting `vreg_l4c_3v3` and the touchpad's `vio-supply` reference; see DTS line ~292 |
| WiFi doesn't appear | wlanmdsp.mbn not at the right path | `find /lib/firmware -name 'wlanmdsp.mbn'` — must be at `qcom/samsung/w767/wlanmdsp.mbn` |
| Kernel panics on ADSP/CDSP/MPSS load | Firmware path mismatch or missing blob | `dmesg | grep -iE 'q6v5\|pas\|remoteproc'` |
| eDP comes up but display is full-brightness only | DPCD-AUX backlight not registered | `ls /sys/class/backlight/` — expect `dp_aux_backlight`; if absent, panel-edp didn't probe the panel — check `aux-bus` block in DTS |

## Capturing crash data

`CONFIG_PSTORE_RAM=y` is enabled but the DTS has no reserved-memory
region named `ramoops` — so pstore-ram won't bind. Post-boot, ramoops at
`/sys/fs/pstore/dmesg-ramoops-0` will be empty. For early-hang diagnosis,
add a `ramoops` reserved-memory node to the DTS before the build (and
note that the chosen RAM region must not overlap any other reservation).

`sysrq` is enabled (`CONFIG_MAGIC_SYSRQ=y`) — `sysrq-c` triggers a
hard crash + dump if you have keyboard input working.

## What's known to work, what's not

See `docs/00-hardware-combined.md` section 3 for the canonical
subsystem-by-subsystem status table. Highlights:

| Subsystem | Status as of 2026-05-17 |
|-----------|-------------------------|
| Display + GPU + UFS + USB host | ✅ proven in iter-17, no DTS regression since |
| Touchpad (i2c-hid) | 🟡 DSDT-canonical values now correct (`reg=0x49`, `hid-descr-addr=0x00ab`); pending regulator-on-first-boot verification |
| WiFi (ath10k_snoc, WCN3998) | 🟡 DTS + config correct; firmware path needs to land at `qcom/samsung/w767/wlanmdsp.mbn` |
| Modem (MPSS) | 🟡 firmware ready; userspace daemons (rmtfs, pd-mapper, tqftpserv, qrtr) are separate work |
| Bluetooth (qcom,wcn3998-bt on uart13) | 🟡 DTS correct; needs `qca/crnv01.bin` + `qca/crbtfw01.tlv` |
| Internal keyboard, Audio amps, Cameras, Suspend, Fingerprint | ❌ requires custom driver work — see `docs/02-samsung-platform.md` |

## Reference

- Full pre-boot audit (the source for this checklist):
  `research/2026-05-17-claude-pre-boot-audit.md`
- Chip identification (EmuEC I²C slaves):
  `research/2026-05-17-claude-recon-emuec-chip-id.md`
- Canonical hardware reference:
  `docs/00-hardware-combined.md`
- iter-17 working-state snapshot (ground truth for display+GPU):
  `docs/iter-17-boot-snapshot.txt`
