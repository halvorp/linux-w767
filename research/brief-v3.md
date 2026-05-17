# Deep-research brief v3: Fedora on Samsung Galaxy Book S (SM-W767, SC8180X)

**Context**: Updated third-pass brief. Between v2 and v3 we landed interactive
SSH access to the device via a USB-C-to-ethernet dongle and ran a comprehensive
on-device audit + a live `modprobe msm` experiment. This brief contains the
**actual log snippets and sysfs state** captured on hardware — not abstract
descriptions. Goal: identify the precise reason the internal eDP panel stays
dark, and the cascade that keeps ADSP / MPSS remoteprocs offline.

## State of play (as of end-of-day iteration 8)

Booting `sc8180x-lenovo-flex-5g.dtb` (mainline, kernel 7.0.0-62.fc45.aarch64)
with Samsung-extracted firmware at `qcom/sc8180x/LENOVO/82AK/*` on the SM-W767:

- ✅ Kernel boots cleanly to `multi-user.target` in ~30 s
- ✅ `sshd` running; interactive login works over USB ethernet at 192.168.1.118
- ✅ Internal USB-HID keyboard enumerated (`Samsung SPACE v57` on
  `xhci-hcd.1.auto`), Caps Lock / Fn Lock LEDs respond
- ✅ `simpledrm` bound to `simple-framebuffer.0`, `card0-Unknown-1 status=connected
  enabled=enabled modes=1920x1080`
- ✅ Backlight PWM driver (`pwm_bl`) bound; brightness writable at 2048/4095;
  `actual_brightness: 2048` (readback confirms value lands)
- ✅ remoteproc CDSP running (firmware loaded OK from Lenovo mirror)
- ✅ Bluetooth (WCN3990 over serial0): QCA firmware `crbtfw21.tlv` + `crnv21.bin`
  loaded; controller reports version
- ❌ **Internal eDP panel dark.** `bl_power: 4` (FB_BLANK_POWERDOWN). Setting
  `bl_power=0` live via SSH does not visibly light the panel.
- ❌ `ath10k_snoc` loads but **no `wlan0` interface ever appears**
- ❌ remoteproc ADSP state `offline`, remoteproc MPSS (modem) state `offline`
- ❌ `msm_dpu` (when force-loaded live) fails to bind GPU with `-ENODEV`

## Live `modprobe msm` experiment results

We blacklisted `msm` on the kernel cmdline (to avoid a previous dark-screen
cascade). After boot, we force-loaded it over SSH to see what happens. Key
dmesg excerpts:

```
[1749.239] calling  panel_edp_init+0x0/0xfb8 [panel_edp] @ 3633
[1749.283] panel-simple-dp-aux aux-ae9a000.displayport-controller: supply power not found, using dummy regulator
[1749.312] ------------[ cut here ]------------
[1749.319] WARNING: drivers/gpu/drm/panel/panel-edp.c:814 at generic_edp_panel_probe+0x13c/0x280 [panel_edp], CPU#5: (udev-worker)/3633
... (call trace: generic_edp_panel_probe → panel_edp_probe → panel_edp_dp_aux_ep_probe → dp_aux_ep_probe → really_probe)
[1749.792] panel-simple-dp-aux aux-ae9a000.displayport-controller: Unknown panel BOE 0x07e7, using conservative timings
[1749.806] msm_dpu ae01000.display-controller: bound ae90000.displayport-controller (ops msm_dp_display_comp_ops [msm])
[1749.818] msm_dpu ae01000.display-controller: bound ae98000.displayport-controller (ops msm_dp_display_comp_ops [msm])
[1749.828] msm_dpu ae01000.display-controller: bound ae9a000.displayport-controller (ops msm_dp_display_comp_ops [msm])
[1749.839] adreno 2c00000.gpu: supply vdd not found, using dummy regulator
[1749.848] adreno 2c00000.gpu: supply vddcx not found, using dummy regulator
[1749.920] msm_dpu ae01000.display-controller: failed to load adreno gpu
[1749.929] msm_dpu ae01000.display-controller: failed to bind 2c00000.gpu (ops a3xx_ops [msm]): -19
```

