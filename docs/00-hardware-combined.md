# Samsung Galaxy Book S (samsung-w767, SC8180X) — Combined Hardware Reference

**Compiled:** 2026-05-16 by Claude Opus 4.7
**Sources cross-referenced:** Gemini V1–V7 RE deliverables · my V1–V5 concerns docs · postmarketOS pmaports W767 port · mainline Linux 7.0 source · jhovold/linux X13s wiki · Cirrus Logic binding/header · DSDT (`dsdt.dat`, md5 `5c8499279d1043dfff19ddf2cab853f7`) · local iter-17 / iter-19 working state.
**Purpose:** Single authoritative document. Every claim labeled with source + verification status. Where sources contradict each other, the contradiction is named and the resolution given.

---

## 1. Headline corrections from cross-referencing

These are the things that changed when we left the Gemini-only loop and brought in the community sources + mainline kernel.

| Question | What Gemini V7 said | What cross-referencing reveals | Resolution |
|---|---|---|---|
| **Audio amp chip identity** | "CS35L40 (Inferred from driver strings/datasheet)" | `linux/include/sound/cs35l41.h:747`: `#define CS35L41_CHIP_ID 0x35a40`. The DEVID value `0x35a40` is what **CS35L41 silicon** returns. Windows naming its driver "CS35L40" is an internal Cirrus codename, not the silicon ID. | **Silicon is CS35L41.** Mainline `CONFIG_SND_SOC_CS35L41_SPI` will bind directly — no L40 quirk needed. The DT binding YAML (`Documentation/devicetree/bindings/sound/cirrus,cs35l41.yaml`) accepts both `cirrus,cs35l40` and `cirrus,cs35l41` compatibles and uses the same driver for both. |
| **Linux kernel driver path for amps** | "No in-tree CS35L40 driver exists; quirked CS35L41 module required" (V6) / "CS35L41_SPI with L40 quirk" (V7) | Per above — no quirk. Direct `cirrus,cs35l41` binding. Existing in-tree users (Sony Xperia sm8350/sm8250/sm8450 phones) all on I²C; we'd be the first SPI user in the qcom DTS tree but the binding fully supports SPI. | Add a standard `cs35l41@N` child node under `&spi0` / `&spi3` with `compatible = "cirrus,cs35l41"`. |
| **Boot args** | `earlycon=efifb keep_bootcon console=ttyHS0 clk_ignore_unused` (V5–V7) | pmOS canonical W767 cmdline (`pmaports/.../device-samsung-w767/kernel-cmdline.conf`): `quiet loglevel=2 iommu.passthrough=0 iommu.strict=0 pcie_aspm.policy=powersupersave clk_ignore_unused pd_ignore_unused arm64.nopauth efi=noruntime`. jhovold X13s wiki confirms `clk_ignore_unused pd_ignore_unused arm64.nopauth efi=noruntime` as the SC8180X/SC8280XP family quirks. | **iter-17's BLS entry is missing four essential quirks.** See §6 for the corrected cmdline. |
| **DTB filename** | `qcom/sc8180x-samsung-galaxybook-s.dtb` (V1) → `qcom/sc8180x-samsung-w767.dtb` (V2+) | pmOS `deviceinfo`: `deviceinfo_dtb="qcom/sc8180x-samsung-w767"`. Existing iter-17 BLS entries use the same. | **`qcom/sc8180x-samsung-w767.dtb`** — aligned across all sources. |
| **Firmware layout** | `qcom/sc8180x/SAMSUNG/GalaxyBookS/` (V1) → `qcom/sc8180x/samsung/w767/` (V3) → `qcom/samsung/w767/` (V5+) | pmOS `firmware-samsung-w767` APKBUILD installs to `/lib/firmware/qcom/samsung/w767/`. iter-17 already uses this path. | **`/lib/firmware/qcom/samsung/w767/`** — three-way confirmation. |
| **Modem firmware blob name** | `modem.mbn` (V2–V3) → `qcmpss8180_XEF.mbn` (V4) → `modem.mbn` (V5) | pmOS APKBUILD installs as `qcmpss8180_XEF.mbn`; existing iter-17 uses same name. Both names work as long as the DTS `firmware-name` matches. | **Keep `qcmpss8180_XEF.mbn`** (matches pmOS + iter-17). |
| **`space_pahp.cap` camera tuning** | Listed as critical file across V1, V2, V4, V5, V6, V7 | Not in any zip's firmware tree. Not in pmOS firmware-samsung-w767. Not anywhere on the open web. | **Phantom. Remove.** Real camera tuning is `com.qti.tuned.default.bin` + `com.qti.tuned.partron_hi1a1.bin` (and even those are only useful once `qcom-camss` is wired up, which is post-iter-19 work). |
| **DT phandle for `\_SB.IC19`** | `&i2c19` (V3) → `&i2c18` (V4+) | `sc8180x.dtsi:1458`: `i2c18: i2c@c84000`. IC19 MMIO base 0x00C84000 confirms. | **`&i2c18`** — V4+ is correct. |
| **Status enum labels for `CHST`** | "Charging / Discharging / AC Power / Critical Low" stated as fact (V4) → marked "Inferred" (V7) | DSDT line 95820 (`Method (PLDR)`) only shows `CHST == X → LED bit Y` mappings. Labels remain unconfirmed without observed runtime correlation. | **Mark as inferred.** Plan: boot Linux, watch `\_SB.EMEC.CHST` while plugging/unplugging AC to nail labels. |
| **DSDT bytes** | Implicitly assumed canonical | md5 `5c8499279d1043dfff19ddf2cab853f7` across all 7 zips — unchanged since V1. | **Single ground truth.** Any claim contradicting the DSDT loses. |

