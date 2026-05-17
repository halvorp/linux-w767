# Deep-research brief v2: Fedora on Samsung Galaxy Book S (SM-W767, SC8180X)

**Context**: Updated follow-up to v1. Since then we have proven kernel 7.0 boots userspace on this hardware with the mainline Flex 5G DTB; the remaining blockers are narrower and technical.

## State of play — what now works

With `sc8180x-lenovo-flex-5g.dtb` (mainline-shipped in Fedora's kernel 7.0.0-62.fc45.aarch64), plus our extracted firmware placed under `/usr/lib/firmware/qcom/sc8180x/LENOVO/82AK/` and `/usr/lib/firmware/qcom/samsung/w767/`:

- Kernel boots, reaches `sysinit.target`, `basic.target`, `multi-user.target`.
- systemd-firstboot no longer blocks (pre-initialized `/etc/machine-id` + `systemd.firstboot=0`).
- systemd-logind runs; **single-press power button triggers graceful `poweroff`**.
- `getty@tty1..4` spawn; autologin drop-ins fire; root shell is active on tty1 (confirmed via `pam_unix(login:session): session opened for user root` across all four ttys).
- `simpledrm` probes, `fbcon` takes over the console, `/dev/fb0` and `/dev/dri/card0` both present.
- Internal keyboard works — `Samsung_Electronics_Co.__Ltd_SPACE_v57_2081368E4D50` enumerated on `xhci-hcd.1.auto-usb-0:1:1.x` with three HID interfaces (keyboard / consumer controls / system). **Caps Lock LED toggles on keypress; Fn-Lock toggles; power button triggers shutdown**.
- Touchpad enumerated at `i2c-1` (`1-002c`) — `VEN_STMT&DEV_1234` (STMicroelectronics).
- Bluetooth controller (WCN3990 BT half) probes; `qca/crbtfw21.tlv` and `qca/crnv21.bin` firmware download OK.
- All three remoteprocs announce as "available" and begin powering up: `remoteproc0: modem`, `remoteproc1: cdsp`, `remoteproc2: adsp`. CDSP/ADSP firmware loads succeed once firmware is placed at the LENOVO/82AK path Flex 5G DTS expects.
- With the right cmdline timing (see below), WCN3990 Wi-Fi driver probes: `probe of 18800000.wifi returned 0 after 12711 usecs`.
- UFS / eMMC / internal storage unused so far (booting from USB XHCI), but XHCI enumeration is healthy (both the boot USB and the internal USB hub come up).

## Definitively ruled out as the display problem

- **DTB missing** — we're using Flex 5G's mainline-maintained DTB; it compiles and the kernel parses it.
- **Firmware missing** — blobs are in both `/usr/lib/firmware/qcom/samsung/w767/` and `/usr/lib/firmware/qcom/sc8180x/LENOVO/82AK/` (30 files each, including `qcadsp8180.mbn`, `qccdsp8180.mbn`, `qcslpi8180.mbn`, `qcmpss8180_XEF.mbn`, `wlanmdsp.mbn`, plus `adspr.jsn`/`cdspr.jsn`/`modemuw.jsn` service maps).
- **Adreno 680 GPU firmware missing** — `a680_sqe.fw` and `a680_gmu.bin` are present (from `gitlab.com/jenneron/firmware-samsung-galaxy-book-s`).
- **systemd-firstboot interactive prompt** — machine-id is a real UUID, `systemd.firstboot=0` is on the cmdline, and `/etc/systemd/system/systemd-firstboot.service → /dev/null` (masked).
- **Boot-splash race** — `plymouth.enable=0 rd.plymouth=0` on the cmdline doesn't change the symptom.
- **I²C-HID assumption for keyboard** — turned out the internal keyboard is **USB-HID** (composite device `VID_04E8&PID_A055` on internal XHCI root hub). This was a red herring from v1; `arm-smmu.disable_bypass=0` doesn't actually help the keyboard (it's confirmed working regardless).

## The single remaining symptom

**Screen goes dark a few seconds after the initial kernel messages.** Holds dark through the rest of boot — even though `simpledrm` has loaded, `fbcon` has taken over, `fb0` is registered, and userspace is fully running (autologin lands on tty1 as root, agetty is serving).

Tried blacklisting in various combinations: `modprobe.blacklist=msm,phy_qcom_edp,pwm_bl,panel_edp` (and `rd.driver.blacklist=` for initramfs). No change to the dark-screen symptom. Something is still putting the panel into a sleep state or disabling its backlight/regulator after handoff — but *nothing visible to our log collection* is naming itself the culprit.

## Key discoveries since v1

### 1. Galaxy Book S internal keyboard is USB-HID, not I²C-HID

