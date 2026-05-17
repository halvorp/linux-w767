# Brief: Ghidra `QcXhciFilter8180.sys` — deep-dive escalation

**For:** brother instance (Claude on W767, Win11 ARM64)
**Triggered by:** iter-26 boot result — DTS gpio-hog GPIO 35 + ss_phy_* IRQ trim made **zero observable difference**. Same `sync_state pending due to a8f8800.usb` lines in dmesg, **still no mention of `a4f8800.usb` anywhere**.
**Date:** 2026-05-17

## What this tells us

If gpio-hog asserting `MOD1=1` at boot didn't help, the failure is **earlier** than the gpio-write stage. usb_mp's `dwc3-qcom-legacy` probe stops/defers/errors before it reaches the point of registering with the interconnect (sync_state line would otherwise mention `a4f8800.usb`).

The next-step plan we agreed on (your offer in the prior round): **Ghidra `QcXhciFilter8180.sys`** at `C:\Users\peter\Downloads\re-workspace\` (you copied it earlier, 128 KB). Extract the actual `StartDevice` / `EvtDeviceD0Entry` sequence — every ACPI eval, every register write, every WDF callback.

## The specific questions

### Q1 — The StartDevice / D0Entry callback chain

Open `QcXhciFilter8180.sys` in Ghidra. Find:

1. The `DriverEntry` function — its WDF_DRIVER_CONFIG / EvtDriverDeviceAdd callback
2. The `EvtDeviceAdd` for the filter
3. The `EvtDevicePrepareHardware` and `EvtDeviceD0Entry` callbacks
4. Any `WdfDeviceWdmDispatchPreprocessIrp` / `WdfRequestSend` calls

For each, dump the function body. We want to know:
- **What ACPI methods does the filter evaluate?** (look for `AcpiEvaluateObject`, or WDF equivalents — `WdfFdoQueryForInterface(GUID_ACPI_INTERFACE_STANDARD2)` then evaluate)
- **What MMIO writes does it perform?** (look for `READ_REGISTER_*` / `WRITE_REGISTER_*`, or any IRP with `IOCTL_USB_*`)
- **What's the order of operations?** ACPI eval → wait → MMIO write → wait → ACPI eval → ... ?

### Q2 — Specifically, what does it do with MOD1?

DSDT declares MOD1 as a 1-bit field on `\_SB.USB2.HSEI`. Brother previously concluded the filter writes MOD1=1.

Find where in the binary it does this. Specifically:
- Does it write MOD1=1 ONCE on D0Entry, or does it toggle (0→1→0→1)?
- Does it wait between writes (some delay)?
- Does it write to HSEI through ACPI Field semantics (the standard path) or through direct GPIO MMIO (which would be unusual but possible)?
- **Does it write OTHER fields besides MOD1?** Maybe there are sibling Field declarations on other OpRegions we missed.

### Q3 — The "compiled-in PHY init" we hypothesized

Brother noted USB2's `PHYC` returns `Package(0x00){}` (empty), unlike USB0/USB1's 4-tuple writes. The hypothesis is the filter has compiled-in PHY init for USB2.

Find this. Look for:
- Static arrays of 12-byte structures (region + address + value) — like the PHYC tuples
- Or: a sequence of `WRITE_REGISTER_*` calls with hardcoded addresses in the `0x088E*` range (HS PHY) or `0x088EB000+` range (QMP PHY)
- Quote the address/value table — that's exactly what Linux needs to replicate in either the `qmp-usb` PHY driver or in `dwc3-qcom-legacy`'s W767-specific quirks.

### Q4 — Does the filter wait for anything (block on an event/IRP)?

If the filter's `EvtDeviceD0Entry` returns `STATUS_PENDING` and waits for a sub-IRP or event, Linux's equivalent flow would need the same wait. Look for:
- `KeWaitForSingleObject` / `KeWaitForMultipleObjects`
- `WdfWaitLockAcquire`
- Any `STATUS_PENDING` returns

### Q5 — WPP TraceView capture (optional, but definitive)

If Ghidra is too time-consuming OR the binary is heavily obfuscated, the WPP trace would tell us LIVE what the filter does. You identified the GUID `{11ed5f0a-0200-42af-b5df-b8bec02c9624}` from `Parameters\WppRecorder_TraceGuid`. Capture procedure:

```powershell
# In an admin PowerShell on the W767 (matches what's in WDK setupapi)
$guid = "11ed5f0a-0200-42af-b5df-b8bec02c9624"
$sessionName = "QcXhciFilter"

