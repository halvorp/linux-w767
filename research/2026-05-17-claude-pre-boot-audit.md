# 2026-05-17 — Pre-boot audit: blockers, risks, and verified-good areas

**Author:** Claude (Opus 4.7) on the W767 itself, Win11 ARM64 side
**Method:** read-only review of DTS, kernel configs (`base-arm64.config`,
`w767.config`, `w767-initramfs.config`), GRUB/BLS templates,
`build-kernel.sh` / `build-initramfs.sh` / `build-usb-image.sh` /
`deploy-kernel.sh` / `install-bls-entry.sh`, against the working state
captured in `docs/iter-17-boot-snapshot.txt`.
**Purpose:** identify everything that would block or risk the next boot
attempt on the Linux build host, before the brother-instance builds.

## TL;DR

One concrete code blocker (`build-kernel.sh:29` pointed at a non-existent
directory — **fixed in companion commit alongside this doc**), three
"need-to-stage-something" blockers, one hardware risk to watch on first
boot, and a long list of things that are verified correct. The cmdline
freeze fixes (`a58bd66`) and touchpad address fix (`07233d6`) cover the
biggest "won't boot at all" and "touchpad won't probe" risks.

## Severity-graded blockers

| # | Severity | File | Issue | Resolution |
|---|----------|------|-------|------------|
| **B1** | 🔴 BLOCKER | `w767-os/kernel/build-kernel.sh:29` | `DTS_SRC` hard-coded to `$REPO_ROOT/dts-stage-v2/` — directory doesn't exist in this repo (leftover from old layout). Every `build-kernel.sh` invocation aborts at the "board DTS not found" check before any compilation. | **Fixed in this commit series** — DTS_SRC now points to `$REPO_ROOT/dts/sc8180x-samsung-w767.dts`. |
| **B2** | 🔴 BLOCKER | `w767-os/initramfs/build-initramfs.sh:26,153` | Firmware staging path `$REPO_ROOT/firmware-stage/lib/firmware/` doesn't exist (correctly — repo doesn't ship redistributables). Build aborts at "firmware staging missing" check. | Stage firmware before running. See `BUILDING.md` §"Firmware staging" for the path and minimum set. |
| **B3** | 🟠 HIGH | `dts/sc8180x-samsung-w767.dts:452,742,817,824` | DTS firmware-name paths all `qcom/samsung/w767/*.mbn`. Recon-staged firmware is at `recon/lib-firmware/qcom/sc8180x/` (different intermediate directory). Without re-staging at the DTS-expected path, every DSP firmware load fails. | Re-stage at `firmware-stage/lib/firmware/qcom/samsung/w767/`. |
| **B4** | 🟠 HIGH | `dts/sc8180x-samsung-w767.dts:289-295` | `vreg_l4c_3v3` (touchpad 3.3V analog supply) is commented out with note: "ldo4 is needed for touch, but causes failed to get current voltage -ENOTRECOVERABLE". Touchpad only declares `vddl-supply` (1.8V logic). | Empirical: first boot, test `i2cdetect -y 1`. If 0x49 silent, uncomment L4C and add `vio-supply` to the touchpad node. |
| **B5** | 🟠 HIGH | `w767-os/kernel/build-kernel.sh:60-65` | Hard-requires `../linux` as a sibling directory containing Linux v7.0 source. Build aborts at "not a valid Linux source tree" check. | Pre-clone Linux v7.0 next to the repo. Documented in `BUILDING.md` §"Prerequisites". |
| **B6** | 🟡 MED | `deploy/build-usb-image.sh:34` | `SD_BOOT` defaults to `/tmp/sd-boot-aa64/usr/lib/systemd/boot/efi/systemd-bootaa64.efi` — assumes manually-extracted RPM. No fetch step in any script. | Document in `BUILDING.md` §"Prerequisites". RPM extraction recipe added. |
| **B7** | 🟡 MED | `w767-os/scripts/install-bls-entry.sh:24-28` | Expects `/root/w767-os/grub/w767-phase{1,2}.conf` on the device. Requires manual `rsync` of the `w767-os/` tree to the device's `/root/` first. | Document the rsync step in BUILDING.md (and/or have install-bls-entry.sh tarball+pull from the dev host). |

## DTS audit: verified-correct items