---

## 2. Source map (where each piece of info comes from)

| Source | Authority for | Status |
|---|---|---|
| **DSDT `dsdt.dat`** (in V1+ zip, md5 above) | ACPI device topology, _HIDs, I²C/SPI/GPIO resources, MMIO bases | **Ground truth** for static topology |
| **pmaports `device/testing/device-samsung-w767/`** | Boot args, DTB name, deviceinfo, initramfs modules | **Authoritative community port** — actively shipped to users |
| **pmaports `firmware-samsung-w767`** | Firmware blob list + install paths | **Authoritative** — derived from Jenneron's collection |
| **pmaports `linux-postmarketos-qcom-sc8180x`** | Kernel branch (pinned to `sc8180x-mainline/linux` commit `27c30b32...`), kernel config | **Authoritative** for "what works today on community kernel" |
| **mainline `linux/sound/soc/codecs/cs35l41*` + `include/sound/cs35l41.h`** | CS35L41 driver capabilities, chip ID values, binding | **Authoritative** for driver/silicon questions |
| **mainline `Documentation/devicetree/bindings/sound/cirrus,cs35l41.yaml`** | DT compatible strings, required properties | **Authoritative** |
| **mainline `arch/arm64/boot/dts/qcom/sc8180x.dtsi`** | DT phandle ↔ MMIO base mapping | **Authoritative** |
| **jhovold/linux X13s wiki** | SC8180X/SC8280XP family kernel cmdline quirks | Authoritative for the SoC family bring-up dance |
| **Gemini V1–V7 RE summaries** | Windows-side reverse engineering of EmuEC.sys, qcauddev8180.sys, qci2c8180.sys | Mixed — *high-value when DSDT-backed, low-value when speculating* |
| **Ghidra dumps in V4+** (EmuEC_Decompile_V3.txt, Audio_CS_Dump.txt, I2C_*_Decompile.txt) | Binary RE evidence | **Authoritative for what the Windows driver actually does**, less so for what Linux should do |
| **Jenneron's firmware-samsung-galaxy-book-s** (gitlab; mirrored locally) | Proprietary firmware blobs + `.jsn` PD-mapper descriptors | **Authoritative for firmware payload** |
| **iter-17 boot snapshot** (`iter-17-VICTORY-snapshot.txt`) | What actually booted in this house | **Empirical truth for display + GPU** |
| **`GalaxyBookS_Linux/dts-stage-v2/sc8180x-samsung-w767.dts` (iter-17/19)** | The DTS that produced a working boot | **Empirical** — additive changes only on this base |
| **NOT useful sources** | — | — |
| `sc8180x-mainline/linux` (gitlab) | — | **Archived since Jan 2023**, but pmaports still uses a pinned commit. Treat as a frozen reference. |
| `velvet-os/imagebuilder` | — | **No W767 profile.** Skip. |
| `aarch64-laptops/debian-cdimage` | — | **Lenovo Yoga C630 + Flex 5G only.** Skip for W767. |
| `aarch64-laptops/linux` | — | Active fork but no specific W767 work visible from landing-page introspection. Worth a deeper dig if a specific question arises; not load-bearing for current iteration. |
| Cirrus Logic `CirrusLogic/linux-drivers` | — | Downstream Cirrus tree, latest release v6.12. No CS35L40 driver. Confirms mainline `cs35l41.c` is the canonical driver. |

