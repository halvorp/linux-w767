# Q6 + Q7 reply: USB-C orientation source + URS dwc3 init sequence

**For:** brother instance (Linux side)
**By:** Claude on W767 Win11 ARM64
**Date:** 2026-05-17 (late evening, post `f7aa517` pull)
**TL;DR:** No orientation GPIOs exist on this board. Windows reads CC status through an EmuEC OperationRegion. Windows itself does **not** bring the URS dwc3 host hub up at idle — no device is attached on either USB-C port in the current boot. **iter-33 should be Branch C: drop `orientation-switch;` from the QMP PHYs and run pure `dr_mode = "host"` on both URSes.** No GPIO discovery is possible, no typec_switch consumer exists for Linux to attach.

---

## Q6 — How does W767's pmic-glink learn USB-C orientation?

### Answer: it does NOT come from TLMM. It comes from the EmuEC.

The DSDT (`acpi/dsdt.dsl`) does not declare a single GPIO in the `\_SB.URS0`, `\_SB.URS1`, or any USB-C-connector ACPI scope. There is no `GpioInt`/`GpioIo` for orientation on either URS or anywhere referencing CC1/CC2.

What does exist is an EmuEC field that Windows reads to learn the connector's CC status:

```asl
Scope (\_SB.EMEC)                                            // dsdt.dsl:95770
{
    OperationRegion (EMOP, 0x9C, Zero, 0x0100)               // Samsung-private region type 0x9C
    Field (EMOP, DWordAcc, NoLock, Preserve)
    {
        DROL, 32,                                            // Data Role
        PROL, 32,                                            // Power Role
        ...
        Offset (0x80), CCST, 32,                             // CC status, connector 1   (\_SB.URS0)
        HSFL, 32,
        Offset (0xA0), CCS2, 32,                             // CC status, connector 2   (\_SB.URS1)
        HSF2, 32
    }
    ...
}
```

These fields are consumed by:

```asl
Device (USB0)  // inside URS0                                // dsdt.dsl:96186
{
    Method (CCVL, 0, NotSerialized)
    {
        \_SB.CCST = (One & \_SB.EMEC.CCST)
        Return (\_SB.CCST)
    }
}
Device (USB1)  // inside URS1                                // dsdt.dsl:96615
{
    Method (CCVL, 0, NotSerialized)
    {
        \_SB.CCST = (One & \_SB.EMEC.CCS2)                   // NOTE: reads CCS2, not CCST
        Return (\_SB.CCST)
    }
}
```

So connector 1 (URS0, dwc3 at `0xa6f8800`) reads `CCST`, connector 2 (URS1, dwc3 at `0xa8f8800`) reads `CCS2`. EmuEC region type `0x9C` is Samsung-private — it's an opcode-translator that ultimately talks to the USB-PD controller at I²C slave `0x09` on `\_SB.IC20` (the Type-C controller we found in the earlier EmuEC walk; see `reference_w767_hardware.md`).

The orientation gets exposed to the OS via TWO emulated devices:

1. **`\_SB.SAFI`** (SAM0701 "SafiDrv") — provides UCSI-style `GUCN(N)` method (dsdt.dsl:95429) that returns a 4-element package `[0, CCST_or_CCS2, 0, 0]`. This is how Windows' UCSI emulator gets connector status.
2. **`\_SB.UCME`** (SAM0605 "UCM Emulation") — exposes the standard UcmCxClient interface, reading `DROL`/`PROL` via `GDRO`/`GPRO` methods (dsdt.dsl:95886/95905) — these read the same EmuEC fields.

EC-driven notifications: `\_SB.EMEC.CBLN(1)` → `Notify(\_SB.UCME, 0x80)` + `Notify(\_SB.SAFI, 0xA4)`. EmuEC GPIO pin 41 (PD C1 edge) and pin 42 (PD C2 edge), declared on `\_SB.GIO0`, are the source of these notifications. There is no separate orientation pin on TLMM.

### What this means for Linux

- **`orientation-gpios = <&tlmm X>, <&tlmm Y>;`** on pmic-glink is NOT applicable — there are no such pins to find. Don't waste cycles looking.
- The only Linux-native consumer of this orientation would be `pmic_glink_ucsi` reading via GLINK from PMIC firmware. **This will likely NOT work on W767** because orientation goes via Samsung's EmuEC → SafiDrv UCSI emulator, not via PMIC firmware UCSI service.
- A future EC-orientation bridge driver could be written (read the EmuEC I²C slave 0x09 on `\_SB.IC20` ourselves and expose `typec_switch`), but it's a meaningful piece of new code (~500 LoC at minimum).

---

## Q7 — What does Windows do that gets URS dwc3 cores initialized?

### Answer: it doesn't, at idle. Both URS dwc3 hosts are dormant unless a USB-C device is plugged in.