Observations:

1. `msm_dpu` successfully binds all **three** DisplayPort controllers —
   `ae90000` / `ae98000` (external USB-C alt mode) and `ae9a000` (internal
   eDP). So the DPU + DP sides work.
2. The **GPU bind fails with -19 (ENODEV)** via `a3xx_ops`. Note: SC8180X uses
   Adreno 680 (A6xx-family), not A3xx. Is the `a3xx_ops` symbol name here
   just generic, or is the driver genuinely falling back to a wrong family?
3. `panel_edp` falls back to EDID-only "conservative timings" because DTS
   compatible is `edp-panel` (Flex 5G style) not `boe,nv133fhm-n61` (Samsung's
   actual panel — which IS in kernel 7.0's panel-edp database for `0x07e7`).
4. `panel-simple-dp-aux: supply power not found, using dummy regulator` — the
   eDP panel's `power-supply` is missing from the Flex 5G DTS (not wired up
   for Samsung's rail layout).
5. The "dummy regulator" warnings on GPU `vdd`/`vddcx` happen **also on working
   Flex 5G hardware** per the mainline DTS (both Flex 5G and Samsung's jenneron
   6.1.2 DTS just have `&gpu { status = "okay"; };` with no supply overrides).
   So dummy-regulator isn't by itself the blocker — it's correlated noise.

## remoteproc state (from audit 70-remoteproc-state.txt)

```
/sys/class/remoteproc/remoteproc0:  state=offline  firmware=qcom/sc8180x/LENOVO/82AK/qcmpss8180_nm.mbn  name=modem
/sys/class/remoteproc/remoteproc1:  state=running  firmware=qcom/sc8180x/LENOVO/82AK/qccdsp8180.mbn    name=cdsp
/sys/class/remoteproc/remoteproc2:  state=offline  firmware=qcom/sc8180x/LENOVO/82AK/qcadsp8180.mbn    name=adsp
```

CDSP powers up; ADSP and MPSS do not. Note that the modem firmware name on our
setup is `qcmpss8180_nm.mbn` (loaded from Lenovo's path), but the Samsung
Windows-extracted blob is `qcmpss8180_XEF.mbn` — **we mirrored the Samsung file
to the LENOVO path but did not rename to `_nm.mbn`**. So remoteproc0 is looking
for a file that doesn't exist under that exact name. Whereas `qcadsp8180.mbn`
is a direct filename match, so remoteproc2 should load — but it's `offline`.
Why?

## Backlight state (from audit 43-backlight-all.txt)

```
=== /sys/class/backlight/backlight ===
  brightness: 2048
  max_brightness: 4095
  actual_brightness: 2048
  bl_power: 4        ← FB_BLANK_POWERDOWN
  type: raw
  scale: non-linear
```

Live-poke experiment results (LIVE-backlight-dance.txt in audit):

- Wrote `0` → `511` → `1023` → `2047` → `3071` → `4095` into `brightness`. Each
  write lands (`actual_brightness` readback confirms). **Panel stays dark
  regardless.**
- Wrote `0` (unblank) → `1` (blank) → `4` (powerdown) → `0` (unblank) into
  `bl_power`. Each write lands. **Panel stays dark regardless.**

So both brightness control and bl_power control reach `pwm_bl` and get stored,
but the screen never lights up at any setting. Suggests the PWM hardware
isn't actually being driven, or the panel's `enable-gpios` signal isn't
asserted, OR no DPU video signal is reaching the panel (the `msm_dpu` DRM
card never registered because GPU binding failed).

## Wi-Fi stack status (from audit 68-wifi-dmesg.txt + regulator summary)

```
[66.019] (udev-worker): 18800000.wifi: hwdb modalias key: "of:NwifiT(null)Cqcom,wcn3990-wifi"
[66.105] (udev-worker): Loading module: of:NwifiT(null)Cqcom,wcn3990-wifi
[66.970] calling  ath10k_snoc_driver_init+0x0/0xfd0 [ath10k_snoc] @ 1033
[66.978] initcall ath10k_snoc_driver_init+0x0/0xfd0 [ath10k_snoc] returned 0
[66.978] (udev-worker): Inserted module 'ath10k_snoc'
[67.828] (udev-worker): ath10k_snoc: Too many messages being logged to kmsg, ignoring
[67.872] calling  qcom_pdm_drv_init+0x0/0xfd0 [qcom_pd_mapper] @ 1047
[67.873] initcall qcom_pdm_drv_init+0x0/0xfd0 [qcom_pd_mapper] returned 0
```

Note the `Too many messages being logged to kmsg, ignoring` — ath10k_snoc was
spamming enough that udev rate-limited it. We lost those messages.

Wi-Fi regulator state (from audit 50-regulator-summary.txt):

```
ldo1 (752mV, 0mA use=0):  18800000.wifi-vdd-0.8-cx-mx         use=0  → not powered
ldo7 (1800mV, 0mA use=0): 18800000.wifi-vdd-1.8-xo            use=0  → not powered
ldo9 (1296mV, 0mA use=0): 18800000.wifi-vdd-1.3-rfa           use=0  → not powered
ldo10 (3000mV, 0mA):      18800000.wifi-vdd-3.3-ch1           use=0  → not powered
ldo11 (3296mV, 0mA):      18800000.wifi-vdd-3.3-ch0           use=0  → not powered
```

Every Wi-Fi supply is `use=0` — none of the WCN3990's power rails are actually
turned on. The supplies are listed in the DT (Flex 5G DTS wires them
correctly), but nothing has requested them to be enabled. This matches
`ath10k_snoc` having driver loaded but never registering a wireless device.

## Display clocks (from audit 51-clk-summary.txt, filtered to display-cc)

ALL `disp_cc_mdss_*` clocks show `enable_cnt=0` but `prepare_cnt` enabled
because we have `clk_ignore_unused`. They're prepared but not actually
running, because no DRM driver is driving them. Example:

```
disp_cc_mdss_edp_pixel_clk_src   prepare=0  enable=0   rate=0     Y
disp_cc_mdss_edp_link_clk_src    prepare=0  enable=0   rate=0     Y
disp_cc_mdss_mdp_clk             prepare=0  enable=0   rate=345MHz
```

## DRM state (from audit 41-drm-details.txt)

Only `card0` exists, owned by simpledrm:

```
=== /sys/class/drm/card0-Unknown-1 ===
  status: connected
  enabled: enabled
  modes: 1920x1080
  edid:          ← empty file
  connector_id: 37
```

The EDID file is empty (simpledrm doesn't read real EDID — it's a dumb
framebuffer that inherits UEFI's pre-set mode). panel-edp's WARN about panel
ID `0x07e7` came from the real DP AUX transaction, so the panel IS responding
to AUX commands over the DP link when msm_dpu is loaded.

## CPU compat string

```
OF_COMPATIBLE_0=qcom,kryo485
```

Marketed as Kryo 495, but DT binding is `kryo485`. Good to know.

## Open questions for Gemini

### 1. What is the actual reason `msm_dpu` fails to bind the Adreno 680 GPU with -ENODEV on kernel 7.0 + Flex 5G DTS?

Is it any of:

- **GMU (`gmu@2c6a000`) not probing first**, so `&gpu { qcom,gmu = <&gmu>; }`
  reference resolves to a device with no driver → GPU probe returns -ENODEV?
- **Zap-shader firmware path mismatch** — Flex 5G DTS sets
  `firmware-name = "qcom/sc8180x/LENOVO/82AK/qcdxkmsuc8180.mbn"`. We mirrored
  Samsung's blob to that path. Is the blob signed against specific hardware
  hash that fails verification on SC8180X-Samsung vs SC8180X-Lenovo?
- **Missing interconnect property**. Mainline 7.0's `sc8180x.dtsi` has
  `interconnects = <&gem_noc MASTER_GRAPHICS_3D 0 &mc_virt SLAVE_EBI_CH0 0>;`
  in the gpu node. Is there a `power-domains` or new property that must be
  added in board DTS for kernel 7.0+?
- Something else in the Adreno A6xx driver that returns -ENODEV when a680_gmu
  firmware load fails silently earlier?

### 2. Why are ADSP and MPSS remoteprocs offline when CDSP is running?

All three remoteprocs use the same `qcom,q6v5-pas` driver. CDSP loads
firmware from the Lenovo path and runs. ADSP points at
`qcom/sc8180x/LENOVO/82AK/qcadsp8180.mbn` (we placed the Samsung blob there)
but stays `offline`. Is there:

- A silent firmware validation failure we're not seeing?
- A PAS (Peripheral Authentication Service) signature mismatch because the
  Samsung ADSP firmware blob carries a hash specific to Samsung's QFPROM
  fuses, and SC8180X-Samsung reports different fuse values than SC8180X-Lenovo?
- A dependency on a memory region reservation that's sized for Lenovo but not
  Samsung?

### 3. What's the working kernel version + DTS combo anywhere?

- `jenneron/linux@galaxy-book-s-6.1.2` — known old (early 2023), may or may
  not have worked fully.
- `gitlab.postmarketos.org/postmarketOS/pmaports:device/testing/device-samsung-w767`
  references `linux-postmarketos-qcom-sc8180x >= 6.6.0`. Does that kernel
  fork actually drive the Samsung panel and Wi-Fi? Any install reports
  post-2024? Any branch/tag/commit that corresponds to a known-good boot?
- Any MR on linux-arm-msm@vger.kernel.org (lore.kernel.org) adding a
  `sc8180x-samsung-w767.dts` that we haven't found?

### 4. The "no wlan0 but ath10k_snoc loaded" problem

ath10k_snoc driver loads cleanly, but no wireless netdev registers and all
five WCN3990 regulators stay `use=0`. The driver spammed so much kmsg that
udev rate-limited it so we lost the real error messages. Is there a known
cause (probe order dep on ADSP service-locator being up? pd-mapper needing
a running ADSP?) or a known workaround?

## Constraints (unchanged from v1/v2)

- x86_64 Fedora 43 build host; image edits via libguestfs.
- Galaxy Book S has two USB-C ports, one used for boot USB, one now used for
  USB-to-ethernet dongle for SSH access. No externally accessible serial.
- Windows 11 still on UFS (for re-extraction if needed).
- Internal keyboard works fully via USB HID, so we can interact during boot.
- We have interactive SSH at 192.168.1.118 for live experiments.

## Concrete, actionable asks

1. A pointer to a kernel source tree (any fork, any tag) that successfully
   probes `msm_dpu` → Adreno 680 binding on SC8180X on mainline-ish kernel
   (6.6+ is fine, 7.x ideal).
2. If the answer is "nobody has made this work yet," confirm that explicitly
   so we don't keep hunting.
3. If the answer is "build kernel X with patch series Y," give us the exact
   commit range or MR link.
4. If the answer is "the Flex 5G DTS has a bug on Samsung only because of
   regulator/pinctrl difference Z," tell us what Z is.

## Appendix: what we have staged on the Fedora host

- `/home/peter/Documents/GalaxyBookS_Linux/audit-extracted/audit-20260423-175507/` —
  full on-device audit, 64 files, ~1 MB, captured 2026-04-23 during live SSH.
- `/home/peter/Documents/GalaxyBookS_Linux/mainline-dts/` — freshly-fetched
  mainline Linux `sc8180x.dtsi`, `sc8180x-lenovo-flex-5g.dts`,
  `sc8180x-primus.dts`, `pm8150*.dtsi`.
- `/home/peter/Documents/GalaxyBookS_Linux/dts-stage/` — jenneron@6.1.2
  reference DTS + DTSI chain + dt-bindings headers.
- `/home/peter/Documents/GalaxyBookS_Linux/firmware-stage/` — extracted
  firmware (Windows DriverStore + pmOS jenneron firmware repo combined).
- `/home/peter/Documents/GalaxyBookS_Linux/win-extract/` — UTF-8-converted
  Windows PnP dump (pnp_details.txt, pnp_all.txt, resourcemap.reg, etc.).
- `/home/peter/Documents/GalaxyBookS_Linux/imgages/fedora-gbs.raw` — 13 GB
  Fedora Rawhide image currently configured with Flex 5G DTB.
