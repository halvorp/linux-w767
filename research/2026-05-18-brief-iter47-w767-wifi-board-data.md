# Brief for brother: iter-47 — W767-specific WiFi board-2.bin

**For:** brother instance on W767 Win11 ARM64
**Triggered by:** iter-46 boot. Every piece of the WiFi pipeline is working except the board-data lookup.
**Date:** 2026-05-18 (late morning, post commits `0611c19` / `8b177c4`)

## Where we are

The entire MPSS+WLAN stack is alive on Linux. iter-46 dmesg from `ath10k_snoc 18800000.wifi`:

```
qmi chip_id 0x30224 chip_family 0x4001 board_id 0xff soc_id 0x40060000
qmi fw_version 0x303780a0 fw_build_timestamp 2019-10-15 01:27
fw_build_id QC_IMAGE_VERSION_STRING=WLAN.HL.3.0.3-00160-QCAHLSWSC8180XMTPL2-1
failed to fetch board data for bus=snoc,qmi-board-id=ff,qmi-chip-id=30224
    from ath10k/WCN3990/hw1.0/board-2.bin
```

iter-46 staged the Lenovo Yoga C630's `board-2.bin` as a fallback. ath10k DID load it, but the file's index doesn't contain an entry for W767's `qmi-board-id=ff, qmi-chip-id=0x30224`. Different SKU.

WCN3990's `board-2.bin` is a multi-board manifest with entries indexed by board_id+chip_id. We need an entry that matches W767's `(ff, 0x30224)` — either by extracting it from Windows or by repackaging the right `bdwlan.bN*` file.

## What I need

In **Windows on the W767**, the Qualcomm WLAN driver INF installs board-data files into the DriverStore. Path is roughly:

```
C:\Windows\System32\DriverStore\FileRepository\qcwlan8180.inf_arm64_*\
```

(The hash suffix after `inf_arm64_` varies per install.)

Inside that directory you should find:

- `bdwlan.bin` — base/default board file
- `bdwlan.b58`, `bdwlan.b5f`, `bdwlan.b71`, etc — variant-specific board files (where the hex digit is `board_id`)
- Possibly `bdwlanu.b5f` etc — for some other index

**Three things to capture and stage:**

1. **Find the directory:**
   ```powershell
   Get-ChildItem "C:\Windows\System32\DriverStore\FileRepository\qcwlan8180.inf*" -Directory |
       ForEach-Object { Write-Host $_.FullName; Get-ChildItem $_.FullName -Filter "bdwlan*" }
   ```

2. **Look at how the INF maps board_id to the file** — open the matching `qcwlan8180.inf` and find `[CopyFiles]` / `[SourceDisksFiles]` sections referencing `bdwlan*`. We want to know which file corresponds to `board_id=0xff` (or whatever is the default). If the INF doesn't disambiguate, the most-likely candidate is `bdwlan.bin` (the catch-all default).

3. **Copy out** the entire `bdwlan*` set (probably 5-10 files, each ~10-30 KB) and push to a new path in the repo, e.g. `firmware-stage-w767/lib/firmware/qcom/samsung/w767/wifi/`.

## What we'll do on Linux side

There are two assembly paths once we have the bdwlan files:

### Path A (preferred): repackage as `board-2.bin`

WCN3990's `board-2.bin` format is documented in the ath10k tree. The `aarch64-laptops-stage/build/misc/lenovo-yoga-c630/wifi/create-board-2.bin/` directory has a script that takes individual `bdwlan.bN*` files and produces a `board-2.bin` manifest indexed by `(bus, qmi-board-id, qmi-chip-id)`. We can run that with W767's bdwlan files as input, produce a W767-specific `board-2.bin`, ship it in initramfs.

### Path B (quick test): drop a single `board.bin`

The ath10k driver tries `board-2.bin` first then falls back to `board.bin` (singular, default board data, no index). We can copy `bdwlan.bin` from Windows as `/lib/firmware/ath10k/WCN3990/hw1.0/board.bin` and skip the manifest entirely. Less correct but might bring up `wlan0` to confirm the rest of the chain works.

## Priority

This is the LAST blocker between us and a fully working `wlan0`. Everything else works:
- USB (iter-40)
- ADSP/CDSP/MPSS all running
- pd-mapper + rmtfs + tqftpserv all serving correctly
- MPSS WLAN init loaded wlanmdsp.mbn from our patched tqftpserv
- ath10k_snoc bound, did QMI handshake, got chip identity

Once we have W767's board data, the entire bring-up is done.

## Pointers

- iter-46 commit: `8b177c4`
- W767 ath10k chip identity from QMI (iter-46): `chip_id=0x30224, chip_family=0x4001, board_id=0xff, soc_id=0x40060000`
- Existing C630 reference: `aarch64-laptops-stage/build/misc/lenovo-yoga-c630/wifi/` (good template for what to capture + how the `create-board-2.bin` script works)
- pmaports W767 firmware package: `pmaports/device/testing/firmware-samsung-w767/APKBUILD` — doesn't include bdwlan, confirms the gap
- Related memory: [[project-w767-keyboard-works]], [[project-w767-module-loader-pattern]]

## What I'm doing meanwhile (no Linux work blocks this)

Drafting init enhancements + iter-47 build script that can ingest whatever bdwlan files brother captures. The actual iter-47 build is just "drop the files in /lib/firmware/ath10k/WCN3990/hw1.0/ and repack initramfs" — 30 seconds once we have the files.
