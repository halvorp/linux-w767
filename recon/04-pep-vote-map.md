# 04 — PEP Vote Map (PMIC regulator votes per device per power state)

**Source:** `acpi/dsdt.dsl`, `\_SB.PEP0.APCC` package walk.
**Extraction:** `_extract-pep.ps1` → `_raw-pep-votes.tsv` (342 rows).
**Format:** flat tuples `(device, component, fstate, rail, ?, microvolt_or_mode, microamp_or_param, …)`.

Each PEP record names a SoC subsystem (or peripheral) and lists, for each fstate (`0` = D0/active, higher numbers = retention/off), which PMIC rails it pulls and at what voltage. Windows hands these to its Platform Extension Plugin (`PEP`) which arbitrates them into PMIC SPMI writes.

## TL;DR — rail map per device

| Device | Rails voted (D0) | Notes |
|---|---|---|
| **`\_SB.URS0.USB0`** (USB-C connector 1 host) | LDO12_A, LDO16_E, LDO3_C, LDO5_E, LDO9_E | Identical set for URS0.UFN0 (device-mode), URS1.USB1, URS1.UFN1 |
| **`\_SB.URS1.USB1`** (USB-C connector 2 host) | LDO12_A, LDO16_E, LDO3_C, LDO5_E, LDO9_E | Identical to URS0 |
| **`\_SB.USB2`** (internal-only xhci, KB MCU) | LDO12_A, LDO16_E, LDO3_C, LDO5_E, LDO9_E | Same as URS — all three USB controllers share PHY rails |
| `\_SB.GPU0` (Adreno 680) | LDO3_C, LDO5_E, LDO9_E, LDO16_E | Shares LDO3_C/LDO5_E with USB + PCI |
| `\_SB.PCI0..PCI3` (PCIe roots) | LDO3_C, LDO5_E (PCI1 also SMPS8_C) | Inactive on W767 (no PCIe consumer wired) |
| `\_SB.UFS0` (primary eUFS) | LDO15_E, LDO2_C, LDO3_C, LDO5_E, LDO9_C | LDO9_C is UFS-specific |
| `\_SB.UFS1` (secondary, unpopulated) | LDO10_E, LDO3_C, LDO5_E, LDO6_A, LDO7_E | Not present on W767 |
| `\_SB.SDC2` (µSD slot) | LDO17_E, LDO6_C | Inactive on W767 (no µSD slot exposed; binding-only) |
| `\_SB.ADSP.SLM1.ADCM.AUDD` (Aqstic codec via SLIMbus) | LDO14_E, CXO_BUFFERS_BBCLK2_A | LDO14_E is audio analog |
| `\_SB.AMSS.QWLN` (WiFi via MPSS) | **LDO1_E** only | **Confirms Q5** — WiFi has ONE OS-managed rail |
| `\_SB.COEX` (WiFi+BT coexistence) | LDO1_E | Same rail as QWLN |
| `\_SB.BTH0` (Bluetooth UART) | LDO11_C, LDO7_A, LDO9_A | LDO7_A + LDO9_A are BT-specific |
| `\_SB.IPA` (cellular IPA accelerator) | (extracted but unused on W767) | Inactive |
| `\_SB.TSC1` (Trust/Secure?) | LDO12_E, LDO4_C | |
| `\_SB.ALS1` (Rohm BH1733 ambient light) | LDO7_C, LDO8_C | Sensor I/O rails |
| `\_SB.SAR1..SAR4` (Semtech SX9360 SAR) | LDO10_A | All four sensors share LDO10_A |
| `\_SB.CAMF, CAMS, CAMI, CAMP` (cameras) | LDO1_A, LDO2_A, LDO10_A, LDO14_A, LDO16_A, LDO17_A | Camera dropouts; not yet relevant for Linux |

**Devices with NO PEP record in this DSDT:** Cirrus CS35L41 amps (SPI-attached, not power-managed by PEP), MOST `SAM*` Samsung-specific devices (EmuEC handles their power), USB hub roots, panel driver, sensors via SAMM IDs.

## Rail voltage decode (D0 working state)

These are the values seen in D0 (active) PEP records for each rail. Format from each `Package(0x06)` is approximately `(name, mode=1, microvolt, microamp, mode_high, mode_low)`. Values are **what Windows asks the PMIC for**; the actual SPMI write may be at a slightly different setpoint.