---

## 3. Hardware checklist with multi-source verification

| Peripheral | ACPI _HID | DSDT | pmOS deviceinfo | iter-17 state | mainline DTS | Status |
|---|---|---|---|---|---|---|
| **SoC** SC8180X | — | — | `soc-qcom-sc8180x` | working | `sc8180x.dtsi` | ✅ Mainline |
| **Storage** UFS0 | QCOM24A5 | MMIO 0x01D84000, IRQ 31 | — | working | UFS bindings in `qcom,ufs.yaml` | ✅ Built-in driver, must be `=y` |
| **Display** eDP | SAM0101 | — | initramfs has `msm` + `drm-dp-aux-bus` + `phy-qcom-edp` + `panel-edp` | working (iter-17 VICTORY: eDP-1 connected 1920×1080) | `mdss_edp` + aux-bus in DTSI | ✅ Working |
| **GPU** Adreno 680 | QCOM043A | — | uses `mesa-vulkan-freedreno` | working (msm/dpu/a3xx_ops bound) | mainline `msm` driver | ✅ Working |
| **Backlight** | — | — | initramfs has `leds-qcom-lpg` + `pwm-bl` | partial (eDP backlight) | LPG driver | ⚠️ DPCD-based via eDP aux-bus per V7; needs DT panel node verification |
| **Touchpad** TSC1 | STMT1234 | I2C2 (MMIO 0x00884000) @ 0x02, GpioInt 113 | initramfs has `i2c-hid-of`, `i2c-hid-acpi`, `hid-generic` | iter-19 adds `touchpad@2` on `&i2c1` | not upstream | 🟡 **iter-19** — DTB built, pending boot test |
| **Keyboard** SVBI | SAMM0901 | virtual scancodes via ACPI Notify, not a bus device | — | not implemented | not upstream | ❌ Needs custom EmuEC driver |
| **EmuEC** | SAM0604 | Multi-bus I²C: IC10/IC12/IC19/IC20 (DT `&i2c9/&i2c11/&i2c18/&i2c19`), slaves 0x33/0x25/0x1A/0x09/0x0B; OperationRegion 0x9C | iter-17 enables i2c9/11/19; iter-19 adds i2c18 | not implemented | not upstream | ❌ Driver work: see §9 |
| **Lid switch** LID0 | PNP0C0D | LIDR field in `\_SB.GIO0` OpRegion; Notify(LID0, 0x80) | — | not implemented | not upstream | ❌ Depends on EmuEC driver — surface as SW_LID via input subsystem |
| **Audio amps** (×2) | (no ACPI device, lives under ADSP scope) | SLM1.ADCM.AUDD: SPI targets on `\_SB.SPI1` (0x00880000 → `&spi0`) and `\_SB.SPI4` (0x0088C000 → `&spi3`); silicon is **CS35L41** (DEVID 0x35a40) | pmOS kernel has ALL CS35L drivers DISABLED | iter-19 enables `&spi0` + `&spi3`, no codec children yet | `cirrus,cs35l41` binding exists, no qcom DTS uses it yet | 🟡 **Driver ready**; needs DT child nodes + ASoC machine driver bringing SPI control + SLIMbus data together |
| **Audio data path** ADSP/SLIMbus | SLM1 (SLIMbus 1) | `\_SB.ADSP.SLM1` | initramfs has `qcom-q6v5-pas` + `qcom-common` + `pdr-interface`; kernel has `CONFIG_SLIMBUS_QCOM_NGD_CTRL=m` capability | not implemented (no ADSP-bringup yet) | mainline supports | ❌ Needs ADSP firmware-name + SLIMbus master config |
| **WiFi/BT** | (PCIe / WCN6855) | — | `firmware-samsung-w767-nonfree-firmware` includes `wlanmdsp.mbn`; depends on `linux-firmware-ath10k` (note: WCN6855 is ath11k upstream) | iter-17 may have it enabled but not verified working | mainline ath11k WCN6855 | 🟡 Firmware in place at `/usr/lib/firmware/ath11k/WCN6855/hw2.0/` (upstream linux-firmware); driver upstream |
| **Modem** mpss / WWAN | (Snapdragon X24) | _DEP chain references `\_SB.MPSS`, `\_SB.IPA0`, `\_SB.SCM0` | initramfs has `qcom-q6v5-pas` + `qrtr-smd`; runtime needs `rmtfs` + `pd-mapper` + `tqftpserv` + `qrtr` daemons | not implemented | mainline `q6v5-pas` supports | ❌ Firmware in pmOS pkg; runtime userspace from `soc-qcom-sc8180x-nonfree-firmware` |
| **Fingerprint** WBDI | SAM0909 (EgisTec) | SPI/I²C TBD, GPIO 32 | — | not implemented | no upstream driver (libfprint has no EgisTec EGIS0510) | ❌ Proprietary path; libfprint gap |
| **Cameras** CAMS/CAMF/CAMI | QCOM0429/0406/04A5 | MIPI CSI-2, _HIDs are Qualcomm CAMSS slot IDs | — | not implemented | `qcom-camss` driver in mainline; per-sensor: OV13855 + OV5695 + OV7251 (sensor identity from RE, *not* DSDT) | ❌ Needs DT + libcamera; sensor IDs are Gemini-inferred, not DSDT-verified |
| **USB-C / PD** | (Synopsys dwc3 x 2, QCOM0497) | — | initramfs supports | iter-17 enables dwc3 | mainline supports | 🟡 Likely works; not explicitly verified |
| **PMICs** | QCOM0430 (PM8150), SAM0606 (Samsung secondary) | DSDT-verified | iter-17 enables `pm8150*` includes | works | mainline | ✅ |