From Windows PnP dump (`pnp_details.txt`):
- `ACPI\SAMM0901` — Samsung ACPI stub
- `USB\VID_04E8&PID_A055\2081368E4D50` — internal USB composite (HS, SuperSpeed-eligible)
- `HID\VID_04E8&PID_A055&MI_00&COL01..COL04` — keyboard rows
- `HID\VID_04E8&PID_A055&MI_02&COL01..COL04` — consumer/system controls

This composite device sits behind an **internal USB hub** that enumerates on `xhci-hcd.1.auto` (different XHCI host from the external USB-C ports). In the journal: `Samsung Electronics Co._Ltd SPACE v57 2081368E4D50` attached at `platform-xhci-hcd.1.auto-usb-0:1:1.0/1.1/1.2`. So the whole keyboard issue is orthogonal to SMMU/I²C/pinctrl.

### 2. Flex 5G DTS hardcodes Lenovo-specific firmware paths

Its remoteproc nodes specify `firmware-name = "qcom/sc8180x/LENOVO/82AK/qc{cdsp,adsp}8180.mbn"`. Placing identical Samsung-extracted firmware at the LENOVO path works (confirmed by lost `Direct firmware load failed` errors and ADSP/CDSP successfully "powering up").

### 3. Wi-Fi probe is timing-fragile

`ath10k_snoc` probes `18800000.wifi` *only* when there's enough delay between SoC subsystem init and WLAN driver probe — i.e., only when `initcall_debug` is on the cmdline (every initcall prints, slowing boot enough that ADSP's QMI services come up first). Remove `initcall_debug` → identical image otherwise → ath10k_snoc never probes → no `wlan0` device → NetworkManager sees only `lo`.

The ordering dependency isn't expressed in the DT; the `ath10k_snoc` driver apparently assumes QMI is available by the time its probe runs, and fails silently if not. No `-EPROBE_DEFER` retry path.

### 4. `qcom_pmic_glink` logs are worrying but boot survives them

Multiple of these in the journal:
```
qcom_pmic_glink pmic-glink: Failed to create device link (0x180) with supplier usbprim-sbu-mux for /pmic-glink/connector@0
qcom_pmic_glink pmic-glink: Failed to create device link (0x180) with supplier 88e8000.phy for /pmic-glink/connector@0
qcom_pmic_glink pmic-glink: Failed to create device link (0x180) with supplier a600000.usb for /pmic-glink/connector@0
synth uevent: /devices/platform/pmic-glink/pmic_glink.power-supply.0/power_supply/qcom-battmgr-bat: failed to send uevent
```
These *might* be benign but they're exactly the subsystem that manages battery AC/USB power-supply state plus USB-C SBU muxing — any of which could feed back to the display rail.

### 5. The Flex 5G DTS describes a panel that isn't ours

The Flex 5G DTB has a panel node under `displayport-controller@ae9a000/aux-bus/panel`. With `msm` and `panel_edp` blacklisted, this *shouldn't* be probed — but the dependency cycle messages in dmesg (`Fixed dependency cycle(s) with .../aux-bus/panel`) show the kernel still walks through it. If *any* regulator/gpio reference in that subtree is getting asserted-then-dropped, the Samsung panel could be left stranded.

### 6. `/etc/machine-id = "uninitialized"` was the v1 blocker

Confirmed by post-fix boot: with real UUID, systemd-firstboot doesn't block and `multi-user.target` is reached. Userspace has been alive in every iteration since.

## Open questions where Gemini can help most