| Rail | D0 voltage | Hex µV | Hex µA | Common consumers | Likely Linux name |
|---|---|---|---|---|---|
| **LDO12_A** | **1.8 V** | `0x001B7740` | — | USB PHY 1.8V, cameras (some) | `vreg_l12a_1p8` |
| **LDO16_E** | **3.072 V** | `0x002EE000` | — | USB VBUS PHY, GPU 3.0V | `vreg_l16e_3p0` |
| **LDO3_C** | **1.2 V** | `0x00124F80` | — | USB/PCI/GPU/UFS PLL+PHY core | `vreg_l3c_1p2` |
| **LDO5_E** | **0.88 V** | `0x000D6D80` | — | USB/PCI/GPU/UFS digital CX/MX | `vreg_l5e_0p88` |
| **LDO9_E** | **0.912 V** | `0x000DEA80` | — | USB PHY analog (newer PEP entries) | `vreg_l9e_0p912` |
| LDO14_E | (binary on/off for AUDD) | — | — | Audio codec | `vreg_l14e_*` |
| LDO1_E | 0.752 V | `0x000B7980` | (peak in Q5) | WiFi + COEX (CX/MX digital) | `vreg_l1e_0p8` |
| LDO15_E | (UFS) | — | — | UFS PHY | `vreg_l15e_*` |
| LDO2_C | (UFS) | — | — | UFS VCC | `vreg_l2c_*` |
| LDO9_C | (UFS) | — | — | UFS VCCQ | `vreg_l9c_*` |

## Action items for iter-33 (the actionable bottom line)

### For Branch C (no orientation switch) → if dwc3 still fails to init

Add explicit supply wiring to the QMP USB PHYs in `dts/sc8180x-samsung-w767.dts`:

```
&usb_prim_qmpphy {
    vdda-phy-supply = <&vreg_l9e_0p912>;     /* LDO9_E in DSDT */
    vdda-pll-supply = <&vreg_l3c_1p2>;       /* LDO3_C */
};
&usb_prim_hsphy {
    vdda-pll-supply = <&vreg_l5e_0p88>;      /* LDO5_E — CX/MX */
    vdda33-supply   = <&vreg_l16e_3p0>;      /* LDO16_E */
    vdda18-supply   = <&vreg_l12a_1p8>;      /* LDO12_A */
};
/* And symmetric on usb_sec_qmpphy / usb_sec_hsphy. */
```

(The exact `vreg_*` phandle names must match `sc8180x-pmics.dtsi`. Cross-check there.)

If the QMP PHY supplies were already declared in mainline `sc8180x.dtsi`, Linux's regulator framework will have auto-binding without the W767 DTS overriding them. If iter-32 dmesg shows `using dummy regulator` for any of these rails, then they're NOT yet wired and you need to add them.

### For the internal USB2 xhci

`\_SB.USB2` shares the SAME 5 rails as URS0/URS1. So the QMP/HS PHY supplies on `&usb_mp_qmpphy` and `&usb_mp_hsphy` should reuse the same vreg phandles. If iter-28 keyboard works without those, they're probably already correct in mainline DTSI — but verify by grepping for "using dummy regulator" in iter-32 dmesg.

### Cross-check against mainline

Mainline `arch/arm64/boot/dts/qcom/sc8180x.dtsi` already declares supplies for `usb_prim_qmpphy` / `usb_prim_hsphy` / etc. as **placeholders** that boards must override. Lenovo Flex 5G's `sc8180x-lenovo-flex-5g.dts` shows the canonical naming. Our DTS should match those phandle names.

## Caveats

- The PEP TSV captures every PMICVREGVOTE package but interprets fields by position; some `Package(0x03)` (3-field) entries don't have microvolt/microamp values and show truncated rows. Not all 342 rows are voltage records — many are mode-only switches.
- The "duplicate" entries (URS0.USB0 appearing at lines 42880 AND 47359) are because PEP0 has multiple `APCC` sections covering different P-states. The second section (47359+) is the more complete one with `LDO9_E` present.
- Component/FState columns in the TSV are often blank because the PMICVREGVOTE sits inside an FSTATE block that didn't preserve through my parser's state machine. The flat (device → rails) mapping is still correct; just don't trust the Component/FState columns without re-checking line numbers in DSDT.

## Cross-references

- `_raw-pep-votes.tsv` — the underlying TSV; awk-friendly.
- `_extract-pep.ps1` — the parser. Rerun if DSDT changes.
- DSDT `\_SB.PEP0.APCC` starts at line `3200` (first APCC section). Second section at `46700+`. Third at `51000+`. They overlap in coverage; pick the most complete entry per device.
- See [08-linux-gap.md](08-linux-gap.md) for the actionable DTS patch list derived from this map.