---

## 4. Definitive ACPI ↔ DT bus map (all sources agreed)

Verified against DSDT `Memory32Fixed` + `sc8180x.dtsi` node base addresses. **The 1-to-1 numeric correspondence ACPI suffix → DT suffix does NOT hold** — always go via MMIO base.

| ACPI | _HID | MMIO base | DT phandle | Used for |
|---|---|---|---|---|
| `\_SB.I2C2` | QCOM0411 | `0x00884000` | `&i2c1` | Touchpad (`touchpad@2`) — iter-19 |
| `\_SB.IC10` | QCOM0411 | `0x00A84000` | `&i2c9` | EmuEC reach (0x33, 0x25, 0x1A) — iter-17 enables |
| `\_SB.IC12` | QCOM0411 | `0x00A8C000` | `&i2c11` | EmuEC reach (0x1A) — iter-17 enables |
| `\_SB.IC19` | QCOM0411 | `0x00C84000` | `&i2c18` | EmuEC reach (0x33, 0x25) — iter-19 enables |
| `\_SB.IC20` | QCOM0411 | `0x00C88000` | `&i2c19` | EmuEC reach (0x09, 0x0B) — iter-17 enables |
| `\_SB.SPI1` | QCOM040F | `0x00880000` | `&spi0` | CS35L41 amp control — iter-19 enables |
| `\_SB.SPI4` | QCOM040F | `0x0088C000` | `&spi3` | CS35L41 amp control — iter-19 enables |

**QUP conflict reminder:** Each QUP slot can be either I²C or SPI, not both. The DT nodes `&i2c0` ↔ `&spi0`, `&i2c1` ↔ `&spi1`, `&i2c3` ↔ `&spi3` share MMIO bases. iter-19's `&spi0` enable means `&i2c0` must NOT be enabled (it isn't). `&i2c1` (touchpad) + `&spi1` would conflict — `&spi1` stays disabled. iter-19 is conflict-free.

---

## 5. Authoritative firmware layout

**Root:** `/lib/firmware/qcom/samsung/w767/` (Option A — confirmed by pmaports, jenneron, iter-17).

