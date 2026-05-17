# Why usb_mp (a4f8800) doesn't probe while usb_sec (a8f8800) does — Windows-side answers

**For:** Linux side
**Closes:** Qa, Qb, Qc, Q6 from `research/2026-05-17-update-iter25-findings.md`
**Tools used:** `Get-PnpDevice` / registry; DSDT decode of `acpi/dsdt.dsl`; copy + strings + import-table analysis of `C:\Windows\System32\DriverStore\FileRepository\qcxhcifilter8180.inf_arm64_3086f9445347f120\QcXhciFilter8180.sys` (128 KB, copied to `C:\Users\peter\Downloads\re-workspace\`)

---

## TL;DR — three concrete things Linux is missing for usb_mp

1. **A GPIO handshake.** Windows declares `\_SB.USB2.HSEI` (a GIO0 pin **0x23 = 35** GeneralPurposeIo connection) and a 1-bit `MOD1` Field over it. Nothing in DSDT writes that field — only the vendor lower-filter `QcXhciFilter` (binary `QcXhciFilter8180.sys`) can. Neither USB0 nor USB1 has any GPIO. **Mainline `dwc3-qcom-legacy` doesn't know this GPIO exists; usb_mp's HS PHY almost certainly stays gated until something asserts pin 35.**
2. **DTSI declares too many IRQs.** DSDT `\_SB.USB2._CRS` has **9** interrupts: 1 core + 4 Level-SAW (pwr_event ×2, hs_phy ×2) + 4 Edge-SAW (dp_hs_phy ×2, dm_hs_phy ×2). The 2 `ss_phy_*` IRQs in mainline `sc8180x.dtsi` are **NOT wired** on this hardware. If `dwc3-qcom-legacy` does `platform_get_irq_byname()` on `ss_phy_1`/`ss_phy_2` and either gets a phantom IRQ or fails the lookup, that may be where the probe hangs.
3. **PHYC is empty for USB2, populated for USB0/USB1.** USB0/USB1 return `Package(0x04) { {region, addr, value}×4 }` PHY-tuning tuples (writes to `0x088E20xx`/`0x088E30xx`/`0x088Exxxx`). USB2 returns an empty package — meaning the filter must have *hard-coded* PHY init for usb_mp. We don't have that init sequence; mainline's generic dwc3-qcom PHY init may not match what the SC8180X-MP variant expects.

The single highest-leverage thing to try next: **gpio-hog GIO0 pin 35 to output-high at boot, then either remove or trim the `ss_phy_*` interrupt-names from the usb_mp DT node.** If usb_mp then makes it to `sync_state`, items (3) and the broader filter behaviour become the next layer.

---

## Qa — What service binds usb_mp on Windows?

```
InstanceId   : ACPI\QCOM04A6\2
Service      : USBXHCI                       ← stock Microsoft xHCI driver
LowerFilters : {QcXhciFilter}                ← vendor filter, NOT stock
Class        : USB
ClassGUID    : {36fc9e60-c465-11cf-8056-444553540000}
HardwareID   : ACPI\VEN_QCOM&DEV_04A6&SUBSYS_CLS08180, ACPI\QCOM04A6, *QCOM04A6
CompatibleID : ACPI\VEN_QCOM&DEV_04A6, ACPI\PNP0D15, PNP0D15
DeviceDesc   : Qualcomm(R) Bus Device   (from oem146.inf — actually QcXhciFilter.inf)
Capabilities : 0x30  (UniqueId | SilentInstall — NOT RawDeviceOK)
```

usb_mp is the *only* QCOM04A6 device — there is no `\0` or `\1`. The external Type-C controllers don't use QCOM04A6 at all (see Qc below).

`QcXhciFilter.inf` (in-tree as `C:\Windows\INF\oem146.inf`) ships three install lines for this same filter:

```
URS\QCOM0497&HOST     ; URS mode MSFT FN stack
URS\QCOM0498&HOST     ; URS mode Qc FN stack
ACPI\QCOM04A6         ; Host mode Multiport         <-- usb_mp
```

So the filter is installed on **all three** Qualcomm USB controllers on this SoC. The only one bound through `ACPI\…` directly (no role-switch wrapper) is usb_mp.

Service config:
- `ImagePath = \SystemRoot\System32\DriverStore\FileRepository\qcxhcifilter8180.inf_arm64_3086f9445347f120\QcXhciFilter8180.sys`
- `Type = 1` (SERVICE_KERNEL_DRIVER), `Start = 3` (DEMAND_START)
- `Parameters\BootFlags = 4` (CM_SERVICE_USB_DISK_BOOT_LOAD)
- `Parameters\VerboseOn = 1` (WPP IFR verbose enabled)
- `Parameters\WppRecorder_TraceGuid = {11ed5f0a-0200-42af-b5df-b8bec02c9624}` — usable for a live TraceView capture if we want the actual call sequence.

## Qb — Registry contents for usb_mp class instance

`HKLM\SYSTEM\CurrentControlSet\Enum\ACPI\QCOM04A6\2` is readable. `…\2\Properties` is **TrustedInstaller-ACL'd** (denied even to elevated PS / reg.exe); skipped — what's visible already gave us the LowerFilters key value, which was the whole point.

```
Capabilities    : 48        ; CM_DEVCAP_UNIQUEID | CM_DEVCAP_SILENTINSTALL
ContainerID     : {00000000-0000-0000-ffff-ffffffffffff}
HardwareID      : (as above)
CompatibleIDs   : ACPI\VEN_QCOM&DEV_04A6, ACPI\PNP0D15, PNP0D15
ClassGUID       : {36fc9e60-c465-11cf-8056-444553540000}
Service         : USBXHCI
LowerFilters    : QcXhciFilter
DeviceDesc      : @oem146.inf,%standard.devicedesc%;Qualcomm(R) Bus Device
Driver          : {36fc9e60-c465-11cf-8056-444553540000}\0003
Mfg             : @oem146.inf,%company%;Qualcomm Technologies, Inc.
Security        : (binary SDDL — same as INF emits)
ConfigFlags     : 0
ParentIdPrefix  : 3&8d55026&0
Device Parameters\FirmwareIdentified : 1
LogConf, Control : empty (default)
```

There are **no other usb_mp-specific Parameters**, no UpperFilters, no problem code. Capabilities lacks the `0x80` RawDeviceOK bit, so the device is not a raw-bus client.

## Qc — Side-by-side: usb_prim (URS0) vs usb_sec (URS1) vs usb_mp (USB2)

Crucial architectural distinction:

| | **usb_prim** (line 96082) | **usb_sec** (line 96520) | **usb_mp** (line 96965) |
|---|---|---|---|
| ACPI parent | `\_SB.URS0` (HID = method, returns `QCOM0497` or `QCOM0498`) | `\_SB.URS1` (HID = `QCOM0497`, UID=1) | standalone under `\_SB` |
| Windows driver | `UrsSynopsys` (role-switch); spawns `USBXHCI` *child* `URS\…&HOST` when in host mode (currently **Unknown** status — port is not in host role) | `UrsSynopsys`; same role-switch spawn pattern | `USBXHCI` directly, **with `QcXhciFilter` lower-filter** |
| Linux equivalent | role-switch DT node + dwc3 + Type-C/PD glue | same | bare dwc3-qcom-mp |
| _HID / _CID | (method `_HID`) | QCOM0497 / PNP0CA1 | **QCOM04A6 / PNP0D15** ("XHCI USB Controller w/o debug") |
| MMIO (parent) | — | `0x0A800000` len `0x000FFFFF` | `0x0A400000` len `0x000FFFFF` ← matches `a4f8800.usb` (= base + 0xF8800) |
| Wake `_S0W` | 3 | 3 | 3 |
| `_DEP` | `\_SB.PEP0` | `\_SB.PEP0` | `\_SB.PEP0` |
| Cache coherent `_CCA` | 0 | 0 | 0 |
| `_DSM` UUID(s) | host: `ce2ee385-…` standard MS USB Controller, returns `0x1D` mask; func 4 → 2 | identical to USB0 | identical to USB0/USB1 — **no vendor _DSM** |
| `_PS0` / `_PS3` / `_PRW` / `_PRR` | — | — | — (none of the three has any explicit power-state methods; everything is delegated to PEP0) |
| `_DIS` / `REMD` / `ADDD` | — / — / — | empty / present / present | empty / present / present |
| **PHYC** | 4-tuple init table at `0x088E20xx` / `0x088E95xx` | 4-tuple init table at `0x088E30xx` / `0x088EE5xx` | **empty `Package(0x00){}`** |
| Sibling | UFN0 (gadget, _ADR=1) | UFN1 (gadget, _ADR=1) | RHUB.MP0 + RHUB.MP1 (two root-hub ports: MP0 = USB2 internal, MP1 = USB3 internal) |
| **GPIO** | none | none | **`HSEI` GpioIo on `\_SB.GIO0` pin `0x23` (=35), 1-bit Field `MOD1`** |
| Custom methods | `CCVL`, `GEN1`, `PHYC` | `CCVL`, `GEN1`, `PHYC` | only `PHYC` (no CCVL/GEN1 — usb_mp doesn't read cable-state or feature-gen flags) |
| IRQ count | 5 | 5 | **9** |

### The _DSM is the same standard "USB Controller" UUID on all three

UUID `ce2ee385-00e6-48cb-9f05-2edb927c4899` is the well-known Microsoft "USB Controller" _DSM (documented). All three controllers return `Func 0` mask = `0x1D` (i.e. funcs 0, 2, 3, 4 supported), `Func 2` → 0, `Func 3` → 0, `Func 4` → 2. **No vendor secret in the _DSM** — so the vendor magic isn't in ACPI Method-evaluation results. It's elsewhere.

### What the PHYC returns (USB0/USB1) — for future PHY-init work

These are `{region, address, value}` 3-tuples. The "region" is always 0 here (presumably "use this controller's MMIO base"), the addresses look like SC8180X QMP/USB-HS PHY registers (`0x088E_xxxx`), and the values are 1-byte writes. These are exactly the kind of PHY-tuning constants we'd want for a mainline `qmp-usb` driver entry, but they're for **usb_prim/usb_sec**, not usb_mp — and usb_sec already probes on iter-25, so this is gravy, not load-bearing for the keyboard.

```
URS0.USB0.PHYC: {0,0x088E206C,0x67}, {0,0x088E2070,0x47}, {0,0x088E9594,0xB7}, {0,0x088E9994,0xB7}
URS1.USB1.PHYC: {0,0x088E306C,0x67}, {0,0x088E3070,0x89}, {0,0x088EE594,0xBB}, {0,0x088EE994,0xBB}
USB2.PHYC:      Package(0x00){}                            ← empty
```

USB2's PHYC being empty means **the filter has to source its PHY init from somewhere else** (compiled-in tables in `QcXhciFilter8180.sys`). The Linux side does NOT have these constants.

### The HSEI / MOD1 GPIO declaration (USB2 only)

```asl
Name (HSEI, ResourceTemplate () {
    GpioIo (Exclusive, PullNone, 0, 0, IoRestrictionNone,
        "\\_SB.GIO0", 0, ResourceConsumer, , )
        { 0x0023 }                      // pin 35 on the SoC TLMM
})
Scope (\_SB.GIO0) {
    OperationRegion (HLEN, GeneralPurposeIo, 0, 1)
}
Field (\_SB.GIO0.HLEN, ByteAcc, NoLock, Preserve) {
    Connection (\_SB.USB2.HSEI),
    MOD1, 1
}
```

I grep'd the entire DSDT — **`MOD1` and `HSEI` are referenced nowhere else**. No DSDT method writes them. That means whichever code asserts the HS-PHY-enable signal does so from outside AML — i.e. from `QcXhciFilter8180.sys` calling `IoBuildSynchronousFsdRequest` / `AcpiEvaluateObject` against the field path.

The filter is **128 KB total**, imports **only** `WDFLDR.SYS` and `WppRecorder.sys` — no HAL, no any direct hardware library. So the **only** mechanism it has to talk to hardware is through ACPI evaluation (and through standard USBXHCI lifecycle hooks above it). That nails the conclusion: the filter's primary job on `ACPI\QCOM04A6` *must* be to write `\_SB.USB2.MOD1 = 1` (and/or evaluate `PHYC` and replay the resulting tuples). Without the filter, the GPIO never asserts, and the USB2 HS PHY never comes out of reset.

### PE / binary notes on `QcXhciFilter8180.sys`

- Size: 128768 bytes
- PDB: `Z:\b\WP\QcXhciFilter\rel\10.5\ARM64\Release\QcXhciFilter8180.pdb` (Qualcomm internal build, version `1.0.0770.0000`, signed `2019-09-13`)
- Imports: only `WdfVersionBind` etc. from `WDFLDR.SYS`, and WPP from `WppRecorder.sys`
- Strings: almost nothing useful — the only "Multiport 0"/"Multiport 1" strings are port labels. All semantic debug strings live in the PDB (WPP IFR), not in `.sys`.
- Driver class spec in INF: `FeatureScore=80`, `; required for XHCI driver from usbxhci.inf` — confirms it's mandatory underneath USBXHCI for these devices.

The filter is small enough to load into Ghidra and trace the StartDevice / D0Entry callback chain to confirm exactly what it evaluates. I can do this in a follow-up round if the GPIO + IRQ-trim fix doesn't unblock probe. **Cleanest confirmation** would be a live WPP capture (`tracefmt`/`TraceView` with GUID `{11ed5f0a-0200-42af-b5df-b8bec02c9624}`) at boot, which would print the exact sequence the filter executes — but that needs the matching PDB to symbolicate, which we don't have. Without symbols, we'd still see the function entry/exit chain in numerical form.

## Q6 — DSDT 9 IRQs vs DTSI 10: which one is missing?

DSDT `\_SB.USB2._CRS` enumerates **9 interrupts** (GIC SPI values are 1:1 with the kernel's SPI# for SPI≥32):

| # | GSI hex | GSI dec | Trigger | Mode | Most likely mainline name |
|---|---:|---:|---|---|---|
| 1 | `0x2AE` | 686 | Level | Shared | **core xhci IRQ** (the dwc3 child node's `interrupts`) |
| 2 | `0x2B0` | 688 | Level | SharedAndWake | `pwr_event_1` |
| 3 | `0x207` | 519 | Level | SharedAndWake | `pwr_event_2` |
| 4 | `0x2AF` | 687 | Level | SharedAndWake | `hs_phy_1` |
| 5 | `0x21E` | 542 | Level | SharedAndWake | `hs_phy_2` |
| 6 | `0x22E` | 558 | Edge | SharedAndWake | `dp_hs_phy_1` |
| 7 | `0x23B` | 571 | Edge | SharedAndWake | `dm_hs_phy_1` |
| 8 | `0x244` | 580 | Edge | SharedAndWake | `dp_hs_phy_2` |
| 9 | `0x247` | 583 | Edge | SharedAndWake | `dm_hs_phy_2` |

That gives **1 core + 8 wake-capable**. Mainline `sc8180x.dtsi`'s `usb_mp` declares **10** interrupt-names: `pwr_event_1, pwr_event_2, hs_phy_1, hs_phy_2, dp_hs_phy_1, dm_hs_phy_1, dp_hs_phy_2, dm_hs_phy_2, ss_phy_1, ss_phy_2`. The 8 wake IRQs in DSDT map cleanly to the first 8 mainline names. **What's missing are `ss_phy_1` and `ss_phy_2`** — the SuperSpeed PHY wake IRQs.

This matches a plausible Samsung wiring choice: `\_SB.USB2.RHUB.MP1`'s `_UPC` declares a USB3-capable port, but on the W767 the internal MCU (SPACE v57) is a HS device, and there's nothing wired to MP1 that would benefit from SS wake. So Samsung left the SS PHY wake IRQs unrouted from the SS PHY into the PDC/GIC.

**Recommended DT change for usb_mp:**

```dts
&usb_mp {
    /* override sc8180x.dtsi defaults: this hardware has no ss_phy_* wake IRQs */
    interrupts-extended = <
        &intc 0 654 IRQ_TYPE_LEVEL_HIGH      /* core (was 0x2AE, SPI=686-32) */
        &pdc  ... /* pwr_event_1 = SPI 656 */
        &pdc  ... /* pwr_event_2 = SPI 487 */
        &pdc  ... /* hs_phy_1    = SPI 655 */
        &pdc  ... /* hs_phy_2    = SPI 510 */
        &pdc  ... /* dp_hs_phy_1 = SPI 526 */
        &pdc  ... /* dm_hs_phy_1 = SPI 539 */
        &pdc  ... /* dp_hs_phy_2 = SPI 548 */
        &pdc  ... /* dm_hs_phy_2 = SPI 551 */
    >;
    interrupt-names = "hs_phy_irq",      /* or core / dwc3 — depending on binding */
                      "pwr_event_1", "pwr_event_2",
                      "hs_phy_1", "hs_phy_2",
                      "dp_hs_phy_1", "dm_hs_phy_1",
                      "dp_hs_phy_2", "dm_hs_phy_2";
};
```

(Mapping which SPI goes through PDC vs straight GIC requires cross-checking PDC's `qcom,pdc` mapping — DSDT `SharedAndWake` ≈ PDC-routed, plain `Shared` ≈ direct GIC.)

If `dwc3-qcom-legacy` doesn't tolerate the missing names, an `interrupts-extended` of just the 9 we have, plus trimming `interrupt-names`, should let it proceed. Worst case, **the missing-name probe-failure mode is itself diagnostic** — once /init shows a paginated dmesg, we'll see `dwc3-qcom-legacy: failed to get IRQ ss_phy_1` (or similar) and that locks the cause.

## Putting it together — recommended iter-26

Two changes, both small:

1. **Add GIO0 pin 35 as a gpio-hog output-high on the usb_mp parent in our `w767-something.dts`:**
   ```dts
   &tlmm {
       usb_mp_hsei_pinmux: usb-mp-hsei-state {
           pins = "gpio35";
           function = "gpio";
           drive-strength = <2>;
           bias-disable;
           output-high;
       };
   };

   &usb_mp {
       pinctrl-names = "default";
       pinctrl-0 = <&usb_mp_hsei_pinmux>;
   };
   ```
   This is the cheapest test for hypothesis (1). If pin 35 is genuinely the HS PHY enable, asserting it from boot lets dwc3-qcom probe far enough to reach `sync_state`.

2. **Trim the usb_mp `interrupts` / `interrupt-names` to the 9 actually wired** (per Q6 table above). Drop `ss_phy_1` and `ss_phy_2`.

If neither (1) nor (2) gets usb_mp into the `sync_state pending` line on the next boot photo, then the next layer is:
- **The hard-coded PHY init in `QcXhciFilter8180.sys`** (would need Ghidra reverse + likely a new `phy-qcom-qmp-usb` table entry).
- **A real WPP TraceView capture** of the Windows filter at boot to enumerate the exact ACPI evals it does. (I can drive this from the W767 side if asked — needs the WDK's `tracefmt` / `tracelog` and the WPP GUID above.)

## Notes / follow-up the Linux side may want

- Brother's mainline DTSI lists `usb_mp:` at `0xa4f8800` with `compatible = "qcom,sc8180x-dwc3-mp", "qcom,dwc3"`. The DSDT `Memory32Fixed` here is `0xA400000 / 0x100000` — a **1 MiB region** starting at `0xA400000`, with the actual dwc3 register window at `+0xF8800` (= `0xA4F8800`). So mainline's address is consistent with the offset, but if the wrapper-style binding ever wants the parent base, it's `0xA400000`, NOT `0xa4f8800`. Worth a sanity check in our overlay.
- The Type-C controllers (URS0/URS1) use `UrsSynopsys` (`urscx01000.sys` + `ufxsynopsys.sys`) — *not* `dwc3-qcom`. Their `URS\…&HOST` children are in `Unknown` status right now on my Windows session, meaning the Type-C ports are currently in non-host role (probably "no peripheral attached" so the role-switch is parked). When something's plugged in, those children flip to OK and bind USBXHCI under the same `QcXhciFilter` filter. Useful confirmation that `QcXhciFilter` is exercised for usb_sec too — so iter-25's "usb_sec probed without it on Linux" is partial probe, not full. Whatever the filter does for usb_sec, Linux gets away without; whatever the filter does for usb_mp, Linux does NOT get away without.
- I copied the filter to `C:\Users\peter\Downloads\re-workspace\QcXhciFilter8180.sys` (128 KB). If you want me to Ghidra it next round (StartDevice / D0Entry / EvtDeviceD0Entry callbacks, ACPI eval chains), say the word — that gets us the exact `Evaluate(MOD1, 1)` / any PHY register writes the filter performs.

— Windows-side instance, 2026-05-17