1. **What, specifically, powers down the internal eDP panel** after `simpledrm` + `fbcon` have taken over the UEFI framebuffer? With `msm`, `phy_qcom_edp`, `pwm_bl`, `panel_edp` all blacklisted, the screen still goes dark a few seconds in. Is there a documented pmic-glink / qcom-battmgr regulator shutdown behavior on SC8180X that's not part of the "display subsystem" proper? Is there a known cmdline like `regulator.ignore_unused`, `video=simpledrm:persist`, `nomodeset`, `fbcon=keep`, or similar that's reported working on SC8180X laptops? Also check the specific "`sysfb_apply_efi_quirks` regression in 6.1.23+" claim — is that fully real, and is there a kernel 7.x patch that reverts or fixes it?
2. **Has anyone produced a hybrid DTS** that uses the mainline-maintained Flex 5G base (for kernel 7.x compatibility) but overlays Samsung-W767-specific differences (different panel, different pmic-glink connector layout, different keyboard ACPI stub)? Ideally a patch series on linux-arm-msm@vger, a GitHub fork, or a pmaports MR. We've checked `jenneron/linux@galaxy-book-s-6.1.2` (too old for 7.x bindings) and the new `gitlab.postmarketos.org/postmarketOS/pmaports` (references `linux-postmarketos-qcom-sc8180x >= 6.6.0` but that's a kernel fork we haven't examined in detail).
3. **External DisplayPort output via USB-C alt mode on SC8180X — any reports of this working on kernel 7.x?** The SC8180X DP controller has both internal and external paths; the external path uses `typec-mux` and `qcom_pmic_glink` → USB-C DP alt mode. If this works on Flex 5G under mainline, it'd give us a working visual output even with the internal panel permanently dark. Any concrete instructions?
4. **Can we express an explicit `ath10k_snoc ← qcom_q6v5_pas@ADSP` dependency** to remove the probe-order race? Otherwise the boot-time "Wi-Fi comes up or doesn't" lottery makes remote SSH unreliable.
5. **Is there an x86-to-ARM USB-gadget debug bridge** anyone is actively using for this kind of work? PiKVM requires RPi hardware (we don't have one handy). Is there a known-good commodity USB-C device (not a full-featured dock) that acts purely as a USB-C ethernet adapter, works with Fedora ARM64 defaults, and costs under £20?
6. **Status of the pmaports `linux-postmarketos-qcom-sc8180x` kernel fork**. APKBUILD says >= 6.6; MR !8032 from "Aelin" (Feb 2026) moves it to LLVM builds. Is this tree actually boot-to-Weston on SM-W767? Any contact with Anton Bambura ("Jenneron"), the previous maintainer, indicating the port is alive or dead?

## Minimal reproducible configuration

For completeness, the kernel cmdline we believe is minimal-and-working-ish (userspace reaches multi-user, Wi-Fi probe happens ~50% of the time depending on whether `initcall_debug` is present):

```
root=UUID=<btrfs-uuid> rootflags=subvol=root
iommu.passthrough=0 iommu.strict=0 arm-smmu.disable_bypass=0
pcie_aspm.policy=powersupersave
clk_ignore_unused pd_ignore_unused
arm64.nopauth
modprobe.blacklist=msm
systemd.unit=multi-user.target systemd.firstboot=0
earlycon=efifb
reserve_mem=2M:4096:oops ramoops.mem_name=oops
  ramoops.record_size=0x4000 ramoops.console_size=0x4000

# add these for Wi-Fi to probe reliably, at cost of log spam:
systemd.log_level=debug systemd.log_target=kmsg printk.devkmsg=on
log_buf_len=4M initcall_debug keep_bootcon
```

DTB: `sc8180x-lenovo-flex-5g.dtb` (mainline, kernel 7.0.0-62).

Firmware tree:
```
/usr/lib/firmware/qcom/a680_sqe.fw
/usr/lib/firmware/qcom/a680_gmu.bin
/usr/lib/firmware/qca/crbtfw01.tlv            (supplements crbtfw21.tlv in linux-firmware)
/usr/lib/firmware/qca/crnv01.bin              (and crnv21.bin)
/usr/lib/firmware/qcom/samsung/w767/*.mbn     (ADSP/CDSP/SLPI/MPSS/WLAN/DXKMS/VSS/WDSP + mcfg carrier configs)
/usr/lib/firmware/qcom/samsung/w767/*.jsn     (adspr/adspua/battmgr/cdspr/modemuw service maps)
/usr/lib/firmware/qcom/sc8180x/LENOVO/82AK/*  (same files mirrored to Lenovo path)
/usr/lib/firmware/ath10k/WCN3990/hw1.0/wlanmdsp.mbn
```

## Things that *definitely* don't work and aren't worth revisiting

- mainline has no `sc8180x-samsung-w767.dtb` or `sc8180x-samsung-galaxy-book-s.dtb` (confirmed from `arch/arm64/boot/dts/qcom/Makefile`, master as of April 2026).
- jenneron's `galaxy-book-s-6.1.2` DTS compiles cleanly against its own DTSI but its DTB fails to initialize userspace under kernel 7.0 (the device tree drifted from mainline's `sc8180x.dtsi`; dependency and binding mismatches cause probe-order cascade that blocks init).
- `aarch64-laptops/build` has only ACPI dumps for Galaxy Book S; no DTS, no kernel patch series.
- The Linux `samsung-galaxybook` platform driver added in kernel 6.15 is **x86-only** (as documented in `Documentation/admin-guide/laptops/samsung-galaxybook.rst`).
- `hel404/Samsung_galaxybook_s_w767_drivers` is an empty README stub.

## User constraints (unchanged from v1)

- x86_64 Fedora 43 build host; image manipulation via libguestfs (can't run ARM binaries natively).
- Galaxy Book S has two USB-C ports, no other I/O. No externally accessible serial console.
- Windows 11 still on internal UFS (needed for Samsung firmware re-extraction if required).
- Internal keyboard works under Linux now; display does not; Wi-Fi works with specific cmdline incantation.
