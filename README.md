# linux-w767

Linux kernel bring-up work for the **Samsung Galaxy Book S (SM-W767)** —
Qualcomm Snapdragon SC8180X / 8cx Gen 1.

Codename: `samsung-w767`.

## Status

| Subsystem | State | Notes |
|---|---|---|
| Boot, eDP display (1920×1080), Adreno 680 GPU | ✅ Working | iter-17 baseline on Fedora 41 aarch64 with the device tree from this repo |
| UFS storage, USB host | ✅ Working | mainline drivers |
| Touchpad (`hid-over-i2c`) | 🟡 DTS ready, unverified | iter-19 places it on `&i2c1` @ addr 0x02 with GPIO 113 IRQ |
| SPI buses for CS35L41 audio amps | 🟡 Buses enabled, no codec child nodes yet | `&spi0` and `&spi3` corresponding to ACPI `\_SB.SPI1` (0x00880000) and `\_SB.SPI4` (0x0088C000) |
| WiFi (ath11k WCN6855) | 🟡 Firmware staged, untested probe | Use upstream `linux-firmware` ath11k WCN6855 set |
| Battery telemetry | ❌ Not wired | Needs `pmic-glink` + `qcom_battmgr` DT nodes + working ADSP firmware loading |
| Audio output (CS35L41 amps over SPI, SLIMbus data) | ❌ Not wired | Needs custom ASoC machine driver — no upstream pattern for SC8180X+CS35L41 |
| Internal keyboard (`SAMM0901` SVBI) | ❌ Blocked | No upstream driver — needs custom EmuEC platform driver. Use external USB keyboard for now. |
| Lid switch, Fn keys | ❌ Blocked | Same as keyboard — gated on EmuEC driver |
| Suspend / resume | ❌ Untested | Even sister-chip SC8280XP (X13s) has flaky S2idle on mainline |
| Fingerprint reader (EgisTec) | ❌ Unsupported | No `libfprint` driver |
| Cameras | ❌ Unwired | `qcom-camss` + DT needed; sensor identities (OV13855/OV5695/OV7251) still RE-inferred |

See [`docs/00-hardware-combined.md`](docs/00-hardware-combined.md) for the full multi-source synthesis with each claim's provenance.

## Repository layout

```
.
├── dts/                          Canonical W767 device tree source
├── kernel-patches/               Patch series against Linux 7.0 mainline
├── acpi/                         Extracted DSDT (binary + decompiled ASL)
├── docs/                         Hardware reference + multi-source synthesis
├── research/                     RE iteration history (Gemini + Claude reviews)
├── w767-os/                      Build infrastructure
│   ├── kernel/                     allnoconfig + W767 merge fragments + build script
│   ├── initramfs/                  Minimal busybox initramfs builder
│   ├── rust/                       w767_init / ctl / netd userspace (musl-static aarch64)
│   ├── scripts/                    deploy + iterate helpers
│   ├── grub/                       BLS entry templates
│   ├── kmods/                      out-of-tree kernel modules (hid-samsung-w767, w767_audit)
│   ├── dts/                        DTS staging copies
│   └── audit/                      hardware audit utilities
├── deploy/                       USB-image builder + iter-19 deploy kit
└── windows-extracts/             Windows-side text dumps from the running W767
```

## Building

The kernel build expects an upstream Linux 7.0 tree as a sibling directory.
The build script applies the W767 device tree from this repo into it and
produces an `Image` + DTB.

```bash
# 1. Clone mainline Linux 7.0 next to this repo
git clone --depth 1 --branch v7.0 https://git.kernel.org/pub/scm/linux/kernel/git/torvalds/linux.git ../linux

# 2. Apply the W767 device tree patch
( cd ../linux && git am ../linux-w767/kernel-patches/*.patch )

# 3. Build kernel + DTB + minimal initramfs + bootable USB image
cd w767-os
./kernel/build-kernel.sh --target w767-initramfs
./initramfs/build-initramfs.sh --fetch-userspace        # downloads busybox + dropbear from Alpine apks
../deploy/build-usb-image.sh --image /tmp/w767-usb.img

# 4. Flash to a USB stick (DESTRUCTIVE — verify the device first)
sudo dd if=/tmp/w767-usb.img of=/dev/sdX bs=4M status=progress conv=fsync
```

