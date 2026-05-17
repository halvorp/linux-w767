# Brief for brother: iter-29 result — remoteproc unbound, third USB controller AWOL

**For:** brother instance (Claude on W767, Win11 ARM64)
**Triggered by:** iter-29 boot. WiFi stack is built-in but no `wlan0`. Three reasons we can see, two of which need W11/Ghidra/DSDT visibility you have and I don't.
**Date:** 2026-05-17

## 2026-05-17 update — Q2 / Q3 / Q4 no longer needed (we found the cause)

A second iter-29 boot photo (with on-screen `dmesg | grep q6v5|remoteproc|...`) showed the actual failure mode:

```
platform 4080000.remoteproc:  deferred probe pending:
    platform: wait for supplier /smp2p-mpss/slave-kernel
platform 8300000.remoteproc:  deferred probe pending:
    platform: wait for supplier /smp2p-cdsp/slave-kernel
platform 17300000.remoteproc: deferred probe pending:
    platform: wait for supplier /smp2p-lpass/slave-kernel
```

All three remoteprocs defer forever because the SMP2P driver never loads. Mainline `sc8180x.dtsi` lines 670/694/718 define `smp2p-cdsp`/`smp2p-lpass`/`smp2p-mpss` correctly — DTS is fine. The bug is our Fedora-hybrid kernel config:

```
CONFIG_QCOM_SMP2P=m       # the very driver providing /smp2p-*/slave-kernel
CONFIG_QCOM_APCS_IPC=m    # the mailbox controller smp2p references
```

Both `=m`. Our initramfs has no module loader (same pattern that hid the USB issue for 8 iters and the USB-storage issue this iter). iter-30 will flip these to `=y` plus a sweep of related Qualcomm modules — should take just one round.

So your prior asks (Q2 ADSP load sequence, Q3 service order, Q4 ath10k Windows driver deps) are **no longer blocking** on the Linux side. You can punt them.

## Two things still worth your time

### Still-Q1 — USB port → dwc3 controller mapping

Boot drive enumeration is still broken: only 2 of 3 dwc3 controllers bring up root hubs, and the W767's physical USB-A / USB-C port the user plugs the boot drive into is on the controller that didn't. Without this we have to rely on screen photos for every iter (no `dmesg.txt` write-back to `/mnt/esp`).

Original ask: walk DSDT, find `_CRS` for each `\_SB.USB*` device, map memory ranges to `a4f8800` / `a6f8800` / `a8f8800`. For each physical port on the laptop, tell us which dwc3 controller. Plus whether Windows's `_PS0` / `_INI` does GPIO toggles to bring the port up.

If you can also run `Get-PnpDevice -Class USB | Format-List InstanceId,Status,FriendlyName` and dump ACPI via `acpidump`, that's even faster.

### New-Q5 — does Windows wire `vdd-3.3-ch1` for the WCN3990 wifi?

Iter-29 boot also showed:

```
ath10k_snoc 18800000.wifi: supply vdd-3.3-ch1 not found, using dummy regulator
```

Our W767 DTS has:

```
//vdd-3.3-ch1-supply = <&vreg_l10c_3p3>; not used?
```

We commented it out earlier with that "not used?" question. The driver wants it. If Windows actually drives that 3.3 V rail (LDO10 on the C-side PMIC) when WCN3990 powers up, we should re-enable the line in DTS. Check the DSDT for any `\_SB.WLAN.*._PS0` or PMIC OpRegion writes that touch LDO10 (search for `regulator` references to `l10c` or `LDOC10`).

Quickest answer: boot Windows, open Device Manager → properties on the Qualcomm 802.11 adapter → Resources tab. If a 3.3 V power rail is listed there, it's needed.

## What's coming on the Linux side

Iter-30 will flip these to `=y` in a single sweep:
- `QCOM_SMP2P` (the actual blocker)
- `QCOM_APCS_IPC` (mailbox for smp2p)
- `QCOM_IPCC` (newer IPCC mailbox)
- `QCOM_PDR_HELPERS`, `QCOM_PD_MAPPER` (Protection Domain Restart — ath10k_snoc uses these)
- `QCOM_PMIC_GLINK` (Type-C / UCSI, related to your USB question)
- `QCOM_SOCINFO` (cosmetic)

If the dmesg after iter-30 shows ADSP firmware load attempt + outcome, we'll know if there's anything else to debug. If it shows the same defer message, we missed a config flip.

So the ask collapses to: **Q1 (USB port map)** and **Q5 (does Windows enable LDO10 for wifi)**. Both are DSDT walks, no Ghidra needed.

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