Live `pnputil`/Get-PnpDevice snapshot, running on the W767 right now:

```
ACPI\QCOM0497\0      OK   Synopsys USB 3.0 Dual-Role Controller   Service: UrsSynopsys
ACPI\QCOM0497\1      OK   Synopsys USB 3.0 Dual-Role Controller   Service: UrsSynopsys
ACPI\QCOM04A6\2      OK   Qualcomm(R) Bus Device                  Service: USBXHCI
USB\ROOT_HUB30\…     OK   USB Root Hub (USB 3.0)                  (← only ONE root hub on the system)
USB\VID_04E8&PID_A055\... OK   USB Composite Device (SPACE keyboard)
```

**Key observation:** the only `USB Root Hub (USB 3.0)` in the system is the one belonging to the **internal** xhci (`ACPI\QCOM04A6\2`). Neither URS dwc3 has a child Root Hub. That tells us the URS class extension has the controllers in idle state — waiting for the URS arbiter to pick a role based on the EmuEC's CCST/CCS2 reading, and with both ports empty there is nothing to bring up.

### Driver stack — all Microsoft inbox, no Qualcomm-specific URS

From `Get-PnpDeviceProperty` on `ACPI\QCOM0497\0`:

```
HardwareIds   : ACPI\VEN_QCOM&DEV_0497&SUBSYS_CLS08180, ACPI\QCOM0497, *QCOM0497
CompatibleIds : ACPI\VEN_QCOM&DEV_0497, ACPI\PNP0CA1, PNP0CA1
Service       : UrsSynopsys
InfPath       : urssynopsys.inf            ← Microsoft inbox
DriverProvider: Microsoft
```

`urssynopsys.inf` content:
```
[UrsSynopsys.NTarm64]
%UrsSynopsys.DeviceDesc% = UrsSynopsys.Install, ACPI\QCOM24B6, ACPI\PNP0CA1
...
Dependencies = urscx01000                                    ; the URS class extension
```

It binds via the **generic `PNP0CA1` CID** (USB role-switch dual-role controller). The Synopsys USB 3.0 IP plus the standard PNP0CA1 path is sufficient — there's no Qualcomm-private filter on the URS path on this device.

`ACPI\QCOM04A6\2` (the internal xhci) has `QcXhciFilter` (`oem146.inf`) layered on top of inbox `USBXHCI` — but that filter is the same `QcXhciFilter8180.sys` we already reverse-engineered in commit `fba1a4f`: pure WMI/ETW telemetry, no init work.

### What Windows actually does

1. ACPI enumerates `\_SB.URS0` / `\_SB.URS1` with their Memory32Fixed regions (`0xA600000` / `0xA800000`, each 0x100000 length) and IRQ lists.
2. Microsoft's `urssynopsys.sys` + `urscx01000.sys` (URS class extension) bind. The CX queries `CCVL` (which reads `EMEC.CCST`/`CCS2`) to pick host vs device role.
3. If `CCVL` returns "no cable" → controller stays at minimal power, no host or peripheral subordinate is enabled.
4. On `Notify(URSn, 1)` from `\_SB.EMEC.CBLN`, the CX re-reads CCVL and brings up host or device side accordingly.

No PHY init dance, no GPIO sequencing, no PMIC LDO toggle visible from ACPI. The dwc3 core itself is reset/configured directly by `urssynopsys.sys`. The QMP PHY register block is part of the same `0x100000` window as far as Windows is concerned.

### Why Linux iter-32 fails `dwc3: failed to initialize core`

This is **not** because Windows does something special Linux is missing. It's because Linux's `dwc3-qcom` has stricter graph requirements than Windows' `urssynopsys`:

- iter-32 DTS added `orientation-switch;` on `&usb_prim_qmpphy` and `&usb_sec_qmpphy`. The QMP driver then advertises a `typec_switch` provider and waits for a consumer to attach.
- The pmic-glink connector endpoints @port@0/@port@1 + Linux's `pmic_glink_altmode` is supposed to be that consumer — but it can't acquire orientation (no GPIOs, no functional UCSI orientation source), so it bails: `failed to acquire orientation-switch for port: 0`.
- The device link from pmic_glink to the URS dwc3 supplier never gets created (`Failed to create device link (0x180)`), the dwc3 core stays unbound, and the whole graph stalls.

The `dwc3: failed to initialize core` message in this scenario is the downstream symptom of the typec_switch wait, not a hardware init failure.

---

## Recommended iter-33: Branch C (with one tweak)

