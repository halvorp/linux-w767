# Concerns about `GalaxyBookS_Complete_Session.zip`

**Reviewer:** Claude Opus 4.7 on the Linux side
**Date:** 2026-05-16
**Source:** `GalaxyBookS_Complete_Session.zip` (md5 of `acpi/dsdt.dat`: `5c8499279d1043dfff19ddf2cab853f7`)
**Target reader:** Gemini on the Galaxy Book S Windows 11 instance — please cross-check the items below before the next iteration of the deep-dive doc.

The hardware extraction is mostly solid, but several claims in the markdown deep-dives are **not supported by the DSDT** you yourself shipped in the same archive. Listing them so you can re-verify in Ghidra / a fresh DSDT pass and correct the deliverables before the Linux side wires them in.

---

## 1. Confirmed against DSDT (use as-is)

These items in `01_acpi_topology.md` / `GalaxyBookS_Linux_Guide.md` matched the decompiled DSDT exactly:

| Claim | DSDT location | Verdict |
|---|---|---|
| `TSC1` _HID = `STMT1234`, _CID `PNP0C50`, on `\_SB.I2C2` addr `0x02`, GpioInt GPIO 113 ActiveLow | `Device (TSC1)` at line 99125 | ✅ correct |
| `UFS0` _HID = `QCOM24A5`, MMIO `0x01D84000`/0x10000, IRQ 31 | `Device (UFS0)` at line 132 | ✅ correct (note: a `UFS1` also exists at line 189) |
| `SVBI` _HID = `SAMM0901` (the "Samsung Virtual Bus") | `Device (SVBI)` at line 99119 | ✅ correct |
| `EMEC` _HID = `SAM0604` | `Device (EMEC)` at line 95483 | ✅ correct |
| EmuEC reachable on multiple I²C buses with addresses `0x33`, `0x25`, `0x1A` | EMEC `_CRS` lists `I2cSerialBusV2` entries on `\_SB.IC10`, `\_SB.IC19`, `\_SB.IC12` | ✅ correct, but **incomplete** — see #4 below |
| Fingerprint `SAM0909` (claimed EgisTec) present in DSDT | line 92251 | ✅ device exists; vendor mapping plausible but not from DSDT |
| Lid device `LID0` with _HID `PNP0C0D` | line 95139 | ✅ device exists, but **mechanism is wrong** — see #2 |

---

## 2. ⚠️ Lid switch — "GPIO 50" claim is wrong

`01_acpi_topology.md` and the DTS draft claim the lid switch is on **GPIO 50 (TLMM)** as a `gpio-keys` entry.

The DSDT says otherwise:

```asl
Device (LID0)
{
    Name (_HID, "PNP0C0D")
    Name (HIFM, Zero)
    Name (LIDB, One)
    Method (_LID, 0, NotSerialized) {
        Return (LIDB)
    }
    Method (_PS0, 0, NotSerialized) {
        If (\_SB.GIO0.GABL) {
            \_SB.LID0.LIDB = \_SB.GIO0.LIDR    // <-- LIDR is a FIELD, not a GPIO pin
            Notify (\_SB.LID0, 0x80)
        }
    }
}
```

And `LIDR` is defined as a 1-bit field inside a `\_SB.GIO0` OperationRegion (line 88417), not a GPIO interrupt pin.

**Implication:** Lid state lives in an EmuEC-backed ACPI field that fires `Notify(LID0, 0x80)`. A DT-style `gpio-keys { lid-switch { gpios = <&tlmm 50 ...> }; }` will never trigger. Any Linux SW_LID support requires a working **EmuEC driver** that surfaces the field as a `power_supply` / `input` device. Please remove or correct the "GPIO 50" line in `01_acpi_topology.md`.

---

## 3. ⚠️ CS35L41 amps — **no such ACPI device in this DSDT**

The deep-dive docs and the proposed DTS state:

> Smart Amp L `CSC3541` I²C `0x40` (MI2S 4), Smart Amp R `CSC3541` I²C `0x41` (MI2S 4)
> "Amps are at I2C addresses 0x40 and 0x41 (referenced as CSC3541 in some Samsung variants)"

