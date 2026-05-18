# Gemini RE pass: EmuEC + SecPmic3p + PanelDriver — for iter-67+ pickup

**Date:** 2026-05-18 (evening, after iter-66 wifi release)
**Source:** `/home/peter/Documents/GalaxyBookS_Linux/recon/Gemini_RE_docs/` (Gemini-generated Ghidra/strings/imports dump for the Samsung WOA driver triple)
**Purpose:** When iter-67+ pivots to battery + thermal + suspend, this is the entry point.

## EmuEC.sys → Linux `acpi_install_address_space_handler` driver (SAM0604)

| Fact | Value |
|---|---|
| Build path | `D:\Perforce\SDM\SC8180X\DEV\src\Drivers\Emuec\ARM64\Release\Emuec.pdb` |
| Compiled | Tue Feb 4 06:02:14 2020 |
| Size | 268 KB |
| ACPI HID | `ACPI\VEN_SAM&DEV_0604` (= `SAM0604` in DSDT) |
| Strings | `"EmuEC Driver is loaded"`, `"EmuEc turned on display"`, `TEMP` (x3) |

**Critical imports**: `RegisterOpRegionHandler` + `DeRegisterOpRegionHandler` → driver installs an ACPI OperationRegion handler. ACPI methods elsewhere read/write that region for battery, thermal, and display-power values.

**Also imports**: `ExGetFirmwareEnvironmentVariable` / `ExSetFirmwareEnvironmentVariable` → reads from / persists state to EFI variables.

Linux side: implement `acpi_install_address_space_handler()` for SAM0604, expose battery + thermal data via `power_supply` and `thermal_zone` frameworks. EFI variable interaction via `efivar_entry_get`. Ghidra dumps for FUN_140001b90, FUN_140004020, FUN_14001ffe0, FUN_1400200c0 in Gemini_RE_docs/ — start there.

## SecPmic3p.sys → Linux I²C regulator driver (SAM0606)

| Fact | Value |
|---|---|
| Build path | `D:\views\SC8180X\Dev\Src\drivers\SecPmic3p\ARM64\Release\SecPmic3p.pdb` |
| ACPI HID | `ACPI\VEN_SAM&DEV_0606` (= `SAM0606` in DSDT) |
| Bus | WDF, no SPB imports → standard I²C client (parent bus IC15 or similar per brother's recon/06-bus-map.md) |

Linux side: a tiny I²C driver + a regulator-fixed wrapper. EFI variable reads here too.

## PanelDriver.sys → already covered (SAM0101)

Brother RE'd this at iter-62 (`research/2026-05-18-claude-iter62-ghidra-paneldriver.md`). Gemini's findings confirm: WDF + userspace `PanelManagerSvc.exe`, uses `PoRegisterPowerSettingCallback` for power-state hooks, no direct hardware IO. Linux gpio-hog on TLMM pins 23/25/35 covers what this driver does on Windows (cold-boot panel enable).

## Recommended iter-67+ order

1. **Cold-boot panel + native timings polish** (gpio-hog + panel-edp.c BOE 0x07e7 entry) — small DTS + small kernel patch, "Partial" → "Works".
2. **DTS regulator wiring + MMCX power-domain** (brother's iter-62 fix-it §1, §2, §5) — eliminates dummy-regulator warnings on `&gpu`/`&edp_phy`.
3. **SAM0606 SecPmic3p Linux driver** — small, standalone, no dependencies. Wire it into DTS as an I²C child. Validate via regulator_get / consumer count.
4. **SAM0604 EmuEC Linux driver** — the big one. ACPI OperationRegion handler + power_supply/thermal_zone backends. Use Gemini's decompiled functions as the spec for which OpRegion offsets mean what. EFI variable reads also needed.
5. Suspend (S2idle) — only attempt once EmuEC is providing the suspend-state handshake Windows expects.

## Notes for whoever picks this up

- Gemini's decompiled function names are still `FUN_140000000`-style (no PDB symbols recovered). Cross-reference with the PDB path (`Emuec.pdb`) might be possible if a public PDB ever shows up, but unlikely.
- The Windows EC is reached via the `\_SB.AMSS` ACPI node per brother's earlier DSDT dossier — that's the ACPI namespace we need to honour in our DT.
- EmuEC's `TEMP` keyword is mentioned three times in `EmuEC_filtered_strings.md` — likely three thermal probe IDs (skin, CPU, battery?).
- Battery state is almost certainly polled via I²C from the EC, not memory-mapped. Linux driver will be I²C-attached child of one of the QUP I²C controllers — check brother's `recon/06-bus-map.md` for the exact IC<N>.

Sibling artefacts: `recon/Gemini_RE_docs/Samsung_WOA_Driver_Analysis.md` (overview), per-binary `*_info.md` / `*_imports.md` / `*_strings.md` / `*_filtered_strings.md`, plus the Ghidra C dumps `ghidra_EmuEC.sys_FUN_*.c`.
