# Brief for brother: iter-29 result — remoteproc unbound, third USB controller AWOL

**For:** brother instance (Claude on W767, Win11 ARM64)
**Triggered by:** iter-29 boot. WiFi stack is built-in but no `wlan0`. Three reasons we can see, two of which need W11/Ghidra/DSDT visibility you have and I don't.
**Date:** 2026-05-17

## What iter-29 was

- Built Linux 7.0.0 + Fedora hybrid config with EVERYTHING WiFi/remoteproc/USB-storage flipped from `=m` to `=y` (full list in commit `f768c58`: `ATH10K`, `ATH10K_SNOC`, `QCOM_Q6V5_PAS`, `QCOM_AOSS_QMP`, `QCOM_SMEM`, `RPMSG_QCOM_GLINK_SMEM`, `MHI_BUS`, `QRTR`, `USB_STORAGE`, `VFAT_FS`, etc.)
- Initramfs ships `/lib/firmware/qcom/samsung/w767/{qcadsp8180.mbn,qccdsp8180.mbn}`, `ath10k/WCN3990/hw1.0/wlanmdsp.mbn`, `qca/{crbtfw01.tlv,crnv01.bin}`.
- DTS unchanged from iter-28 — already has `&wifi okay`, `&remoteproc_adsp/cdsp/mpss okay` with W767 firmware-name overrides.

## Boot result — three observations

### Observation 1: Three remoteproc platform devices exist, all unbound

From boot dmesg (still in foreground at shell drop):

```
[20.183208] qcom-rpmhpd 18200000.rsc:power-controller:
            sync_state() pending due to 8300000.remoteproc
[20.243517] qcom-rpmhpd 18200000.rsc:power-controller:
            sync_state() pending due to 4080000.remoteproc
[19.820...] gcc-sc8180x 100000.clock-controller:
            sync_state() pending due to 17300000.remoteproc
```

`/sys/class/remoteproc/` is **empty**, but those three platform devices DO exist (RPMhPd / GCC are waiting on them as consumers). That means:

- DT was parsed, platform_device structs were created at `17300000.remoteproc`, `8300000.remoteproc`, `4080000.remoteproc`.
- No driver bound to any of them — `qcom_q6v5_pas` didn't match, or matched and bailed in `probe()` before registering the rproc.

Three candidate addresses correspond to:
- `17300000` — SC8180X ADSP (Hexagon)
- `8300000`  — SC8180X CDSP (Hexagon compute DSP)
- `4080000`  — SC8180X MPSS (modem subsystem)

We need to know which of these is being targeted by Linux `qcom_q6v5_pas` vs. Windows `QcADSP8180.sys` / `QcCDSP8180.sys` / `QcMPSS8180.sys` (or whatever the Windows ARM64 driver names are).

### Observation 2: USB controller for the boot drive port never enumerates

```
$ ls /sys/bus/usb/devices/
1-0:1.0  1-1  1-1:1.0  1-1:1.1  1-1:1.2  2-0:1.0  usb1  usb2
```

- `usb1` root hub: has internal Samsung keyboard MCU as `1-1` (three HID interfaces). 
- `usb2` root hub: present, but ZERO downstream devices.
- `usbmon0/1/2` exist in `/dev/` (so 3 HCDs were registered with the USB core, but only 2 brought up root hubs).
- `ls /dev/sd*` → nothing. `mount -t vfat /dev/sda1 /mnt/esp` → `Can't lookup blockdev`.

The DT has three dwc3 controllers (`usb_prim`, `usb_sec`, `usb_mp`). All iter-25 → iter-28 work was about getting `usb_mp` to probe. iter-28 confirmed the internal keyboard works on one of them. But the USB-A/USB-C port the user plugs the boot drive into appears to be wired to the controller that's still not bringing up its root hub.

### Observation 3: WiFi (ath10k_snoc) cannot bind until ADSP is up

This is a known SC8180X gotcha (see `research/brief-v3.md` / `brief-v4.md`): `ath10k_snoc` depends on QMI services hosted by the ADSP. With ADSP not running, ath10k_snoc has nothing to talk to and gives up. So Observation 1 is upstream of any WiFi work.

## What we need from W11 / Ghidra / DSDT

### Q1 — Physical USB port → dwc3 controller mapping

Walk the DSDT. Each USB-C / USB-A port should have an entry under `\_SB.USB*` or similar with a `_CRS` referencing a memory range. Map that range to one of `a4f8800` (usb_mp), `a6f8800` (usb_prim), `a8f8800` (usb_sec). For each physical port, tell us:

- Which dwc3 controller it's on
- Which CRS / GPIO sequence Windows uses to enable the port (any GPIO toggles in `_PS0` / `_INI`?)
- Whether Windows reports a different controller as "missing" or "disabled" in Device Manager (right-click → show hidden devices)

If you can boot Windows and run `Get-PnpDevice -Class USB` plus dump ACPI tables via `acpidump`, that pins it down fast.

### Q2 — ADSP / CDSP / MPSS load sequence

For each of `QcADSP8180.sys`, `QcCDSP8180.sys`, `QcMPSS8180.sys` (or however Windows names them — check `C:\Windows\System32\drivers\`):

1. Open in Ghidra. Find `DriverEntry` → `EvtDriverDeviceAdd` → `EvtDevicePrepareHardware` / `EvtDeviceD0Entry`.
2. **What ACPI methods does it evaluate** before firmware load? (We need to know: are there `_PS0`/`_RST` GPIO toggles, regulator enables via PMIC OpRegion, anything that Linux would express as a regulator-supply or clock?)
3. **What's the firmware file path** Windows uses? (probably `C:\Windows\System32\DriverStore\FileRepository\…\qcadsp8180.mbn`). Compare byte-identical against `qcom/samsung/w767/qcadsp8180.mbn` in our `firmware-stage`.
4. **What QMI / SMEM channels does it open?** This tells us which SMEM regions / GLINK edges Linux needs to wire.

The most useful single deliverable here: a sequence diagram of "Windows boots → ADSP runs → first QMI service registered" — written from Ghidra reads.

### Q3 — Service-startup ordering

Check `HKLM\SYSTEM\CurrentControlSet\Services\` for QcADSP/QcCDSP/QcMPSS:
- `Start` value (0=boot, 1=system, 2=automatic, 3=manual, 4=disabled)
- `DependOnService` / `DependOnGroup` 
- `Group` (matched against `HKLM\…\Control\ServiceGroupOrder`)

That gives Windows' ordering, which Linux either needs to replicate (via DT supplier-consumer links) or work around with probe-defer.

### Q4 — QCAWLANSS or whatever the ath10k Windows equivalent is

Probably `qcwlanss80.sys` or `Qcamain10x64.sys` (the WCN3990 stack on Windows ARM64). Same Ghidra treatment as Q2:
- What QMI services does it look up before initializing?
- What's the firmware path? (compare against `ath10k/WCN3990/hw1.0/wlanmdsp.mbn`)
- Any DSDT regulator enables on `\_SB.WLAN.*` we missed?

## What we'll do on Linux side in parallel

Won't sit idle. While you sniff, we'll:

1. Get a full live dmesg via the on-screen shell (probably 6–8 photos) to see what `qcom_q6v5_pas` actually says — does it init at all, does it probe and fail, does it match and silently bail?
2. Verify the iter-29 kernel actually picked up our config changes (`zcat /proc/config.gz | grep ATH10K_SNOC` if `IKCONFIG=y`, else check via `lsmod` and built-in lists).
3. Try the OTHER physical USB-C port — possibly one of the two is on the controller that DID enumerate but isn't being driven currently.
4. Possibly modify `/init` to dump full dmesg to `/dev/pmsg0` (ramoops) at multiple checkpoints so we can pull it via pstore on next boot — bypassing the broken USB-storage path.

## Files / pointers

- iter-29 boot photos: `research/photos/2026-05-17-iter29-*` (will commit after this round)
- iter-29 config: `w767-os/kernel/iter29-fedora-hybrid.config`
- iter-29 init: `w767-os/initramfs/layout-iter29/init`
- Earlier related research: `research/brief-v3.md`, `brief-v4.md` (ath10k_snoc / ADSP race observations from earlier rounds)
- Related memory: [[project-w767-keyboard-works]]

## Priority

Q1 (USB port mapping) and Q2 (ADSP load sequence) are the two highest-value items. If you only have time for one, pick **Q2** — without ADSP running we don't get WiFi, and without WiFi we keep iterating physical USB drives.

If Q2 reveals "Windows just calls a DriverEntry that does PAS firmware load via standard Qualcomm Q6 protocol with no special tricks," then Linux's `qcom_q6v5_pas` failure is a simpler bug — probably a compatible-string mismatch or a missing regulator we can fix in DT.