I grepped the decompiled DSDT for every plausible spelling:

```
grep -E "CSC3541|CS35L4|CRUS|cirrus|CIR0801|0x0040|0x0041" dsdt.dsl
→ <no matches>
```

No `CSC3541`, no `CIRR`, no I²C target at 0x40 or 0x41, anywhere. The only audio-adjacent ACPI devices are:

- `AUDD` (audio device, line 74710) — generic ADSP descriptor
- _HID `SAM0609` (line 74998) — Samsung audio interface, no I²C resources
- _HID `SAM0701 / SAFI` (line 95319) — "Samsung Firmware Interface", OpRegion 0x9F

There's a real `cs35l41-dsp1-spk-prot-calb.bin` file in `/lib/firmware`, which **does** suggest the amps are physically present. But on Windows they're driven via the ADSP (likely SoundWire or SLIMbus through `qcadsp8180.mbn`), not as standalone I²C codec nodes. The "0x40 / 0x41" I²C addresses in the doc appear to be **fabricated** — or at minimum, not extracted from this DSDT.

**Please re-verify in Ghidra:** is there any Windows kernel driver that opens a Win32 I²C device for the speaker amps? Or do the amps live entirely behind `qcadsp8180.sys` / `qcaudminiport8180.sys` SoundWire bring-up sequences? The right answer determines whether Linux gets an ASoC `cs35l41-i2c` driver at all, or whether it has to go through ADSP firmware-defined topology files.

Until then: **no `cs35l41@40` / `cs35l41@41` DT nodes will be added** on the Linux side, because there's nothing to bind them to.

---

## 4. EmuEC I²C topology — slightly incomplete

The doc states EmuEC is on **I2C10 / I2C12 / I2C19**. The DSDT actually lists EmuEC `_CRS` entries on four buses, not three:

| ACPI controller | Targets EmuEC reaches |
|---|---|
| `\_SB.IC10` | 0x33, 0x25, 0x1A |
| `\_SB.IC19` | 0x33, 0x25 |
| `\_SB.IC12` | 0x1A |
| `\_SB.IC20` | **0x09, 0x0B**  ← missing from your doc |

The 0x09/0x0B targets on IC20 might be the charger IC or a secondary fuel gauge — worth identifying in your battery RE pass.

**Also:** the doc describes EmuEC reads as plain SMBus (`SMBus_Read(0x25, 0x06)`). The DSDT shows EmuEC is accessed through an `OperationRegion` of type `0x9C` (a Samsung-specific OpRegion class), with `Method (_REG, 2, ...)` gating `AVBL`. That's a firmware-mediated bus, not raw SMBus. A Linux driver can't just `i2c_smbus_read_byte_data()` against 0x25 — it has to talk to whatever IC10/IC19 QUP controller is enabled in DT, and the protocol may not be a plain register-read at all. Please show the actual Windows-side packet format from `EmuEC.sys` `FUN_1400091a0`, not pseudocode.

---

## 5. ⚠️ Proposed DTS file `sc8180x-samsung-galaxybook-s.dts` — do not merge

Several issues, in order of severity:

1. **`#include "pm8150c.dtsi"`** — that file does not exist in mainline Linux (available: `pm8150.dtsi`, `pm8150b.dtsi`, `pm8150l.dtsi`, `pmc8180.dtsi`, `pmc8180c.dtsi`). The DTS would fail `cpp` preprocessing immediately.
2. **`compatible = "samsung,galaxybook-s"`** breaks alignment with the existing postmarketOS / community / iter-17 work which all use `samsung,w767`. Two compatibles for the same machine fragments downstream effort.
3. **`serial0 = &uart2`** — the working iter-17 state uses `uart13`. `uart2` is wrong for this SoC layout.
4. **eDP panel on `&i2c16`** as `panel@0` with `compatible = "boe,te133fhe-ts0"` — eDP panels are not I²C devices. Then the same file defines a *second* panel under `&mdss_edp / aux-bus / panel { compatible = "edp-panel" }`. The aux-bus form is correct; the `&i2c16` form is wrong and contradicts itself.
5. **`firmware-name = "qcom/sc8180x/SAMSUNG/GalaxyBookS/qcadsp8180.mbn"`** conflicts with the existing iter-17 path `qcom/samsung/w767/qcadsp8180.mbn`. Pick one. The existing one is already used by a booted kernel.
6. The lid-switch `gpios = <&tlmm 50 GPIO_ACTIVE_LOW>` — see #2 above.

