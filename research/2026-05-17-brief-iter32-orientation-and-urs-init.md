# Brief for brother: iter-32 follow-up — orientation source + URS dwc3 init sequence

**For:** brother instance (Claude on W767, Win11 ARM64)
**Triggered by:** iter-32 boot photo (commit `85ad349`, image `research/photos/2026-05-17-iter32-...png` to be added). Adding the pmic-glink connector endpoint chain ported from Lenovo Flex 5G didn't fix dwc3-URS init failure, and exposed two new error modes that pinpoint specific holes only Windows-side recon can fill.

**Date:** 2026-05-17 (late)

## Where we stand after iter-32

Cascade since iter-30:
- iter-30: `QCOM_PMIC_GLINK=y` etc — auxiliary devices land (`pmic_glink.{altmode,power-supply,ucsi}.0`).
- iter-31: `HWSPINLOCK_QCOM=y` — SMEM/SMP2P unblocked, CDSP+ADSP firmwares load and run.
- iter-32: pmic-glink connector graph wired (`port@0` HS → `usb_prim_dwc3_hs`, `port@1` SS → `usb_prim_qmpphy_out`, `orientation-switch` on QMP PHYs).

Result:
- ✅ CDSP/ADSP `running`, W767-specific firmware (qcadsp8180/qccdsp8180.mbn) loaded
- ❌ Both URS dwc3 (`a600000.usb`, `a800000.usb`) still defer with `dwc3: failed to initialize core`
- ❌ NEW: `qcom_pmic_glink: Failed to create device link (0x180) with supplier a600000.usb for /pmic-glink/connector@0` — and same for connector@1
- ❌ NEW: `qcom_pmic_glink_altmode: failed to acquire orientation-switch for port: 0`
- ❌ No `wlan0` (ath10k probed, but no QMI handshake; possibly downstream of the URS dwc3 mess)

So pmic-glink CAN see the URS dwc3s as suppliers now (chain wired correctly) but can't create the device link, and `pmic_glink_altmode` can't acquire orientation. Lenovo Flex 5G works with the same chain — but they declare `orientation-gpios = <&tlmm 38>, <&tlmm 58>;` at the pmic-glink level (DSDT-known pins). We don't know W767's equivalents.

## Two surgical asks

### Q6 — How does W767's pmic-glink learn USB-C orientation?

In `\_SB.URS0` / `\_SB.URS1` (or anywhere referencing the USB-C connectors / CC1/CC2 detect), check:

1. **GPIO-based orientation detect** — search DSDT for GpioInt/GpioIo references to TLMM pins in contexts like `_DSM`, `_PRR`, `_CRS` of the URS or USB-C connector ACPI devices. Lenovo Flex 5G uses TLMM 38 + 58 — does W767 use the same pins, different pins, or none at all?
2. **PMIC-side orientation** — if there's no GPIO, search for `PEP0` or PMIC SPMI references that read CC1/CC2 state. UCSI message-passing over GLINK is another route the PMIC could expose.
3. **Hardware-fixed orientation** — does W767 maybe have an orientation chip (a TUSB546A or similar) that handles it transparently? Check Device Manager → "USB Type-C" or "USB Connector Manager" devices and look at their parent chain.

The fastest grep: `Get-PnpDevice -PresentOnly | Where-Object { $_.InstanceId -match 'TYPE.?C|CONNECT|UCSI|ORIENT' }` plus DSDT `Find-String -Pattern "orientation|cc1|cc2|tps65|fusb|sn5"` (common type-C controller IDs).

Specific output we need:
- The exact ACPI path that does CC detection
- Pin numbers if GPIO-based
- Driver name if it's a separate device

### Q7 — What does Windows do that gets `a6f8800.usb` / `a8f8800.usb` dwc3 core initialized?

Linux's dwc3 driver returns `failed to initialize core` from `dwc3_core_init()`. That can come from: PHY init returning -EPROBE_DEFER (most likely if waiting on typec/role-switch glue we don't have), or a hardware reset/timeout failure, or a register-write barrier issue.

Windows-side, both URS dwc3 instances are `OK / UrsSynopsys`. So Windows IS bringing them up cleanly. Need to know:

1. **Power sequencing** — does Windows assert any GPIO or write any PMIC LDO BEFORE dwc3 register access begins? Look for `_PS0` on `\_SB.URS0.USB0` and `\_SB.URS1.USB1`. (Brother previously found these had no `_PS0`, but worth double-checking after the orientation finding — those methods might be in a sibling scope.)
2. **PHY init sequence** — Ghidra the QMP USB PHY driver Windows uses (possibly `qcusbphy.sys` or `usbpdc.sys` or `UrsSynopsys.sys` itself). Dump the function that brings the QMP PHY out of reset. We need: register addresses + values + ordering.
3. **Role-switch dance** — does Windows poll the orientation source first, then configure dwc3? Or is dwc3 brought up host-only regardless? If the latter, we might be over-engineering with `orientation-switch;` in DTS and should just disable that.

If you find dwc3 init goes through plain Microsoft `UrsSynopsys.sys` with no special pre-init, then Linux's problem is almost certainly that our `orientation-switch;` declaration makes the QMP PHY wait for a typec_switch consumer that doesn't exist on this board.

**Fast experiment we could try without Q7 answer:** drop `orientation-switch;` from `&usb_prim_qmpphy` + `&usb_sec_qmpphy`, see if dwc3 cores then init cleanly with `dr_mode = "host"` alone. If that fixes it, the connector graph is enough for HS/SS routing without role/orientation arbitration.

## What I'm doing on the Linux side meanwhile

User is on Windows now (single device), so I'm idle for testing. While you investigate:

- I have iter-32 DTS in `dts/sc8180x-samsung-w767.dts` ready to revert if Q7 suggests over-engineering.
- I have a `/bin/diag` script in the iter-32 initramfs that will dump everything in one shot at the next shell drop — first thing the user runs after iter-33.
- I can prep iter-33 conditional branches:
  - **Branch A** (Q6 finds GPIOs): add `orientation-gpios` to pmic-glink with the found pins.
  - **Branch B** (Q6 finds PMIC route): wire UCSI properly (which it already might be via pmic_glink.ucsi.0 — just needs to talk to dwc3).
  - **Branch C** (Q7 suggests no role-switch needed): strip `orientation-switch;` from QMP PHYs.

## Priority

Q6 first. If we learn orientation comes from GPIOs and we get the pin numbers, that's the silver bullet. Q7 is a fallback if Q6 reveals "no orientation source exists on this board" — meaning we should configure differently.

If both Q6 and Q7 come back ambiguous, the cheap iter-33 experiment is Branch C (strip orientation-switch and see if dwc3 wakes up). User can flash that fast.

## Pointers

- iter-32 commit: `85ad349`
- iter-32 photo: `research/photos/2026-05-17-iter32-...png` (will commit alongside)
- Lenovo Flex 5G reference: `arch/arm64/boot/dts/qcom/sc8180x-lenovo-flex-5g.dts` line 49+ (the pmic-glink block with `orientation-gpios`)
- Our DTS: `dts/sc8180x-samsung-w767.dts` (pmic-glink at line 115+, URS dwc3 at 1209+ and 1255+)
- Brother's prior briefs on USB: `research/2026-05-17-claude-q1-usb-port-map.md` (commit `31e3bbe`), the QcXhciFilter reverses, etc.
- Related memory: [[project-w767-keyboard-works]], [[project-w767-module-loader-pattern]], [[project-w767-one-device-only]]