| Path | File | Source | Purpose |
|---|---|---|---|
| `qcom/samsung/w767/` | `qcadsp8180.mbn` | Jenneron / Windows DriverStore | ADSP firmware (audio + sensor) |
| `qcom/samsung/w767/` | `qccdsp8180.mbn` | Jenneron | CDSP firmware (compute DSP) |
| `qcom/samsung/w767/` | `qcdxkmsuc8180.mbn` | Jenneron | Adreno GPU zap shader |
| `qcom/samsung/w767/` | `qcmpss8180_XEF.mbn` | Jenneron | Modem subsystem firmware (78 MB) |
| `qcom/samsung/w767/` | `qcslpi8180.mbn` | Jenneron | SLPI (sensor low-power island) |
| `qcom/samsung/w767/` | `qcvss8180.mbn` | Jenneron | Video subsystem firmware |
| `qcom/samsung/w767/` | `qcwdsp8180.mbn` | Jenneron | Wireless DSP |
| `qcom/samsung/w767/` | `wlanmdsp.mbn` | Jenneron | (legacy filename; current ath11k uses split amss.bin + m3.bin instead) |
| `qcom/samsung/w767/` | `adspr.jsn`, `adspua.jsn`, `battmgr.jsn`, `cdspr.jsn`, `modemuw.jsn` | Jenneron | pd-mapper JSON descriptors |
| `qcom/samsung/w767/` | `storsec.mbn` | Jenneron | UFS secure storage |
| `qcom/samsung/w767/` | `dxhdcp2.mbn`, `hdcp{1,2p2,srm}.mbn`, `pr_3_wp.mbn` | Jenneron | DRM/HDCP (optional) |
| `qcom/` | `a680_gmu.bin`, `a680_sqe.fw` | Jenneron | Adreno-680 GPU controller |
| `qca/` | `crnv01.bin`, `crbtfw01.tlv` | Jenneron | Bluetooth (Qualcomm Atheros legacy path) |
| `ath11k/WCN6855/hw2.0/` | `amss.bin`, `board-2.bin`, `m3.bin`, `regdb.bin` | upstream `linux-firmware` | ath11k WiFi (preferred over Jenneron's legacy `wlanmdsp.mbn` for new boots) |
| `cirrus/` | `cs35l41-dsp1-spk-prot-calb.bin` | Gemini V1+ zip (Windows DriverStore) | CS35L41 calibration (per-device — keep) |
| `cirrus/` | `cs35l41-dsp1-spk-prot.bin` | mainline `linux-firmware` (generic) | CS35L41 protection firmware (default) |

**NOT in this layout (Gemini phantoms):**
- `space_pahp.cap` — doesn't exist anywhere; remove from all docs

---

## 6. Boot args — corrected from iter-17

**iter-17 current (insufficient):**
```
root=UUID=@ROOT_UUID@ ro rootflags=subvol=root console=tty0 loglevel=7 dyndbg="file drivers/gpu/drm/msm/* +p"
```

**Recommended for iter-19 (merging pmOS + iter-17 needs):**
```
root=UUID=@ROOT_UUID@ ro rootflags=subvol=root
console=tty0 loglevel=7
iommu.passthrough=0 iommu.strict=0
pcie_aspm.policy=powersupersave
clk_ignore_unused pd_ignore_unused
arm64.nopauth efi=noruntime
dyndbg="file drivers/gpu/drm/msm/* +p"
```

What each addition does:
- `clk_ignore_unused` / `pd_ignore_unused` — leaves "unused" clocks and power-domains on; required because Qualcomm's clock framework misidentifies in-use SoC blocks as unused and powers them off (locking up the system). Already in iter-17 per `galaxybook-s/fixes/kernel-cmdline.txt`; not visible in current BLS — worth checking it actually made it.
- `arm64.nopauth` — disables ARMv8.3 pointer authentication. SC8180X Cortex-A76 has erratum/firmware-bug interaction that crashes the kernel with PAC enabled.
- `efi=noruntime` — stops the kernel from calling UEFI runtime services; SC8180X firmware has a bug that hangs on certain runtime calls. Per jhovold X13s wiki, can be omitted on newer UEFI with "Linux Boot" option enabled — but iter-17 UEFI is the stock Samsung one, so keep this.
- `iommu.passthrough=0 iommu.strict=0` — SMMU configuration that the SC8180X needs for stable DMA on UFS/USB/PCIe.
- `pcie_aspm.policy=powersupersave` — PCIe link-state management; relevant once any PCIe device (ath11k WiFi) probes.

`earlycon=efifb` (Gemini V5+ suggestion) — **not used by pmOS, omit**. Early framebuffer console requires the UEFI GOP framebuffer handover to survive into kernel, which on aarch64 Qualcomm laptops is unreliable. The pmOS-style cmdline accepts a dark screen until DRM brings up panel-edp.

---

## 7. Kernel config additions (from pmOS reference)

Verified against pmaports `config-postmarketos-qcom-sc8180x.aarch64`:

```
# Already working in iter-17 (verify still =y or =m)
CONFIG_SCSI_UFS_QCOM=m          # critical for storage boot
CONFIG_PINCTRL_MSM=y
CONFIG_DRM_MSM=m
CONFIG_DRM_MSM_DPU=y
CONFIG_DRM_MSM_DP=y
CONFIG_SERIAL_QCOM_GENI=y       # GENI UART driver
CONFIG_SERIAL_QCOM_GENI_CONSOLE=y
CONFIG_I2C_QCOM_GENI=m
CONFIG_SPI_QCOM_GENI=m

# iter-19 additions
CONFIG_HID_GENERIC=m            # for touchpad@2
CONFIG_I2C_HID_OF=m             # hid-over-i2c device tree binding
# (CONFIG_I2C_HID_ACPI not needed on DT boot, but pmOS has it as m — harmless)

# For future audio work (currently disabled in pmOS)
CONFIG_SLIMBUS=m
CONFIG_SLIMBUS_QCOM_NGD_CTRL=m
CONFIG_REGMAP_SLIMBUS=m
CONFIG_SND_SOC_CS35L41_SPI=m    # IS the right driver despite "L40" Windows naming

# For EmuEC custom work — none in mainline; out-of-tree driver
```

The `cs35l41` driver does not need any quirk — its DEVID check expects `0x35a40` or `0x35b40` (R variant), which is exactly what the W767's amps report.

---

## 8. Initramfs modules (from pmOS — should match iter-19 initramfs)

```
# Input (touchpad)
hid-generic
i2c-hid-of
i2c-hid-acpi          # harmless on DT, included for safety

# Display
msm
drm
drm-dp-aux-bus
phy-qcom-edp
panel-edp
leds-qcom-lpg
pwm-bl

# Remoteproc (for ADSP/CDSP/MPSS bring-up)
pdr-interface
qcom-common
qcom-q6v5-pas
qrtr-smd
```

If iter-17's initramfs doesn't have these, ADSP/modem bring-up will fail post-iter-19. Check via `lsinitrd` on the existing initramfs.

---

## 9. Open questions / next iterations

| Item | Status | What's needed |
|---|---|---|
| EmuEC packet wire format | Still open — `EmuEC_Packet_Dump.txt` is the 181-byte stub for 5 rounds | Decompile the SPB callback functions that V7's qci2c8180.sys dumps *register* (function pointers in `FUN_140012000`'s setup) — those are the actual transfer entry points. V7 stopped at the WDF lifecycle layer. |
| EmuEC `CHST` value-to-state mapping | DSDT-verified values (0x05, 0x11, 0x21, 0x40), inferred labels | Empirical confirmation on running Linux: read `\_SB.EMEC.CHST` via the future EmuEC driver while plugging/unplugging AC. |
| ADSP SLIMbus bring-up | Hardware path documented, no Linux driver wiring | Add `&slimbam` / SLIMbus master node, ADSP firmware-name in DTS, ASoC machine driver. The mainline `cs35l41` driver handles the codec side. |
| Touchpad regulator dependency | iter-17 had a "ldo4c blocks PMIC probe" note for the broken touchscreen@49 attempt | Likely irrelevant for iter-19's `touchpad@2` (different address, no `vdd-supply` declared) — but watch boot for regulator probe failures. |
| Camera sensors | Sensor identification (OV13855/OV5695/OV7251) is RE inference, not DSDT-verified | Read Windows Device Manager hardware IDs to confirm. Then DT + `qcom-camss` driver + libcamera. Far-future work. |
| Fingerprint EgisTec EGIS0510 | Proprietary UMDF on Windows; libfprint has no driver | Skip — wait for libfprint upstream support. |
| Lid switch | DSDT path is `LIDR` field in EmuEC OpRegion | Comes free once EmuEC driver exists. |
| WiFi (ath11k WCN6855) | Mainline-supported; firmware in `/usr/lib/firmware/ath11k/WCN6855/hw2.0/` (upstream linux-firmware on host system) | Just needs `&wifi { status = "okay" }` + correct calibration variant. Already in iter-17 DTS — verify probe on iter-19 boot. |
| Modem bring-up | All firmware + userspace daemons cataloged | Enable `&mpss` (already done in iter-17 DTS), install `rmtfs` + `pd-mapper` + `tqftpserv` + `qrtr` daemons on rootfs. Probably the next-best feature ROI after touchpad. |

