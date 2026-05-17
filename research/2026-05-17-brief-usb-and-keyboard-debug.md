# Research brief — why USB + internal keyboard didn't enumerate on iter-22

**For:** brother instance (Claude on W767, Win11 ARM64)
**Context:** iter-22 booted, kernel ran, initramfs printed banner + shell prompt on tty0, but:
- USB host controllers never enumerated (no `/dev/sda1` visible → ESP auto-snapshot didn't write)
- Internal keyboard didn't work (consistent with USB not coming up, since brother's previous round established the keyboard is USB-HID on `\_SB.USB2.RHUB.MP0` PID_A055)
- Screen went black some seconds after the shell prompt appeared (unclear if hardware idle, DRM_MSM crashed during late probe, or something else)
- User had to power-cycle (no keyboard = no `poweroff -f`) — that wiped ramoops, so we have **no captured dmesg from this boot**

We're flying blind on the actual failure mode. The Linux side will improve `/init` to capture better next round (write to `/dev/pmsg0` immediately, retry ESP mount in background loop), but **the higher-leverage question is: what's needed to bring up the SC8180X internal USB2 stack that we might be missing?**

That's where brother's Win11-ARM access becomes critical: Windows is bringing this up successfully, so the recipe is visible in the OEM INFs, driver init code, and ACPI _CRS / power resources for the relevant nodes.

## What we know vs don't know