The `--target w767-initramfs` build produces a self-contained kernel +
initramfs that boots to a busybox shell on the GBS via systemd-boot.

## Firmware

This repo does **not** ship proprietary Samsung / Qualcomm / Cirrus firmware
blobs.  For a complete deployment you need:

| Path | Source |
|---|---|
| `/lib/firmware/qcom/samsung/w767/*.mbn` | [`jenneron/firmware-samsung-galaxy-book-s`](https://gitlab.com/jenneron/firmware-samsung-galaxy-book-s) on GitLab — extract `qcadsp8180.mbn`, `qccdsp8180.mbn`, `qcdxkmsuc8180.mbn`, `qcmpss8180_XEF.mbn`, `qcslpi8180.mbn`, `wlanmdsp.mbn` plus the `.jsn` PD-mapper descriptors |
| `/lib/firmware/qcom/a680_{gmu.bin,sqe.fw}` | Same jenneron tree, or from a Windows GBS install via `c:\Windows\System32\DriverStore` |
| `/lib/firmware/ath11k/WCN6855/hw2.0/{amss.bin,board-2.bin,m3.bin,regdb.bin}` | Upstream `linux-firmware` |
| `/lib/firmware/cirrus/cs35l41-dsp1-spk-prot*.{wmfw,bin}` | Cirrus driver firmware (extract from Windows install — vendor-distributed) |

`docs/00-hardware-combined.md` §5 has the full firmware layout with sources
for each blob.

## Hardware references

- **postmarketOS:** [`device/testing/device-samsung-w767`](https://gitlab.com/postmarketOS/pmaports/-/tree/master/device/testing/device-samsung-w767) — community port (testing tier)
- **Mainline / archived community kernel:** [`sc8180x-mainline/linux`](https://gitlab.com/sc8180x-mainline/linux) — frozen Jan 2023 but still the kernel pmOS pins
- **jhovold's X13s wiki:** [`jhovold/linux X13s`](https://github.com/jhovold/linux/wiki/X13s) — authoritative SC8180X / SC8280XP family kernel cmdline + bring-up quirks
- **aarch64-laptops:** [`aarch64-laptops/build`](https://github.com/aarch64-laptops/build) — covers Lenovo Flex 5G (closest SC8180X cousin)

## Reverse-engineering history

The `research/` directory captures an 8-round iterative reverse-engineering
cycle between Gemini (running on Windows 11 ARM64 on the W767 itself) and
Claude (cross-checking against DSDT and mainline source on a separate host).
Notable conclusions:

1. The amp chip is **CS35L41**, not CS35L40 — `CS35L41_CHIP_ID = 0x35a40` per
   `include/sound/cs35l41.h:747`; Windows just named its driver `CS35L40_*`
   as an internal codename.
2. EmuEC (`SAM0604`) owns keyboard scancodes, lid state, thermals, and AC-presence
   notifications.  Actual battery SOC/voltage/current telemetry goes through
   the **`pmic-glink` + `qcom_battmgr` RPMSG path** via the ADSP (not direct I²C
   from EmuEC).  This was missed in initial RE rounds — corrected in
   `docs/00-hardware-combined.md`.
3. ACPI controller suffix (`I2C2`, `IC19`, `SPI1`, `SPI4`) does **not** map
   1:1 to DT phandle suffix.  Always go via `Memory32Fixed` base ↔
   `sc8180x.dtsi` node base.
4. The `space_pahp.cap` camera tuning file does not exist (Gemini-hallucinated
   across multiple rounds; flagged and removed).

## License

GPL-2.0-only.  See [`LICENSE`](LICENSE).

The device tree source (`dts/sc8180x-samsung-w767.dts`) is dual-licensed
BSD-3-Clause / GPL-2.0 per its SPDX header, consistent with the Linux kernel
device tree convention.

## Contributing

Patches and bug reports welcome via GitHub issues / pull requests.
The two highest-value contribution areas right now are:

1. Verifying the iter-19 touchpad probe on real hardware (boot the USB
   image, dump `dmesg | grep -iE 'hid|touchpad|i2c-1'`)
2. A `samsung,w767-ec` platform driver for the SAMM0901 keyboard — unlocks
   internal-keyboard usability and the rest of EmuEC functionality (lid,
   Fn keys, AC events)

## Author

Peter Koczka — `mg.peter.koczka@gmail.com`