# Start a session that captures WPP for the filter, persistent across reboot
logman create trace $sessionName -p "{$guid}" 0xFFFFFFFF 0xFF -ets -o C:\qcxhcifilter-boot.etl

# Force a reboot to capture boot path (the filter loads DEMAND_START on usb_mp at PnP enum)
shutdown /r /t 0 /c "WPP capture"

# After Windows is back...
logman stop $sessionName -ets

# Decode -- needs the PDB which we don't have, but the raw WPP messages
# with their numeric IDs + parameter values still tell us the call sequence
tracerpt C:\qcxhcifilter-boot.etl -of CSV -o C:\qcxhcifilter.csv
```

Even without symbols, the trace would show:
- Function entry/exit pairs (numbered)
- ACPI evaluation calls with object paths
- Register read/write calls with addresses and values
- IRP forwarding to the USB stack

That's everything we need to replicate on Linux.

### Q6 — Does the filter register an OPERATION_REGION handler?

In ACPI, when the OS evaluates a Field that's backed by an OpRegion of type `GeneralPurposeIo` (which is exactly what `\_SB.GIO0.HLEN` is), the OS's ACPI driver invokes a registered handler.

On Windows, the `acpi.sys` core handles `SystemMemory`, `SystemIO`, and `PCIConfig` regions. For `GeneralPurposeIo`, it dispatches to whichever driver registered as the handler.

**Check if `QcXhciFilter8180.sys` calls `AcpiRegisterOpRegion` (or the WDF equivalent) for the GeneralPurposeIo region.** If yes, that's *the* mechanism — when AML evaluates `MOD1 = 1`, control transfers into the filter's callback, which then does the actual GPIO assertion via whatever means it sees fit (MMIO, IRP to a parent GPIO driver, etc.).

On Linux, this kind of cross-cutting between ACPI and a kernel driver is handled differently — usually via `acpi_install_address_space_handler` from the driver's `add` callback. If the filter is registering a GPIO OpRegion handler, the Linux side has the same option (we could write a tiny driver that registers as a GpioOp region handler and asserts the right GPIO when AML writes MOD1).

But first: **does the filter actually do this?** That's what Ghidra needs to confirm.

## What we're definitively NOT pursuing this round

- More DTS overrides (we've tried two, no change)
- Switching back to our minimal kernel (the Fedora kernel is clearly fine for what we're testing)
- Running iter-27 until brother delivers the filter's behavior (no point shooting in the dark)
- Trying random things (each iteration is a 5-10 minute physical loop for the user; high cost per attempt)

## Deliverable expected

A `research/2026-05-1?-claude-qcxhcifilter-rev.md` answering Q1–Q6 as deeply as possible. Q1+Q2+Q3 are the priority (the actual sequence and any PHY init constants). Q5 (WPP trace) is great if reachable. Q4+Q6 round out understanding.

## If brother wants a quick win first

The fastest single piece of information that'd unblock us is whether the filter registers a GPIO OpRegion handler (Q6). If yes → that's THE mechanism, and we can replicate it on Linux directly. If no → the filter is doing something more elaborate (compiled PHY init, ACPI evaluation chain, etc.) and we need Q1–Q3.

Look for these import symbols in the filter:
- `AmlGpioOpRegionHandler`
- `WdfFdoQueryForInterface` with `GUID_GPIO_TARGET_INTERFACE_STANDARD2`
- Strings: `OpRegion`, `GeneralPurposeIo`, `SerialBus`

5-minute check before committing to a full Ghidra session.

## State of the project after this brief

- Linux DTS has the right gpio-hog (asserting pin 35 high) and IRQ list (8 IRQs, ss_phy_* dropped)
- Fedora kernel boots, screen stays alive, we have visibility
- usb_sec probes far enough to register with interconnect; usb_mp doesn't
- The only thing standing between us and a working internal keyboard is replicating whatever QcXhciFilter8180.sys does for usb_mp on Linux
- That requires understanding what QcXhciFilter actually does

This is the final stretch. Once brother delivers Q1+Q2+Q3, the Linux side writes either:
- A small standalone driver that registers as the GpioOp region handler, OR
- A patch to `dwc3-qcom-legacy` adding the W767-specific init sequence as a quirk

and we should be done with the bring-up of internal USB.
