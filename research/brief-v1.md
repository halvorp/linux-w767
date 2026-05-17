# Deep-research brief: Fedora on Samsung Galaxy Book S (SM-W767, SC8180X) — stuck at early boot

## Goal

Boot Fedora Workstation Rawhide (aarch64, kernel 7.0.0-62.fc45) on a **Samsung Galaxy Book S** (model **SM-W767**, Qualcomm **Snapdragon 8cx Gen 1 / SC8180X**) from a USB SSD. The underlying objective is a usable Fedora install; a text console is sufficient initially.

## Hardware

- SoC: Qualcomm SC8180X (Snapdragon 8cx Gen 1, Kryo 495, Adreno 680)
- Wi-Fi/BT: WCN3998
- Modem: Qualcomm X24 LTE
- Audio: Realtek ALC298 + discrete amplifiers
- Storage: UFS 3.0 (128/256 GB)
- Firmware: UEFI, Secure Boot (currently disabled), NOT fully SBSA-compliant
- Internal keyboard: I²C-HID behind a QUP serial engine behind the SMMU
- No externally accessible serial console

## Build host

- Fedora 43 x86_64, kernel 6.19.x
- libguestfs-tools-c + qemu-user-static + dtc installed
- All image edits done via `virt-customize` / `virt-copy-in` (no chroot; libguestfs can't run aarch64 commands on x86_64 hosts)

## What we've built and tried

### Base image

- `Fedora-Workstation-Disk-Rawhide-20260422.n.0.aarch64.raw.xz` (a pre-installed btrfs disk image, not a Live ISO). Partitions:
  - `sda1` 500 MB vfat `EFI` → `/boot/efi`
  - `sda2` 2 GB ext4 `BOOT` → `/boot`
  - `sda3` 10.4 GB btrfs `fedora` (subvols `root`, `home`, `var`) → `/`
- Kernel: `7.0.0-62.fc45.aarch64` (well past the 6.15 milestone where upstream added the x86 `samsung-galaxybook` platform driver — which does **not** apply to ARM Galaxy Book S).
- Boot loader: GRUB2 via shim → `/boot/efi/EFI/fedora/grubaa64.efi` → chain-loaded `/boot/grub2/grub.cfg` → BLS entries under `/boot/loader/entries/`.

### Upstream DTB status (confirmed, not speculated)

- Read `arch/arm64/boot/dts/qcom/Makefile` from Linus `master`. The only `sc8180x-*.dtb` entries in mainline are `sc8180x-lenovo-flex-5g.dtb` and `sc8180x-primus.dtb`. **No Galaxy Book S / SM-W767 DTB in mainline Linux**, as of April 2026.
- `aarch64-laptops/build` repo only has ACPI dumps for Galaxy Book S; no DTS, no kernel patchset.
- Mainline kernel's `samsung-galaxybook` driver (since 6.15) is x86-only, despite AI-generated guides claiming otherwise.

### DTS source used

- `gitlab.com/jenneron/linux`, branch `galaxy-book-s-6.1.2`, commit `561ad081` (dated 2023-01-30).
- File: `arch/arm64/boot/dts/qcom/sc8180x-samsung-w767.dts` (1,118 lines).
- Includes: `sc8180x.dtsi`, `pmc8180.dtsi`, `pmc8180c.dtsi` (+ pm8150 family).
- DTS targets kernel 6.1.2 bindings. Our Fedora kernel is 7.0.0-62 — potential binding drift.
- Compiled on the x86_64 host using `cpp -nostdinc -I inc -I .` + `dtc -I dts -O dtb -@`. Compiled cleanly; warnings only about duplicate unit-addresses on QUP serial engines (normal upstream-kernel warnings).
- Output: `sc8180x-samsung-w767.dtb`, 111 KB, magic `d00dfeed`. Installed to `/boot/dtb-7.0.0-62.fc45.aarch64/qcom/sc8180x-samsung-w767.dtb`.
- BLS entries reference it via `devicetree /dtb-7.0.0-62.fc45.aarch64/qcom/sc8180x-samsung-w767.dtb`.

### Firmware

Extracted from the machine's Windows 11 DriverStore via robocopy. Combined with pmOS's `firmware-samsung-w767` authoritative layout (from `gitlab.com/jenneron/firmware-samsung-galaxy-book-s`):

- `/lib/firmware/qcom/a680_sqe.fw`, `a680_gmu.bin` — Adreno 680 microcode
- `/lib/firmware/qcom/samsung/w767/` — 30 files (ADSP, CDSP, SLPI, MPSS modem, VSS, WDSP, WLAN mdsp, HDCP, modem carrier configs `mcfg_sw.mbn.*`, JSON service maps `adspr.jsn` / `cdspr.jsn` / `modemuw.jsn` / `battmgr.jsn`)
- `/lib/firmware/qca/crbtfw01.tlv`, `crnv01.bin` — WCN3998 Bluetooth
- `/lib/firmware/ath10k/WCN3990/hw1.0/wlanmdsp.mbn` — ath10k path

SELinux relabeled after each injection.

### Kernel cmdline (current, after iteration 3)

```
root=UUID=704fea3c-d88a-40ba-a4c2-5a037380a40a rootflags=subvol=root
iommu.passthrough=0 iommu.strict=0 arm-smmu.disable_bypass=0
pcie_aspm.policy=powersupersave
clk_ignore_unused pd_ignore_unused
arm64.nopauth
modprobe.blacklist=msm
systemd.unit=multi-user.target
systemd.log_level=debug systemd.log_target=kmsg
printk.devkmsg=on log_buf_len=4M
initcall_debug keep_bootcon
```

Notes:
- postmarketOS's reference `samsung-w767` cmdline is `console=null iommu.passthrough=0 iommu.strict=0 pcie_aspm.policy=powersupersave clk_ignore_unused pd_ignore_unused arm64.nopauth efi=noruntime`.
- We dropped `console=null` to preserve console output, dropped `efi=noruntime` in iteration 3 (may have been breaking simpledrm's framebuffer grab under no-runtime), added `arm-smmu.disable_bypass=0` in iteration 3 for I²C/keyboard, added debug/journal knobs.
- `modprobe.blacklist=msm` still set — disables Adreno/display driver so the UEFI framebuffer isn't torn down by a failing probe.

### Image augmentations (for debugging visibility)

- `/var/log/journal/` created → systemd-journald should write persistently.
- Root password set to `galaxy`, `/etc/machine-id` is the sentinel `uninitialized` (first-boot trigger for systemd-firstboot).
- Autologin drop-ins for getty@tty1..tty4 (so if any VT frame-buffer works, we land at a root shell).

### Boot attempts on hardware

All via USB (UEFI → F10 boot menu → USB SSD, Secure Boot disabled).

**Iteration 1** (cmdline: pmOS-style, `efi=noruntime`, **no** `arm-smmu.disable_bypass`, **no** firmware):
- GRUB menu visible, entry selected, **kernel messages scroll**, then **screen goes dark**.
- Keyboard dead (Caps Lock LED doesn't toggle).
- Ctrl-Alt-F1..F9 does nothing.
- USB LED keeps blinking.

**Iteration 2** (added `modprobe.blacklist=msm systemd.unit=multi-user.target`, **injected all firmware**):
- Identical symptoms. Kernel messages, dark, dead keyboard, LED blinking.

**Iteration 3** (debug cmdline above: added `arm-smmu.disable_bypass=0`, dropped `efi=noruntime`, added initcall_debug / persistent journal / autologin, kept everything from iteration 2):
- User booted, waited ~90 s, force-powered-off (10-s hold).
- On bringing the USB back: `/etc/machine-id` still `uninitialized`, `/var/log/journal/` **empty**, no files modified on disk in the last 2 hours. **Nothing was written from the boot attempt.**
- Implies boot hung *before* systemd-journald started — i.e., somewhere between kernel init and PID 1 writing anything to /var.

### What the "LED blinking + dead keyboard + blank screen + nothing written" pattern suggests

- Kernel loaded past GRUB, past initial messages, then something tore down the framebuffer and/or the kernel never reached userspace.
- Candidates: initramfs can't complete (dracut retrying root mount), driver probe loop, SMMU faults, DTS-to-kernel binding mismatch between the 2023 DTS and 2026 kernel 7.0, or systemd blocking on `systemd-firstboot` interactive prompt with no console I/O.

## What we explicitly know is **not** the problem

- DTB isn't missing — confirmed present at the path BLS references.
- Firmware isn't missing — ADSP/CDSP/SLPI/MPSS/GPU/Wi-Fi/BT blobs all in place in pmOS's canonical layout.
- Kernel does start — the user saw kernel messages on screen.
- USB and XHCI work — the image is booted from USB.
- GRUB2 + BLS + `devicetree` key works — we're past the bootloader handoff.

## Open questions we'd like Gemini to chase

1. **Is there a newer, actively-maintained Galaxy Book S / SM-W767 DTS anywhere?** The pmOS device `samsung-w767` depends on `linux-postmarketos-qcom-sc8180x >= 6.6.0`, but that package does not exist in pmaports. Is there a private tree, a linux-sc8180x topic branch on Linaro, or a patch series on the linux-arm-msm list (lore.kernel.org) that adds `sc8180x-samsung-w767.dts` for 6.6+? Any fork (jenneron, jglathe, denysvitali, bamse, anholt, Johan Hovold) with a fresher branch than jenneron's 2023-01?
2. **Is there a known incompatibility between SC8180X DTS bindings from 6.1 and kernel 7.0?** Specifically: QUP GENI (`drivers/soc/qcom/geni-se.c`), UFS Qualcomm host (`drivers/ufs/host/ufs-qcom.c`), I²C-HID, SMMU, or pinctrl MSM/TLMM.
3. **Does anyone boot the SC8180X Galaxy Book S on kernel ≥7.0 successfully**, and with which DTS / kernel patches? The pmOS answers imply yes on their fork kernel, but the package trail dead-ends.
4. **The "dark screen after kernel messages" symptom** specifically on SC8180X laptops: is there a known fix beyond `modprobe.blacklist=msm` + keeping UEFI fb? Any combination of `video=simpledrm:...`, `drm.edid_firmware=...`, or dracut options?
5. **The `uninitialized` machine-id blocking boot**: if `systemd-firstboot` is blocking interactively on tty1 with no keyboard, does setting a baked-in machine-id in the image skip it entirely? Or is `systemd-firstboot` architecturally unable to block multi-user.target?
6. **How to exfiltrate a kernel log from a Galaxy Book S that hangs before userspace** with no serial console, no USB-to-USB debug, no `dmesg` to disk, no panic-pstore (no NVRAM pstore backend on this hw)? Specific recipes involving `ramoops` via a reserved-memory DT node, `efi-pstore` on UEFI vars, or something dracut-level.
7. **Does the postmarketOS samsung-w767 port actually boot?** Any install reports post-2024, any wiki/forum threads, any videos? Or is the pmOS device package effectively abandonware?
8. **Alternative distros that boot SC8180X laptops** beyond our failed Fedora attempt: Debian sid arm64, Ubuntu 25.10 arm64 (we have the ISO and will try), Arch Linux ARM with a custom kernel, NixOS? Boot reports specifically on Galaxy Book S / Lenovo Flex 5G / Lenovo Yoga 5G would all be useful signal.

## Constraints we can't change

- Can only modify the image from an x86_64 Fedora host (no ARM build machine available).
- No external serial/JTAG access on the Galaxy Book S.
- User only has the built-in I²C keyboard (no USB keyboard handy).
- Must ultimately preserve Windows 11 on the internal UFS for firmware re-extraction; target is USB-installed Linux.

## What format is most useful in your answer

- Concrete pointers (URLs, commit hashes, mailing-list archive links, wiki pages).
- Specific kernel cmdline parameters that resolved identical symptoms on similar hardware.
- "It's actually pmOS/Debian that works, not Fedora; here's the install path" — that's a perfectly acceptable answer.
- Null result also useful ("I searched X, Y, Z and found nothing newer than jenneron's 2023 fork") — saves us from repeating it.