---

## 10. Resolved Gemini contradictions (full audit)

For posterity — these are the V1–V7 claims that surfaced as wrong during cross-checking and the source that settled them:

| Round | Claim | Wrong because | Resolution |
|---|---|---|---|
| V1 | Touchpad on `&i2c2` at addr 0x02 | ACPI I2C2 maps to DT `&i2c1` by MMIO base | Fixed in V5 (touchpad@2 on `&i2c1`) |
| V1 | Lid switch on `gpios = <&tlmm 50>` | LIDR is an OpRegion field, not a TLMM pin | Fixed in V2/V3 |
| V1 | Audio amps CS35L41 on I²C 0x40/0x41 | No such ACPI device exists | Replaced with SLIMbus+SPI story in V3 |
| V1 | `pm8150c.dtsi` included | File doesn't exist | Removed in V2 |
| V1 | `serial0 = &uart2` | Should be `uart13` | Fixed in V2 |
| V2 | Audio "SoundWire SAMM0802" | AUDD is on SLIMbus, not SoundWire | Fixed in V3 |
| V3 | `IC19 → &i2c19` | MMIO 0x00C84000 is `&i2c18` | Fixed in V4 |
| V4 | Touchpad bus `&i2c2` re-asserted | Same as V1 — base 0x00888000 is wrong (real is 0x00884000) | Fixed in V5 |
| V4 | SPI1→`&spi1`, SPI4→`&spi4` | MMIO bases map to `&spi0`/`&spi3` | Fixed in V5 |
| V4 | EmuEC "16-byte struct, SOC@offset 0x06" | Real length is 8 bytes; "0x06" is field ID, not byte offset | Fixed in V5 |
| V4–V7 | Audio amp is CS35L40 | DEVID 0x35a40 = CS35L41 silicon per `include/sound/cs35l41.h:747`. Windows codename misled. | **Resolved in this combined doc** |
| V1–V7 | `space_pahp.cap` is a critical camera tuning file | File doesn't exist anywhere | **Resolved in this combined doc** — phantom, remove |
| V5+ | `earlycon=efifb console=ttyHS0` boot args | pmOS production cmdline doesn't include these; X13s wiki doesn't either | **Resolved in this combined doc** — use pmOS-style quirks instead |