Cross-referenced against the iter-17 working snapshot
(`docs/iter-17-boot-snapshot.txt`) and the recon-decoded DSDT.

| DT element | Lines | Status |
|------------|-------|--------|
| `compatible = "samsung,w767", "qcom,sc8180x"` | 15 | ✓ |
| Reserved-memory: rmtfs / wlan / mpss / adsp / cdsp / scss + gpu_mem override | 46-85, 1189-1191 | ✓ no overlaps; iter-18 fix relocates gpu_mem to `0x9b400000/0x5000` outside DSP regions |
| `&dispcc`, `&mdss`, `&mdss_edp` enabled with aux-bus + BOE TE133FHE-TS0 panel | 443-444, 628-721 | ✓ identical to iter-17 working state |
| `edp_ref_clk` 19.2MHz fixed-clock workaround + wiring into `&edp_phy` | 30-35, 680-685 | ✓ iter-17 fix, validated working |
| Touchpad (`touchpad@49`, reg=0x49, hid-descr-addr=0xab, IRQ GPIO 113) | 471-482 | ✓ DSDT-canonical post commit `07233d6` |
| `&remoteproc_adsp/cdsp/mpss` enabled with memory-region + firmware-name | 739-825 | ✓ structurally correct |
| `&ufs_mem_hc` with reset-gpios + vcc/vccq2 | 1043-1053 | ✓ |
| `&usb_prim` / `&usb_sec` / `&usb_mp` host-mode | 1088-1148 | ✓ |
| `&wifi` block (ath10k_snoc, WCN3998-family) | 1150-1160 | ✓ matches `CONFIG_ATH10K_SNOC=m`; **DTS was already correct on WCN3998 vs the README's WCN6855 claim** |
| `&uart13` with `qcom,wcn3998-bt` + supplies | 1026-1041 | ✓ matches the WCN3998 confirmation from `research/2026-05-17-claude-recon-emuec-chip-id.md` |
| Pinctrl groups for all enabled QUP I²C buses, uart13, edp_hpd, pcie3 | 873-1023 | ✓ |
| SPMI PMICs `@2`/`@a` disabled | 827-834 | ✓ correct for this SKU |

## DTS audit: lesser issues worth knowing about (not blockers)

| # | File:line | Issue | Impact |
|---|-----------|-------|--------|
| **D1** | DTS:289-295 | LDO4C disabled (see B4) | Touchpad may not enumerate |
| **D2** | DTS:149-150, 156-157, 213-214, 228-234 | Multiple regulators have "failed to set initial mode -ETIMEDOUT" commented-out modes (ldo1, ldo2, ldo14 in pmc8180-a; ldo17) | Non-fatal; upstream RPMh-regulator bugs |
| **D3** | DTS:873-880, 471-482 | Touchpad pinctrl named `touchscreen_active` selects gpio123 input-enable, but the DSDT only references gpio113 as IRQ. gpio123 is possibly a power-down or reset line carried over from the earlier `touchscreen@49` placeholder. | Input-enable is non-destructive; verify gpio123 role on first boot |
| **D4** | DTS:87-106, 745-811 | pmic-glink declared at root with two usb-c-connectors, but no port topology — the `remoteproc_adsp_glink` block bridging it to ADSP is commented out, and `mdss_dp0/dp1` are also commented out | USB-C dwc3 host-mode works; Alt-Mode DisplayPort over USB-C will not be advertised. Future work. |
| **D5** | DTS:979-1002, no `&pcie3` reference | `pcie3_default_state` pinctrl declared but PCIe never enabled | Dead pinctrl block; harmless |
| **D6** | DTS:21 | `chosen { }` empty — no `stdout-path` | Without a serial console exposed, early-boot diagnostics rely entirely on the framebuffer (which simpledrm/efifb preserves). Acceptable. |

## Kernel config audit (mature; iter-19/iter-20 fixes already applied)

### Phase 2 `w767-initramfs.config` (the most exposed for first boot)

**Critical builtins present:**

