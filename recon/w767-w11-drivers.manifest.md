# W767 Windows 11 ARM64 driver bundle — manifest

Archive: `recon/w767-w11-drivers.zip`
Archive SHA-256: `d948dec9b62904335855f69832675474146f93bceb1660e7e31dcab35d23beae`
Archive size:    50.8 MB compressed / 134.2 MB raw

Each entry is a complete copy of the active DriverStore directory at the time of capture (2026-05-18) on a W767 running Windows 11 ARM64 / BIOS P02AHP.003.241226. The version was selected by `LastWriteTime`; older installs at adjacent hash dirs are NOT included.

## Contents

| DriverStore subdir (`*.inf_arm64_<hash>`) | Role |
|---|---|
| `paneldriver.inf_arm64_7cd3a695b9ab839b` | Samsung PanelDriver (SAM0101 panel companion shim) |
| `emuec.inf_arm64_e75e3cf0be3c7ddc` | Samsung EmuEC (SAM0604 embedded controller; PMIC + keyboard scan MCU) |
| `secpmic3p.inf_arm64_839a8b4377736cd9` | Samsung Secondary PMIC 3rd-party (SCSS power-management) |
| `qcdx8180.inf_arm64_c1c5f5f4255a7d2a` | Qualcomm Adreno 680 GPU + DPU + Venus codec kernel driver |
| `qci2c8180.inf_arm64_191d0a2751394008` | Qualcomm GENI I2C controller |
| `qcwlan8180.inf_arm64_59a3e6c522523a44` | Qualcomm WCN3998 WLAN (ath10k_snoc-equivalent) |
| `qcbtfmuart8180.inf_arm64_ba0b068654fae2c1` | Qualcomm WCN3998 BT + FM over UART |
| `qcauddev8180_ss.inf_arm64_6080a9338434daca` | Qualcomm audio device (ADSP-fronted) |
| `qcaudminiport_ss.inf_arm64_df27788dfb77a0db` | Qualcomm audio miniport (codec) |
| `storufs.inf_arm64_0b7fc1ac1487e052` | Microsoft UFS storage (unused on W767 but present) |

## Per-binary SHA-256s

- `PanelDriver.sys` (      58,056 B) — `cdb7b15c5dfa79c70df0de3902021ddf7637b25544659ca263b2de2fe03b3739`
- `SecPmic3p.sys` (      55,176 B) — `640325cc9e94c37128e61f1918e5de6c1a4d512694870f1e3e2ee00774aa6b98`
- `EmuEC.sys` (     268,192 B) — `eefa88e079e9fec1bcb9ecbf3a939e54bf8abbbcd0df01177d3b9081301ad264`
- `qcaudminiport8180.sys` (     743,304 B) — `4592a0733c14e2a61cde05f22580c88b16ac3cb910e111f2cd8c5a6da6b4bfcd`
- `qcauddev8180.sys` (     686,472 B) — `be7a75d0cf4286cc829c306ad734930d313e0d3d53bc7373a71ae653d9819fca`
- `storufs.sys` (      80,768 B) — `6d8e7262b0373fd2e43d431c0b8ec06d97dca117c6b0dc6ac1009733e699ca69`
- `qcdxkm8180.sys` (   2,921,832 B) — `19a22e040a59c43096cd651522890d1c8cf3708f22c01ceb1cbdb51e2632ff59`
- `qcwlan8180.sys` (   2,606,800 B) — `eb25eb57584d1ab7f042513af92919ac7ef2fc45ee08cd482bfd003ba9c3a80b`
- `qcbtfmuart8180.sys` (     416,552 B) — `837087fdef3c03f1b76db9e8189cf4bc11e7774f8a7c2d541e0fcc2ab91a6b25`
- `qci2c8180.sys` (      77,280 B) — `ff28f2dfe49b0b490bba68083f71020c1c519fad91818177ef2907621d198e00`

## How to use (Linux side)

```sh
# Extract
unzip w767-w11-drivers.zip -d w767-w11-drivers/

# Load any one into Ghidra (project already auto-analyzed on Windows side; this is for
# a fresh import on the Linux build machine):
analyzeHeadless ./ghidra-w767 W767 -import w767-w11-drivers/paneldriver.inf_arm64_*/PanelDriver.sys
```

## What's NOT in this archive

- DSDT (already in `recon/dsdt-w767.aml`)
- Per-device EDID (already in `recon/edid-boe-07e7.bin`)
- DSP firmware MBN blobs — *those ARE in the archive* (inside qcdx8180 etc) but are duplicates of `firmware-stage-w767/`. Kept for completeness so the archive is self-contained.
- Other less-relevant drivers from Windows DriverStore (UCM, USB-C UCSI, ACPI helpers).

## Drivers NOT yet imported into the Ghidra project

Worth pulling next round if needed:
- `safidrv.sys` (Samsung Firmware Interface — for SAM0701 device, possibly does the actual panel GPIO sequencing)
- `qcdx11arm64xum8180.dll`, `qcdx12arm64xum8180.dll` (user-mode DXG companions to qcdxkm8180)
- `QcXhciFilter8180.sys` (the USB filter that asserts HSEI mod-1; iter-26 root-cause)
- `qcsubsys_ext_*.sys` (ADSP/CDSP/SLPI/MPSS/SCSS subsystem extensions — each one loads firmware)