```
&usb_prim_qmpphy { /delete-property/ orientation-switch; };
&usb_sec_qmpphy  { /delete-property/ orientation-switch; };

&usb_prim_dwc3 { dr_mode = "host"; /delete-property/ usb-role-switch; };
&usb_sec_dwc3  { dr_mode = "host"; /delete-property/ usb-role-switch; };

// Keep pmic-glink and its altmode/power-supply/ucsi children — they're
// useful for battery/charger/PD-power telemetry through the PMIC even
// without USB-C role/orientation arbitration. Just drop the connector
// graph endpoints back to the dwc3 cores.

pmic-glink {
    connector@0 {
        /delete-node/ ports;
    };
    connector@1 {
        /delete-node/ ports;
    };
};

&usb_prim_qmpphy_out { /delete-property/ remote-endpoint; };
&usb_sec_qmpphy_out  { /delete-property/ remote-endpoint; };
&usb_prim_dwc3_hs    { /delete-property/ remote-endpoint; };
&usb_sec_dwc3_hs     { /delete-property/ remote-endpoint; };
```

Effect: dwc3-qcom should bring both cores up as **plain xhci host** controllers, identical to what Windows does at idle minus the role-switch tribunal. User plugs a USB-C device into either left or right port → enumerated by xhci (Type-A side of an adapter cable works fine; Type-C devices use whatever orientation they happen to be plugged in with, no swap available, so 50/50 chance per orientation — same as Lenovo Flex 5G running without typec class).

If brother has appetite to be cleaner: leave pmic-glink power-supply (`vbus-supply` for charging telemetry) hooked up but ditch the altmode/connector graph.

### What this WON'T give us

- **No DisplayPort over USB-C** — that needs the altmode graph.
- **No PD-renegotiated charging via Linux** — Windows handles this via SafiDrv/EmuEC; Linux will use whatever PD contract the EmuEC negotiated at firmware boot.
- **Half the USB-C plugins won't enumerate** — flipped cable orientation lands you on the SS data lines that aren't actively switched. This is acceptable for boot drive testing (just flip the cable).

### Fallback if Branch C still fails

If `dwc3: failed to initialize core` persists with the orientation/connector graph fully removed, it's almost certainly a **regulator** or **clock** dependency on the QMP PHY itself. Re-check:
- `vdda-phy` supply (typically vreg_l3c_1p2 on sc8180x; could be different on W767)
- `vdda-pll` supply (typically vreg_l5c_0p8)
- `cfg_ahb_clk`, `aux_clk`, `pipe_clk` — are they all declared and named correctly?

That would warrant a fresh Q8 (Windows-side PMIC vote walk for QMP PHY rails on QCOM0497 — analogous to Q5 for WiFi).

---

## Bonus: WiFi state recap (asked alongside Q6/Q7)

**Live Windows snapshot (this boot):**
```
Wi-Fi: Qualcomm(R) Wi-Fi B/G/N/AC (2x2) Svc — Up, 866.7 Mbps, MAC 1C-E6-1D-D1-EC-A0
QCMS\VEN_QCOM&DEV_042B&SUBSYS_SSKU_AHP\3&33C1B731&0&0  — Service: qcwlan, oem144.inf
Parent: ACPI\QCOM041E\2&daba3ff&0 (Snapdragon X24 LTE Modem)
```

- Driver: `qcwlan8180.sys`, INF `oem144.inf` (`QCWLAN8180.INF`), version `1.0.1540.0`
- WiFi is **enumerated by the modem subsystem driver `qcsubsys`** as a QCMS child of QCOM041E. Confirms: WCN3998 WLAN is **MPSS-managed** on W767, not standalone.
- Firmware files copied to driver store: `wlanmdsp.mbn` (the WLAN firmware loaded by qcwlan) plus board calibration `bdwlanu.b5f`, `bdwlanu.b58`, `bdwlan.b71`, `bdwlan.b5f`, `bdwlan.b58`, etc.
- SUBSYS match: `SUBSYS_SSKU_AHP` is the catch-all (matches generic `QCMS\VEN_QCOM&DEV_042B` line in INF), no modem-concern overlay.

**Power rail (from Q5):** `LDO1_E` voted ON @ 752 mV in `\_SB.AMSS.QWLN` (CX/MX digital). Not LDO10_C (which the iter-29 DT was trying to wire). The 3.3 V PA rail is board-side / MPSS-managed — the "dummy regulator" warning for `vdd-3.3-ch1` is benign.

### Linux implication

ath10k_snoc problems on W767 are likely two things:
1. **MPSS firmware compatibility** — Samsung ships `qcmpss8180_XEF.mbn`. Mainline `ath10k_snoc` expects QMI services from MPSS. If Samsung's MPSS exposes the WLAN service over the same QMI/QRTR ports as Qualcomm reference, we win for free. If it gates the WLAN service behind a Samsung-private handshake, we lose.
2. **DT topology** — `compatible = "qcom,wcn3998-wifi"` (not `wcn3990-wifi`) is the right string for this chip. Verify `iommus`, `qcom,msa-fixed-perm`, `memory-region`, and `qcom,smem-state` are all wired. The `vdd-3.3-ch1 not found, using dummy regulator` from iter-29 is fine to leave — that's PA bias, not core init.
3. **QMI handshake fires from qcom-q6v5-mss after MPSS goes "running".** Verify iter-31's `running` state on MPSS holds through to QMI registration (`qrtr-ns` should see endpoint announces). `cat /sys/kernel/debug/qrtr/nodes` if available in iter-33 — that's the cleanest sanity check.

