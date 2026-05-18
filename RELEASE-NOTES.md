# linux-w767 — v1.0-iter66-wifi-up

**Tag:** `v1.0-iter66-wifi-up`
**Date:** 2026-05-18
**Hardware:** Samsung Galaxy Book S (SM-W767 / model variant ZNABTU) — Qualcomm Snapdragon 8cx Gen 1 (SC8180X), Adreno 680 GPU, BOE TE133FHE-TS0 13.3" eDP panel, WCN3990 WiFi, internal Samsung-companion keyboard/touchpad over I²C.

> **First working boot with display + GPU + WiFi simultaneously.** Sixty-seven iterations from the morning of 2026-05-18.

## What works

| Subsystem | Status |
|---|---|
| Boot (systemd-boot on EFI) | ✓ |
| Internal keyboard + touchpad (Samsung I²C-HID) | ✓ |
| USB host — three dwc3 controllers, six ports | ✓ |
| USB Mass Storage on external drive (the ESP itself) | ✓ |
| Adreno 680 GPU (`msm` + `a3xx_ops`) | ✓ |
| Display — eDP, 1920×1080, conservative VESA timings | ✓ Partial† |
| ADSP | ✓ running |
| CDSP | ✓ running |
| MPSS (modem) | ✓ running |
| **WiFi (WCN3990 / ath10k_snoc / wlan0)** | **✓** |
| Bluetooth | not yet (firmware staged but not exercised) |
| Audio (CS35L41 SPI) | not yet |
| Battery | not yet — managed by EmuEC, needs driver work |
| Suspend/Resume (S2idle) | not yet — known-flaky on SC8180X family |