| CONFIG | Why it matters | Verified at |
|--------|---------------|-------------|
| `PM=y`, `PM_SLEEP=y`, `SUSPEND=y` | Without these, `DRM_MSM` silently drops via olddefconfig (iter-19 fix) | lines 22-28, comment explicit |
| `SYSFB=y`, `DRM_SIMPLEDRM=y`, `FB_EFI=y`, `PRINTK_TIME=y` | Preserves UEFI GOP framebuffer across `ExitBootServices()` so screen isn't black during DRM_MSM probe (iter-20 fix) | lines 30-44, comment explicit |
| `DRM_MSM=y`, `DRM_PANEL_EDP=y`, `DRM_DP_AUX_BUS=y` | Display stack builtin — no module-load race | 134-136 |
| `SCSI_UFS_QCOM=y`, `USB_DWC3_QCOM=y` | Storage + USB builtin | 99-107 |
| `QCOM_Q6V5_PAS=y`, `QCOM_RPROC_COMMON=y`, `RPMSG_QCOM_GLINK_SMEM=y` | Remoteproc + glink builtin (needed for DSPs to load) | 84-94 |

**Builtin/module decisions worth knowing:**

| CONFIG | Phase 2 | Note |
|--------|---------|------|
| `ATH10K_SNOC` | `=m` | WiFi as module. Won't auto-load on Phase 2 initramfs (no udev). If WiFi needed in Phase 2, flip to `=y`. |
| `I2C_HID`, `I2C_HID_OF` | `=m` (lines 203-204) | Touchpad as module. Phase 2 initramfs has no udev — touchpad won't probe. **Recommendation:** flip both to `=y` for early bring-up. |
| `BACKLIGHT_CLASS_DEVICE` | `=y` (line 140) | OK. `dp_aux_backlight` from panel-edp registers under this class. iter-17 proved working without `BACKLIGHT_PWM`. |

### `base-arm64.config` is solid

- `EFI_STUB=y`, `EFI_ZBOOT=y` — direct UEFI boot from systemd-boot
- `DEBUG_INFO_DWARF5=y`, `KALLSYMS_ALL=y` — full debug info
- `MAGIC_SYSRQ=y` — `sysrq-c` for emergency dump
- `PSTORE_RAM=y`, `PSTORE_CONSOLE=y`, `PSTORE_PMSG=y` — but no `ramoops` reserved-memory in DTS, so it won't bind (warning only)
- `# CONFIG_SECURITY_SELINUX is not set` — sensible for bring-up

## Boot args / freeze prevention (post-`a58bd66`)

All three independent sources agree:

| Source | Has all 6 quirks? |
|--------|-------------------|
| `w767-os/grub/w767-phase1.conf` (post `a58bd66`) | ✓ |
| `w767-os/grub/w767-phase2.conf` (post `a58bd66`) | ✓ |
| `deploy/build-usb-image.sh:150-152` (inline BLS template) | ✓ — was already correct pre-session |

**Quirk inventory** (each addresses a specific SC8180X failure mode):

| Flag | Failure prevented |
|------|------------------|
| `clk_ignore_unused` | Qcom clock framework misidentifies in-use SoC blocks as unused, gates them off |
| `pd_ignore_unused` | Same for power domains |
| `arm64.nopauth` | Cortex-A76 ARMv8.3 pointer-auth + SC8180X firmware crash interaction |
| `efi=noruntime` | Stock Samsung UEFI hangs on certain runtime services calls |
| `iommu.passthrough=0`, `iommu.strict=0` | SMMU settings needed for stable UFS/USB/PCIe DMA |
| `pcie_aspm.policy=powersupersave` | Link-state policy for ath11k (Phase 1 only) |

Concerns to track:
- `dyndbg="file drivers/gpu/drm/msm/* +p"` uses a glob in the dynamic-debug
  expression. Kernel's dynamic_debug.c supports fnmatch-style globs and
  BLS preserves quotes. **Likely fine** but if no MSM debug spew, try
  dropping the `*`.
- No `console=ttyMSM0`. W767 has no user-accessible UART. All early-boot
  visibility goes to the framebuffer. The simpledrm/efifb path is the
  only diagnostic surface.

## Build pipeline observations

- `build-kernel.sh:24-25` defaults to `CROSS_COMPILE=aarch64-linux-gnu-`.
  For native aarch64 builds (e.g., on the W767 itself in Fedora), pass
  `--cross ''` or set `CROSS_COMPILE=`.
- `w767-initramfs.config:7-14` notes Phase 2 duplicates Phase 1 settings
  manually. Drift risk over time; a CI diff lint would help.