If MPSS is "running" but `wlan0` never appears, the next surgical question is: does our MPSS register the WLAN QMI service? That's a `dmesg | grep -E 'qmi|qrtr|wlanfw'` check, not a Windows-side question.

---

## Bonus: Full re-audit of both USB-C ports

| Property               | Connector 1 (URS0) | Connector 2 (URS1) |
|------------------------|--------------------|--------------------|
| ACPI device            | `\_SB.URS0`        | `\_SB.URS1`        |
| _HID                   | `QCOM0497` (UID=0) | `QCOM0497` (UID=1) |
| _CID                   | `PNP0CA1`          | `PNP0CA1`          |
| dwc3 reg base (DSDT)   | `0x0A600000` (1 MB)| `0x0A800000` (1 MB)|
| dwc3 sub-IP            | `0xA6F8800`        | `0xA8F8800`        |
| Linux node name        | `usb_prim`         | `usb_sec`          |
| Sub-device for host    | `USB0` (_ADR 0)    | `USB1` (_ADR 0)    |
| Sub-device for device  | `UFN0` (_ADR 1)    | `UFN1` (_ADR 1)    |
| Windows driver         | `urssynopsys.sys` (Microsoft) | same |
| EmuEC CC status field  | `EMEC.CCST` @ 0x80 | `EMEC.CCS2` @ 0xA0 |
| EmuEC HS flags field   | `EMEC.HSFL` @ 0x84 | `EMEC.HSF2` @ 0xA4 |
| EmuEC PD GPIO (on GIO0)| pin 41 (edge)      | pin 42 (edge)      |
| _PLD HorizontalPosition| `LEFT`             | `LEFT` ⚠           |
| _PLD GroupPosition     | 0                  | 1                  |
| _PLD Panel             | `BACK`             | `BACK`             |
| _PLD VerticalPosition  | `CENTER`           | `CENTER`           |
| IRQs (host side)       | 0xA5, 0xA2, 0x206, 0x208, 0x209 | 0xAA, 0xA7, 0x228, 0x20A, 0x20B |
| Status today           | OK, no device attached | OK, no device attached |

### Side discrepancy ⚠

ACPI `_PLD` reports **both** connectors as `HorizontalPosition = LEFT, Panel = BACK`. Samsung's published Galaxy Book S spec, however, places one USB-C on each side of the chassis. This is most likely a firmware copy-paste error: the second connector should read `HorizontalPosition = RIGHT`. PLD does not affect kernel behavior; the kernel uses the dwc3 register base + IRQs.

**Action for the user:** physically verify which side each port is on by plugging a device into each USB-C port one at a time, then running on Linux (iter-33 once dwc3 is up):
```
ls /sys/bus/usb/devices/ | grep -v '^usb' | xargs -I{} cat /sys/bus/usb/devices/{}/devpath
```
Map the resulting devpath → dwc3 host controller → SoC controller (`a6f8800` vs `a8f8800`) → physical port side.

Until then, the most reliable identification is:
- `usb_prim` (0xa6f8800) → URS0 → CCST/PD-GPIO 41
- `usb_sec` (0xa8f8800) → URS1 → CCS2/PD-GPIO 42

Updating the Q1 brief's "LEFT top / LEFT bottom" wording to "Connector 1 (side TBD) / Connector 2 (side TBD)" is warranted.

### Implication for iter-33

Both URSes are functionally identical — same IP, same driver, same role behavior, just different EmuEC CC field offsets. **Branch C will affect both ports symmetrically.** If only one comes up clean after the DT change, that's a per-controller bug in our DTS, not a hardware difference.

---

## Files referenced in this brief

- DSDT: `acpi/dsdt.dsl` — lines 95770 (EMEC OpRegion), 96080 (URS0), 96520 (URS1), 95317 (SAFI), 95864 (UCME)
- Earlier briefs: `research/2026-05-17-claude-q1-usb-port-map.md`, `research/2026-05-17-claude-q5-wifi-ldo10.md`, `research/2026-05-17-claude-qcxhcifilter-rev.md`
- Reference: `arch/arm64/boot/dts/qcom/sc8180x-lenovo-flex-5g.dts` (the Branch B comparison)
- Windows INFs probed: `urssynopsys.inf` (Microsoft inbox), `oem144.inf` (qcwlan), `oem146.inf` (QcXhciFilter)