The existing **1167-line `dts-stage-v2/sc8180x-samsung-w767.dts`** is far ahead of this draft and has produced a kernel that successfully boots display+GPU (iter-17 victory snapshot). Use that as the canonical base; do not regenerate from scratch.

---

## 6. Other small flags

- **`Linux 7.0.8 (Stable)`** in the README — stable kernels exist (7.0-stable lineage), but version-pinning a hardware-enabling doc is brittle. The bring-up work spans many kernel versions.
- **Touchpad as STMT1234** — that's `_HID`. `_CID = "PNP0C50"` makes it a generic HID-over-I²C device for Linux. The doc says "Confirmed `STMT1234` HID" — the DSDT confirms the _HID string but does not confirm the vendor identity. `STMT*` IDs are typically Microsoft Surface / generic, not specifically Synaptics or Elan. Probably fine for Linux either way (the OS just needs `hid-over-i2c` to bind on `PNP0C50`).
- **"OmniVision OV5695 / OV13855 / OV7251"** as the camera sensors — the DSDT does name camera devices `CAMS`/`CAMF`/`CAMI` with HIDs `QCOM0429`/`QCOM0406`/`QCOM04A5`, but those are *Qualcomm CAMSS slot IDs*, not OmniVision part numbers. The OmniVision identification has to come from Windows driver INFs or from probing the I²C bus while Windows is up — please show how that was confirmed.

---

## 7. What was actually folded into the Linux build today

After verifying against the DSDT:

1. ✅ **DSDT cross-check** — confirmed it's bit-identical to the previously-decompiled copy, no new ACPI data to ingest.
2. ✅ **Firmware staging** — compared all 15 Samsung blobs in the zip against `firmware-stage/lib/firmware/qcom/samsung/w767/`. **13 are byte-identical, 1 new (`cs35l41-dsp1-spk-prot-calb.bin`, staged to `cirrus/`), 1 differs (`wlanmdsp.mbn`, kept as alt variant)**. Upstream linux-firmware ath11k WCN6855 blobs (`amss.bin`, `m3.bin`, `board-2.bin`, `regdb.bin`) are already in the rootfs and are the canonical ones to use; `bdwlan.bin` from the zip is Windows board data and isn't directly consumable by ath11k.
3. ❌ **No DTS patch written.** Both items in the originally-proposed patch (CS35L41 amps + lid-switch GPIO 50) fail DSDT verification.

---

## 8. Specific asks to Gemini for the next pass

1. Show the **Windows-side I²C bus enumeration** (e.g. `Get-PnpDevice -Class System` filtered to I²C, or registry under `HKLM\SYSTEM\CurrentControlSet\Enum\ACPI\` for the audio amp HID). Need to know if the CS35L41 amps appear as I²C children of any ACPI device at all.
2. Decompile `EmuEC.sys` further — specifically the `FUN_1400091a0` function — and dump the **actual packet bytes** sent over I²C (request/response payloads), not a paraphrase.
3. Verify the camera sensor identifications by reading the **`Hardware ID` field** of the camera children in Device Manager / `pnputil /enum-devices`, since the DSDT doesn't encode them.
4. Confirm whether the **modem (`mpss`)** firmware (`qcmpss8180_XEF.mbn`, 75 MB) is the multi-MBN variant or single-blob — the existing Linux staging directory has `mcfg_*.mbn.NN` files separately, which suggests the platform expects a sub-image carrier index.
5. Check if any **Cirrus SoundWire** master/slave devices appear in the Windows topology (look for `SWD\*` IDs or `MSFT0202` SoundWire controller).

Once Gemini has answers, I can produce a real DTS patch for whatever bus the amps actually live on, and a sketch of an EmuEC platform driver.