† **"Partial"** in the postmarketOS-wiki sense — panel runs at conservative VESA DMT timings (the WARN_ON in `panel-edp.c` fires because BOE 0x07E7 isn't in the upstream lookup table). EDID + decoded timings are in the repo at `recon/edid-boe-07e7.bin` and `research/2026-05-18-claude-iter62-boe-edid-decoded.md` — adding the entry to `drivers/gpu/drm/panel/panel-edp.c` promotes this to "Works" but is left for a future release.

## Provenance

| Component | Source |
|---|---|
| Kernel base | `gitlab.com/sc8180x-mainline/linux` @ commit `27c30b32783351f6ca31c619cbf42697e7208f3d` (Linux 6.6) — same kernel postmarketOS pins for SC8180X devices |
| Kernel config | `pmaports/device/testing/linux-postmarketos-qcom-sc8180x/config-postmarketos-qcom-sc8180x.aarch64` verbatim, with two local Fedora-host build fixes (disable `SYSTEM_TRUSTED_KEYRING`, stub `certs/extract-cert.c` for OpenSSL 3) |
| Kernel patch | `0001-ath10k-wcn3990-set-fw-board.patch` (this release) — one-line add of `.fw.board = "board.bin"` to the WCN3990 hw_params so the 6.6 `api_1` NULL guard doesn't short-circuit |
| DTS | `dts/sc8180x-samsung-w767.dts` in this repo — based on upstream `sc8180x-samsung-w767.dts` with iter-17 (eDP reference clock), iter-21 (ramoops reserved-memory), iter-33 (pmic_glink simplification, no orientation-switch) |
| Initramfs | Custom busybox+Rust w767_init layout, with iter-62 (`modprobe rmtfs_mem`) and iter-63 (MPSS kick filter matches by firmware substring not `name=="modem"`) `/init` fixes |
| WCN3990 board-2.bin | Built (iter-66) from the 11 W767-specific `bdwlan.b<XX>` files extracted from the Windows DriverStore `qcwlan8180.inf_arm64_*`, using `qca/qca-swiss-army-knife/tools/scripts/ath10k/ath10k-bdencoder`. Contains the exact entry `bus=snoc,qmi-board-id=ff,qmi-chip-id=30224` ath10k requests for the W767. Source JSON: `firmware-stage-w767/lib/firmware/ath10k/WCN3990/hw1.0/board-2.json` |
| Firmware blobs | Brother's extract at `gitlab.com/jenneron/firmware-samsung-galaxy-book-s` (the .mbn .jsn files for MPSS/ADSP/CDSP plus zap-shader) |

## Release assets

All in the `release/` directory of this tag, plus attached to the GitHub release.

| File | Purpose | SHA-256 prefix |
|---|---|---|
| `Image` | ARM64 kernel binary (23 MB) | `ae252c45…` |
| `sc8180x-samsung-w767.dtb` | Device tree blob (82 KB) | `0fa20c83…` |
| `w767-initramfs.img` | gzip+cpio initramfs (62 MB) — contains all the modules, daemons, firmware, the busybox + the Rust `w767_init` PID 1 | `69cf4059…` |
| `ath10k-WCN3990-hw1.0-board-2.bin` | The W767-native ath10k board archive (238 KB) | `eb97c9ca…` |
| `ath10k_core.ko` | Patched module (the kernel binary above is from the same tree; this is here as a separate asset if you build your own kernel from source and want to drop in just this) | `48b42756…` |
| `loader-entry-w767.conf` | systemd-boot entry template | `3960cc95…` |
| `0001-ath10k-wcn3990-set-fw-board.patch` | The one-line kernel patch (if you build the kernel yourself) | `ce61683e…` |
| `SHA256SUMS` | All of the above |

## Flash and boot

Minimum: an ≥1 GB USB drive partitioned as a single GPT partition, formatted FAT32, with the `boot,esp` flags. **Replace `/dev/sdX` with your actual drive everywhere below — wrong device = data loss.**

```sh
# (host) partition + format
sudo parted /dev/sdX -- mklabel gpt
sudo parted /dev/sdX -- mkpart W767ESP fat32 1MiB 100%
sudo parted /dev/sdX -- set 1 boot on esp on
sudo mkfs.vfat -F32 -n W767ESP /dev/sdX1

# (host) mount + lay out
sudo mount /dev/sdX1 /mnt
sudo mkdir -p /mnt/EFI/BOOT /mnt/EFI/systemd /mnt/loader/entries
sudo cp Image                                  /mnt/Image
sudo cp sc8180x-samsung-w767.dtb               /mnt/sc8180x-samsung-w767.dtb
sudo cp w767-initramfs.img                     /mnt/w767-initramfs.img
sudo cp loader-entry-w767.conf                 /mnt/loader/entries/w767.conf

# systemd-boot itself — adjust path if your distro has it elsewhere
sudo cp /usr/lib/systemd/boot/efi/systemd-bootaa64.efi    /mnt/EFI/systemd/systemd-bootaa64.efi
sudo cp /usr/lib/systemd/boot/efi/systemd-bootaa64.efi    /mnt/EFI/BOOT/BOOTAA64.EFI

sudo umount /mnt
```

Plug into the W767, hold `F12` at power-on to get the boot menu, pick the USB drive.

## Building from source (kernel)

```sh
git clone https://gitlab.com/sc8180x-mainline/linux.git
cd linux
git checkout 27c30b32783351f6ca31c619cbf42697e7208f3d
patch -p1 < ../release/0001-ath10k-wcn3990-set-fw-board.patch
cp ../release/config-postmarketos-qcom-sc8180x.aarch64 .config
# (Note: if you're on a Fedora 43+ host with OpenSSL 3 and no openssl1.1
#  compat headers, also disable CONFIG_SYSTEM_TRUSTED_KEYRING in .config.)
make ARCH=arm64 CROSS_COMPILE=aarch64-linux-gnu- olddefconfig
make ARCH=arm64 CROSS_COMPILE=aarch64-linux-gnu- -j$(nproc) Image modules qcom/sc8180x-samsung-w767.dtb
```

The W767 DTS in this repo (`dts/sc8180x-samsung-w767.dts`) is the one that goes into `arch/arm64/boot/dts/qcom/` — it adds the iter-17 eDP reference clock, the iter-21 ramoops region, and the iter-33 pmic_glink simplification on top of the upstream-pmOS DTS.

## The 67-iter ladder

For the long-form context of how we got here, the relevant commits to read in order:

- `iter-49` — original USB+DSPs+WiFi working on Linux 7.0 with `DRM_MSM=m` (no display)
- `iter-50→55` — broke when display was added with `DRM_MSM=y` (silent hangs)
- `iter-56` — disabled display nodes in DTS (hung differently at arm-smmu)
- `iter-57` — also disabled adreno_smmu + gmu (GPU power island)
- `iter-58` — re-enabled dispcc (USB-C combo phys needed it; hit RCU stall)
- `iter-59` — disabled edp_phy (Kimi K2.6's cross-review hypothesis)
- `iter-60` — added hardlockup detector, modularized DRM_MSM
- **iter-61** — **pivot to postmarketOS-pinned 6.6 kernel** (KIMI_2 diagnosed the 6.8-rc1 DRM regression). Display + GPU bound.
- `iter-62` — `modprobe rmtfs_mem` in `/init` so MPSS QMI/RFSA works
- `iter-63` — fixed MPSS kick filter (matched by firmware substring, not `name=="modem"`)
- `iter-64` — staged Yoga C630 board-2.bin (didn't match W767 QMI triplet, but useful diagnostic)
- `iter-65` — ath10k 6.6 hw_params `.fw.board` patch
- **`iter-66`** — **built W767-native board-2.bin** matching the exact `bus=snoc,qmi-board-id=ff,qmi-chip-id=30224` string. WiFi up.

KIMI_0..3 + the brother briefs in `research/` carry the technical reasoning at each step.

## Credits

- **Peter (the human driving this)** — 67 iterations of plug-in / boot / read-the-screen / unplug discipline.
- **Brother instance** (Claude running on the W767 Win11 side) — Windows DSDT extracts, PEP-vote map, Ghidra pass on `PanelDriver.sys`, the BOE 0x07E7 EDID, the 11 W767-specific `bdwlan.b<XX>` files from the Windows DriverStore, and the iter-62 fix-it report from log analysis.
- **Kimi K2.6** (MoonshotAI, via Kilo Code) — three cross-reviews (KIMI_0..3) that produced the iter-58 wedge diagnosis, the iter-61 pmOS pivot diagnosis, and the iter-62 rmtfs_mem fix-it.
- **postmarketOS** — `linux-postmarketos-qcom-sc8180x` package and the SC8180X kernel pin that made this whole release possible.
- **jhovold / sc8180x-mainline gitlab** — the 6.6 SC8180X kernel branch.
- **jenneron** — the staged firmware-samsung-galaxy-book-s blob bundle.

## Known issues / next priorities

(In rough order of leverage)

1. Wire `vdda-supply` (`vreg_l3c_1p2`) and `vddcx-supply` (`vreg_l9e_0p88`) on `&gpu` in DTS — eliminates dummy regulator warnings (brother's iter-62 fix-it §1)
2. Wire `vdda-phy-supply` / `vdda-pll-supply` on `&edp_phy` — saves runtime power (§2)
3. Add `power-domains = <&rpmhpd SC8180X_MMCX>` to `&gpu` and `&mdss` — runtime PM for display+GPU (§5)
4. Add BOE TE133FHE-TS0 entry to `drivers/gpu/drm/panel/panel-edp.c` — promotes display "Partial" → "Works", uses native 147.9 MHz pixel clock (brother's iter-62 EDID decode)
5. Drop `wireless-regdb-2024.10.07/regulatory.db` + `.p7s` into initramfs `/lib/firmware/` — fixes `cfg80211: malformed regulatory.db` warning
6. SAM0101 panel companion gpio-hog (cold-boot panel power, §6)
7. Bluetooth (`hci_qca` UART) bring-up — firmware already staged at `recon/.../bt/`
8. Audio (CS35L41 SPI) — needs a Samsung-specific ASoC machine driver
9. EmuEC battery/AC — needs an `\_SB.AMSS` ACPI OperationRegion handler
10. Suspend/resume — known-flaky on SC8180X family in mainline