| Item | Status |
|---|---|
| iter-22 PSCI cpuidle fix (`ARM_PSCI_CPUIDLE_DOMAIN=y`) compiled in | ✅ Verified in built config |
| Kernel reached initramfs + shell | ✅ Visual confirmation |
| Earlycon=efifb rendered through ExitBootServices | ✅ Visual confirmation |
| `cluster_pd` registered (the iter-22 fix's purpose) | ❓ Unknown without dmesg |
| `&rpmh_rsc @ 0x18200000` probed | ❓ Unknown |
| PMC8180-a/c RPMh regulators bound | ❓ Unknown |
| `100000.clock-controller` probed | ❓ Unknown |
| `1500000.interconnect` (and siblings) probed | ❓ Unknown |
| USB DWC3 host controllers (`a4f8800` / `a6f8800` / `a8f8800`) probed | ❌ Strong signal: no `/dev/sda1` after boot |
| Internal USB2 hub (`\_SB.USB2.RHUB`) enumerated | ❌ Same evidence |
| SPACE v57 (`USB\VID_04E8&PID_A055`) device appeared as HID | ❌ Same evidence |
| DRM_MSM probed cleanly OR fell back to simpledrm | ❓ Unknown — screen went black, could be probe failure OR display PM idle |

The dropped USB enumeration is the single biggest signal. Everything follows from "USB host controllers blocked" — including the internal keyboard not working.

## What brother could investigate (Windows side)

### Q1 — Exactly which DSDT path does the SPACE v57 device live on?

Brother said earlier: "internal USB-HID composite at `\_SB.USB2.RHUB.MP0` (VID_04E8 PID_A055)". Please re-verify and capture the full ACPI path with one of:

```powershell
Get-PnpDevice -InstanceId "USB\VID_04E8&PID_A055*" | Format-List *
pnputil /enum-devices /instanceid 'USB\VID_04E8&PID_A055*' /properties
```

Looking specifically for the `DEVPKEY_Device_LocationInfo` and `DEVPKEY_Device_LocationPaths` properties. The path tells us which **physical USB controller** the SPACE MCU is plugged into. Three candidates in the DSDT:

```
\_SB.USB0  ← MMIO 0xa4f8800 (DT &usb_prim)
\_SB.USB1  ← MMIO 0xa6f8800 (DT &usb_sec)
\_SB.USB2  ← MMIO 0xa8f8800 (DT &usb_mp)  ← brother's prior claim
```

We need this to know which controller MUST come up for the keyboard. The two USB-C ports also live on USB0 and USB1; the internal MCU + dock connector live on USB2.

### Q2 — What's `\_SB.USB2`'s full `_CRS` + dependency chain in DSDT?

Decode the USB2 device's `_CRS` for `Memory32Fixed` (MMIO base) + `Interrupt` (IRQ) + `GpioInt/Io` (overcurrent / VBUS sensing) + `_DEP` (power resources / dependencies).

Specifically need:
1. MMIO base (should be `0xa8f8800` if our DT phandle mapping is right — please confirm)
2. Any `_DEP` references — these are the **ACPI power resources** the controller needs ON before it can probe. Each `_DEP` entry has an `_ON` method that flips clocks/regulators on. We need to know which RPMh regulator IDs + which clock branches.
3. The PHY's MMIO base — should be at `0xa8e?000` per the DTSI (sibling node)
4. Any `_PRW` (wake-from-S3 GPIOs) — relevant for understanding the USB power topology

### Q3 — What does the Windows driver enable, in order, to bring up `\_SB.USB2`?

The Windows USB stack initialization is roughly: ACPI evaluates the device's `_ON` power resource → clocks come up → regulators come up → PHY trains → controller becomes responsive → root hub enumerates → SPACE MCU enumerates → HID class binds.

The `_ON` method for the USB2 power resource will have a long sequence of writes to MMIO offsets under the GCC (Global Clock Controller) and the RPMh RSC. **Quote the full _ON method body** from the AML, hand-decoded if needed (~50–100 bytes of ASL).

For reference, the equivalent path in mainline Linux DTS for a SC8280XP USB controller looks like:

```dts
usb_prim: usb@a6f8800 {
    compatible = "qcom,sc8180x-dwc3", "qcom,dwc3";
    clocks = <&gcc GCC_USB30_PRIM_MASTER_CLK>, ...;
    resets = <&gcc GCC_USB30_PRIM_BCR>;
    power-domains = <&gcc USB30_PRIM_GDSC>;
    interconnects = <&aggre1_noc MASTER_USB3 ...>;
    phys = <&usb_prim_qmpphy QMP_USB43DP_USB3_PHY>, <&usb_prim_hsphy>;
    ...
};
```

If any of those (clock IDs, power-domain IDs, interconnect paths, PHY refs) are wrong in our DTS, the controller silently fails to probe even with all the right kconfigs. Knowing what Windows actually enables would let us verify.

### Q4 — Is there a vendor-specific init sequence the SPACE MCU expects?

Some Samsung-internal MCUs require a firmware upload over USB or a vendor-specific control request before they'll start emitting HID reports.

Three places to check:
1. **`kbdHelper.sys` strings** — brother noted it's an OSD filter, but it might also issue a firmware-load IOCTL at start of day. Grep for `"FIRMWARE"`, `"FW_LOAD"`, `"BOOT"`, `"INIT"`, `"VendorRequest"`, `"USBControl"` in the binary.
2. **The SPACE v57 USB descriptors** — capture full descriptors via:
   ```powershell
   # Either Windows' built-in USBView (download from Microsoft) or:
   Get-WmiObject Win32_USBControllerDevice |
       Where-Object { $_.Dependent -match 'A055' } |
       Select Dependent | Format-List
   ```
   Need: device descriptor, all configuration/interface descriptors, all HID report descriptors per interface (especially MI_02, the "vendor-defined" one whose role we don't fully know).
3. **`oem15.inf` `kbdHelper_SamsungOSDSvcInstall`** entry references `ComponentIds = VEN_SAMS&PID_0906` — there might be a UWP service that talks to MI_02 and tells the SPACE MCU "I'm ready, start sending reports." Quote the InstallSection from the INF and any AddService entries pointing at user-mode .exe files.

### Q5 — What does the DSDT say about USB-PD / Type-C controller dependencies?

The earlier recon-decode found these chips on the USB-C ports:
- S2MM005 USB-PD CC controller (×2)
- SM5508 MUIC (×2)
- PTN36502 SuperSpeed redriver (×2)

These are on the **external** USB-C ports (USB0 / USB1). But it's worth checking if `\_SB.USB2` (internal MCU) has any analogous dependency we don't expect — e.g., does it wait for a PD negotiation to complete before powering up? On the off chance there's a chain like:

```
USB2 root hub power → Type-C PD negotiation → S2MM005 firmware ready → ...
```

we'd need to handle that too. Decoding the `_DEP` of `\_SB.USB2` will reveal this if it's the case.

### Q6 — What's the difference between iter-17 (which worked) and iter-22 (which didn't)?

Big-picture: iter-17 was the **stock Fedora aarch64 kernel** with all the Snapdragon drivers built ~thousand-flag style. iter-22 is our `allnoconfig + 200-flag merge` build. The pre-boot audit already found we're missing things (PM=y, SYSFB, FB_EFI, PSCI cpuidle).

Brother could grep the equivalent `Fedora generic-aarch64.config` (from any Fedora kernel-source RPM) for everything containing `USB`, `DWC3`, `PHY_QCOM`, `INTERCONNECT_QCOM`, `GENI`, `GCC_USB`, `SDHCI`, `QCOM_SCM`, and compare against our `w767-initramfs.config`. Anything in Fedora's config that's not in ours and isn't obviously irrelevant is a candidate for the next "silently dropped config" we're missing.

The Fedora kernel RPM is at:
```
https://kojipkgs.fedoraproject.org//packages/kernel/7.0.0/62.fc45/aarch64/
```

The config of interest is `/boot/config-7.0.0-62.fc45.aarch64` once extracted. If brother can `Invoke-WebRequest` it (small file, ~250 KB) and grep, that'd close the comparison loop.

## What the Linux side will do in parallel

While brother investigates, Linux side will land **iter-23** with:

1. Enhanced `/init` that:
   - Waits up to 15 s for `/dev/sda1` to appear before snapshotting (USB might take seconds to enumerate)
   - **Writes a compact dmesg to `/dev/pmsg0` immediately on boot** so we capture *something* in ramoops even when USB stays dead
   - Spawns a background snapshot retrier so late USB enumeration still produces a snapshot
2. `consoleblank=0` in BLS cmdline to disable framebuffer auto-blanking (rules out one possibility for the post-shell black screen)
3. Possibly `video=efifb:keep` or similar to influence simpledrm → DRM_MSM handover

That way the next boot will produce diagnostics even if USB never comes up — pmsg0 doesn't depend on any block device.

## Deliverable expected from brother

A `research/2026-05-1?-claude-usb-stack-deps.md` answering:

- **Q1** — confirmed ACPI path for SPACE v57
- **Q2** — full `\_SB.USB2._CRS` + `_DEP` decoded
- **Q3** — `_ON` method body for the USB2 power resource (the canonical "what Windows enables to bring this up"), plus a list of the specific GCC clock IDs and RPMh regulators it toggles
- **Q4** — whether SPACE MCU needs a vendor handshake before reporting; if yes, what
- **Q5** — any USB-PD / Type-C dependencies on the internal USB2 path
- **Q6** — diff of relevant CONFIG_* between Fedora kernel and our minimal config (where Fedora has =y / =m and ours has =n / missing)

The §3 + §6 deliverables are the highest priority — together they should pin down whether the failure is "missing kconfig", "wrong DT bindings", or "missing vendor handshake".

## Out of scope (don't bother)

- **Don't** try to load the iter-22 USB image in Windows to inspect — it's an ESP-only image, won't boot under Windows
- **Don't** spend cycles on more Ghidra disassembly of EmuEC/VHIDEvent — keyboard *protocol* is solved; this round is about *bring-up dependencies*
- **Don't** investigate the touchpad separately — same USB controller, same root cause; once Q1–Q3 pin down the issue, touchpad and keyboard both unblock at once
- **Don't** worry about the post-shell black screen yet — that's a secondary issue, lower priority than the USB enumeration question
