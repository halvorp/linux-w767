# iter-28 result: internal keyboard works

**Date:** 2026-05-17
**Status:** ✅ Internal Samsung Galaxy Book S keyboard enumerates and types.

## What the boot photo shows

`research/photos/2026-05-17-iter28-keyboard-works.png`

```
--- USB devices: ---
1-0:1.0 1-1 1-1:1.0 1-1:1.1 1-1:1.2 2-0:1.0 usb1 usb2

--- HID devices: ---
0003:04E8:A055.0001  0003:04E8:A055.0002  0003:04E8:A055.0003
```

Breakdown:
- `usb1`, `usb2` — two xhci controllers up (`usb_prim` and `usb_sec`, or `usb_sec` and `usb_mp`; need to confirm from the un-truncated dmesg)
- `1-1` — Samsung "SPACE v57" MCU enumerated on bus 1 port 1
- `1-1:1.0`, `1-1:1.1`, `1-1:1.2` — three USB interfaces on the MCU
- Three HID devices on `VID_04E8:PID_A055` — that's the keyboard + presumably media/fn keys + maybe a vendor-specific debug interface

User confirms keys are typed on the on-screen output (kernel VT echo).

## Root cause — what actually made it work

It was **not** any of the things we spent iter-19 through iter-27 chasing.

The fix was iter-28's switch to Fedora's full `.config` with `USB_DWC3=y`, `USB_DWC3_QCOM=y`, `PHY_QCOM_QMP_USB=y`, `PHY_QCOM_USB_SNPS_FEMTO_V2=y`, `USB_XHCI_PLATFORM=y` flipped from `=m` to `=y`.

In all prior iters (minimal config and iter-25's first Fedora attempt), the dwc3 stack was either disabled or compiled as modules. Our initramfs has no module loader, so no `dwc3-qcom` driver ever bound to any USB controller. That's why no USB device ever enumerated.

The `sync_state pending due to a8f8800.usb` messages we read for many iters as "usb_sec partly probed" were actually just registered `platform_device`s waiting for a driver that never arrived. That interpretation, called out in `2026-05-17-iter28-dwc3-instrumentation.md`, turned out to be the entire bug.

## What does NOT appear to have mattered (but is still in the DTB)

These were applied in earlier iters and are still in the canonical DTS. They may or may not be required — we have not tested with them stripped:

- **iter-26**: `gpio-hog` asserting HSEI/MOD1 (GPIO 35) high
- **iter-26**: trimming `usb_mp` interrupts list to 8 (dropping `ss_phy_*`)
- **iter-27**: compatible override on `usb_mp` to plain `qcom,sc8180x-dwc3`
- **iter-22**: `ARM_PSCI_CPUIDLE_DOMAIN=y` — pretty sure this IS needed (without it cluster_pd doesn't register and the whole power-domain graph fails)
- **iter-23**: `QCOM_PDC=y` — wakeup IRQ routing, likely needed for any wakeable USB device

The instrumentation patch (`kernel-patches/iter28-diag/0001-...patch`) is no longer load-bearing for the bring-up — it was a diagnostic that fired *during* probe and helped confirm the dwc3-qcom-legacy path was being taken. We can keep it as a documentation aid or strip it for a clean follow-up build.

## Implication for project direction

Original plan after iter-28 was to swap base tree to `gitlab.com/sc8180x-mainline/linux`. **No longer needed.** Mainline 7.0.0 + Fedora config has every driver we needed for the internal keyboard.

Next milestones for daily-driver use (in rough priority order):

1. **Display** — currently on simpledrm/EFI framebuffer (no GPU). MSM DPU + Adreno 680 needs `drm/msm` enabled + Adreno firmware blob from `linux-firmware` (`a680_*.fw`). Once GPU works, console is hardware-accelerated and `nomodeset` can be dropped.
2. **Touchpad** — the I2C HID node at `i2c1@0x49` should enumerate now that the I2C bus is up; check on next boot whether a touchpad input device exists in `/sys/class/input/`.
3. **Audio** — CS35L41 codec (Cirrus). Needs `SND_SOC_CS35L41_I2C=y` + firmware (`cs35l41*.bin`).
4. **WiFi/BT** — QCA6390 (WCN3998). Needs `ath11k` + firmware blobs from Qualcomm. Brother has these in firmware-stage.
5. **Battery / thermal / suspend** — RPMh side; needs `qcom_smem`, `qcom_aoss`, full PSCI stack tested for suspend-to-RAM.

We're now well past the "is this possible?" stage. From here it's a parts-and-firmware project, not a debug-the-bring-up project.

## Files

- Boot photo: `research/photos/2026-05-17-iter28-keyboard-works.png`
- Built kernel: `w767-os/kernel/out/w767-initramfs/Image` (Linux 7.0.0 + Fedora hybrid config + dwc3 instrumentation, 65 MB)
- Image: `/tmp/w767-iter28.img` (local only)
- DTB: `arch/arm64/boot/dts/qcom/sc8180x-samsung-w767.dtb` (iter-27 unchanged)