- `build-initramfs.sh` requires: pre-built Rust binaries (line 131-136),
  Alpine apks fetched once (`--fetch-userspace`), firmware staged at
  `firmware-stage/lib/firmware/` (line 153), kernel modules at
  `kernel/out/<target>/lib/modules/` (line 148), SSH pubkey readable
  (line 157).

## Per-subsystem readiness (first-boot confidence)

| Subsystem | Confidence | Why |
|-----------|-----------|-----|
| Display + GPU | 🟢 HIGH | iter-17 proven; no DTS regression in display block since |
| UFS root | 🟢 HIGH | iter-17 proven |
| USB host | 🟢 HIGH | All 3 controllers + PHYs enabled with regulators |
| WiFi (ath10k_snoc) | 🟡 MED-HIGH | DTS already correctly uses WCN3998 path. Just needs firmware at the right path. |
| Bluetooth | 🟡 MED-HIGH | DTS has `qcom,wcn3998-bt` + supplies on uart13. Needs `qca/crnv01.bin` + `qca/crbtfw01.tlv`. |
| Modem (MPSS) | 🟡 MED | Firmware ready; userspace daemons separate work; not boot-critical |
| Touchpad | 🟠 MED-LOW | Address correct (0x49 + 0xab post `07233d6`); open question on L4C regulator (B4) |
| Keyboard | 🔴 LOW | Blocks on EmuEC platform driver. Use USB keyboard. |
| Audio | 🔴 LOW | Needs ASoC machine driver for CS35L41 SPI + WCD9340 SLIMbus |
| Cameras | 🔴 LOW | sc8180x CAMSS not upstream |
| USB-C PD / Alt-Mode | 🔴 LOW | Connectors declared but no port topology wired |
| Battery / charging | 🔴 LOW | EmuEC blocked; SBS path identified but not yet wired in DTS |
| Suspend / resume | 🔴 LOW | Even sc8280xp X13s has flaky S2idle on mainline |

## Recommended pre-boot tweaks (NOT auto-applied)

These are design judgments left for the user/brother to decide:

1. **`w767-initramfs.config`: `CONFIG_I2C_HID=y` + `CONFIG_I2C_HID_OF=y`** —
   so the touchpad probes during kernel init on Phase 2 (no udev in
   initramfs). Trade-off: marginally larger kernel, but actually-usable
   touchpad in Phase 2 vs needing to manually modprobe.

2. **Add `ramoops` reserved-memory node to the DTS** — backs `PSTORE_RAM`
   so kernel crashes survive reboot at `/sys/fs/pstore/dmesg-ramoops-0`.
   Useful for diagnosing early-boot hangs that don't reach userspace.
   Trade-off: shaves a few MiB off available RAM; needs to not overlap
   any other reserved region.

3. **`build-kernel.sh`: add an auto-clone of Linux v7.0 if not found** —
   the current behavior is to print the clone command and exit. A small
   QoL win; debatable whether the script should silently download a 3GB
   source tree.

## Cumulative state of `main` after the 2026-05-17 sessions

| Commit | What |
|--------|------|
| `4c74ede` | (pre-session) Initial public release |
| `895a13e` | research: chip IDs (S2MM005/SM5508/PTN36502/SBS) + 4 corrections to canonical doc |
| `07233d6` | dts: touchpad reg=0x49, hid-descr-addr=0x00ab (revert iter-19 regression) |
| `a58bd66` | grub: SC8180X freeze-prevention quirks on phase1/phase2 cmdlines |
| (this) | build: fix DTS_SRC path in build-kernel.sh + docs (this audit + `BUILDING.md`) |

## What's still on the table after this session

- DTB binary rebuild: not done on the Windows ARM64 side this session
  (no dtc + flex/bison without sudo). The DTB at
  `deploy/deploy-iter19/sc8180x-samsung-w767.dtb.iter19` is stale (built
  from the wrong-touchpad DTS). The Linux-side build will produce a
  fresh DTB via `build-kernel.sh --target w767-initramfs`.
- EmuEC platform driver: still 200-400 LOC of work per the chip-ID doc.
- Touchpad regulator verification: empirical only — first-boot test will
  tell us if L4C needs to be re-enabled.