---

## 11. What this house has that the community doesn't

For symmetry — items where the local W767 work (Gemini + iter-17 + this combined doc) goes beyond what's in pmOS or other community sources:

- **A working iter-17 boot with display + GPU**. pmOS may have this but their kernel pkg ships no W767-specific patches — the iter-17 DTS evolution (rmtfs_mem placement, gpu_mem relocation, edp_ref_clk fixed-clock hack, etc.) was independently done in `dts-stage-v2/`.
- **A decompiled DSDT cross-reference** for every claim — pmOS just ships the kernel + firmware; doesn't document the hardware origin.
- **The CS35L41 silicon identification** via DEVID 0x35a40. Even pmOS doesn't have CS35L drivers enabled — nobody on the community side has actually identified what's on those buses. This combined doc is the first place to say "it's L41, use the standard driver."
- **Ghidra-level decompiles of EmuEC.sys, qcauddev8180.sys, qci2c8180.sys** — original RE work in V4–V7 zips.

---

**Bottom line:** With 7 rounds of Gemini iteration + cross-referencing against pmOS + mainline + the Cirrus binding + the X13s wiki, the picture for samsung-w767 is now: known-working = display + GPU; known-actionable = touchpad (iter-19 ready), modem userspace, WiFi probe, CS35L41 audio (driver exists, just needs DT + machine setup); known-blocking = EmuEC driver work (battery, keyboard, lid all gated on this). The audio path was the biggest Gemini red herring and is now resolved.
