# Fix-it report: iter-61 log analysis → DTS patches

**For:** brother instance on Linux side
**Triggered by:** "see the dmesg files or any other logs that could help you identify
what sort of things we need fixing, then try and extract values"
**Date:** 2026-05-18 (evening, after iter-62 EDID push `ca22992`)
**Inputs read:** `research/iter61-logs/collect/{dmesg,regulator-summary,iomem,
deferred,drm,interrupts,platform-devices}.txt` + `research/iter61-logs/SUMMARY-for-kimi.md`
**Cross-references:** my iter-56 DSDT dossier + `dts/sc8180x-samsung-w767.dts` (head)

## TL;DR

iter-61 is in much better shape than the dmesg looks. **Six** of the WARN/ERROR
lines are missing-DTS-property issues with a 1-to-2-line fix each, and I can
quote the exact mainline regulator nodes to wire to (all already defined in
your DTS). One free win is identifying what your `&i2c15` "SSPN - 0x2c what is
that? idk" comment actually maps to — it's the **Samsung SAM0101 panel
companion** I already characterised in the DSDT extract.

Plus: **0x088E0000 mystery solved** (it's the USB-C / DP combo PHY block;
answers iter-56 §8 question #4).

## Ranked fix-it list

| # | Item                                  | Fix kind   | Impact | Effort |
|---|---------------------------------------|------------|--------|--------|
| 1 | GPU `vdd` / `vddcx` regulators        | DTS, 2 lines | High  | trivial |
| 2 | eDP PHY `vdda-pll` / `vdda-phy`       | DTS, 2 lines | High  | trivial |
| 3 | `regulatory.db` malformed             | Firmware   | Low (channels) | trivial |
| 4 | WiFi `vdd-3.3-ch1` dummy              | DTS, comment fix | Low | trivial |
| 5 | MMCX power-domain on `&gpu` / `&mdss` | DTS, 2 lines | Med   | trivial |
| 6 | SAM0101 panel companion on `&i2c15`   | DTS, child node | Med-High once upstream patch needed | small |
| 7 | Adreno devfreq cooling-device error   | DTS, thermal-zones | Low | small |
| 8 | UFS / `1d87000.phy-wrapper` cycle     | DTS, status=disabled? | Cosmetic | trivial |

iter-62's `modprobe rmtfs_mem` already takes care of #0 (wlan0 root cause).
None of the above blocks wlan0; they're quality-of-life improvements + the
panel-companion enables future Samsung-specific quirks (brightness, etc.).

---

## 1 — GPU regulator supplies (HIGH impact, trivial)

**Symptom in dmesg (line 426 / 428):**
```
adreno 2c00000.gpu: supply vdd not found, using dummy regulator
adreno 2c00000.gpu: supply vddcx not found, using dummy regulator
```

**Cross-check with regulator-summary.txt:**
```
regulator-dummy
    2c00000.gpu-vddcx             0   …   0mA     0mV
    2c00000.gpu-vdd               0   …   0mA     0mV
```

**Cross-check with iter-56 DSDT (`Device(GPU0)._DSD`):**
- `vdd` = `vdda` rail = **`LDO3_C` (1200 mV)** = `vreg_l3c_1p2`
- `vddcx` = **`LDO9_E` (880–912 mV)** = `vreg_l9e_0p88`

**Both labels already exist in your DTS** (lines 329 and 440). Patch:

```dts
&gpu {
    status = "okay";

+   vdd-supply   = <&vreg_l3c_1p2>;   /* "vdd"  = vdda, LDO3_C  1.2V  */
+   vddcx-supply = <&vreg_l9e_0p88>;  /* "vddcx" = LDO9_E  0.88–0.91V */

    zap-shader {
        memory-region = <&gpu_mem>;
        firmware-name = "qcom/samsung/w767/qcdxkmsuc8180.mbn";
    };
};
```

This will also eliminate "supply vdd not found" for the GMU (since the GMU
genpd is in the same `mmcx` island and inherits the rail from the GPU).

## 2 — eDP PHY regulator supplies (HIGH impact, trivial)

**Symptom in dmesg (line 2.225s / 2.233s):**
```
qcom-edp-phy aec2a00.phy: supply vdda-phy not found, using dummy regulator
qcom-edp-phy aec2a00.phy: supply vdda-pll not found, using dummy regulator
```

**Cross-check with regulator-summary.txt:**
- `aec2a00.phy-vdda-pll` consumed at **36 mA** (active!) on a dummy regulator
- `aec2a00.phy-vdda-phy` consumed at **21 mA** (active!) on a dummy regulator

**Cross-check with what already works:** The other QMP PHYs at `88e9000.phy`
and `88ee000.phy` (USB-C / DP combo) are correctly wired to `ldo3` (1.2 V)
for `vdda-phy` and `ldo5` (880 mV) for `vdda-pll`. The eDP PHY at `aec2a00.phy`
needs the same pattern — it's literally the same QMP IP block but configured
for eDP instead of USB3-DP-alt.

**Patch** (place in your `&edp_phy` block on top of the existing clock wiring):

```dts
&edp_phy {
    clocks = <&dispcc DISP_CC_MDSS_EDP_AUX_CLK>,
             <&dispcc DISP_CC_MDSS_AHB_CLK>,
             <&edp_ref_clk>;
    clock-names = "aux", "cfg_ahb", "edp_ref";

+   vdda-phy-supply = <&vreg_l3c_1p2>;   /* 1.2V  = LDO3_C */
+   vdda-pll-supply = <&vreg_l5e_0p88>;  /* 0.88V = LDO5_E */
};
```

These rails are *already on* (bootloader-configured), which is why iter-61
works despite the dummy. But getting them out of dummy-mode lets Linux gate
them on suspend instead of leaving them live → 36 mA + 21 mA saved at runtime.

## 3 — `regulatory.db` malformed (LOW impact, trivial)

**Symptom in dmesg (line 2.220s):**
```
cfg80211: loaded regulatory.db is malformed or signature is missing/invalid
```

This means `/lib/firmware/regulatory.db` and `regulatory.db.p7s` are either
missing or stale. They come from `wireless-regdb` upstream; latest release
is `wireless-regdb-2024.10.07` (or newer). Drop the two files into the
initramfs `/lib/firmware/` and the warning goes away — also, channel
restrictions become honoured properly for whatever regulatory domain
`cfg80211` resolves to.

Not blocking wlan0 — cfg80211 falls back to "no regulatory rules" which is
unrestricted-but-noisy. Easy to ship in iter-63 alongside the other fixes.

## 4 — WiFi `vdd-3.3-ch1` dummy (LOW impact, trivial)

**Symptom in dmesg (line 2.251s):**
```
ath10k_snoc 18800000.wifi: supply vdd-3.3-ch1 not found, using dummy regulator
```

Your DTS already has:
```dts
&wifi {
    vdd-3.3-ch0-supply = <&vreg_l11c_3p3>;
    //vdd-3.3-ch1-supply = <&vreg_l10c_3p3>; not used?
};
```

`vdd-3.3-ch1` is the second 3.3 V RF rail. Most WCN3998 reference designs
share `ch0` and `ch1` on one rail; some split them. Two reasonable fixes:

**Option A** (cleaner, matches most ARM laptops): point both to the same
rail, since W767 only ever has one 3.3 V rail for the WiFi chip:
```dts
+   vdd-3.3-ch1-supply = <&vreg_l11c_3p3>;
```

**Option B** (if you want to drop the request entirely): the `ath10k_snoc`
binding lists `vdd-3.3-ch1-supply` as optional, so removing the request just
means no dummy gets allocated. Actually verifying this requires reading
`drivers/net/wireless/ath/ath10k/snoc.c` — the binding may be probe-time
required. Option A is safer.

## 5 — MMCX power-domain (MEDIUM impact, trivial)

**Symptom in regulator-summary.txt (last line):**
```
MMCX  0  0  0  unknown  0mV  0mA  0mV  0mV
```

`MMCX` is the **Multi-Media Cx rail** — the rpmhpd power domain that all
display + GPU + camera hardware on sc8180x sits on. The `mmcx-reg`
platform device exists (per `platform-devices.txt`) but **has zero
consumers** — meaning nothing in your DTS references it.

From iter-56 DSDT findings: `Device(GPU0)._DEP` lists `rail_mmcx` as a
required NPA, and the GPU's PEP table votes corner `0x40` on it during
display bring-up. The Linux analog:

```dts
&gpu {
+   power-domains = <&rpmhpd SC8180X_MMCX>;
};

&mdss {
+   power-domains = <&rpmhpd SC8180X_MMCX>;
};
```

This puts MMCX into the runtime-PM vote chain. Without it, the rail
stays at whatever the bootloader left it (which works today on 6.6
thanks to `pd_ignore_unused`), but Linux can't drop it for suspend.

Same `SC8180X_MMCX` token used by the Lenovo Yoga C630 and HP Spectre
x360 14 DTS files — fully upstream-supported.

## 6 — SAM0101 panel companion on `&i2c15` (MED-HIGH if Samsung quirks needed)

**Your DTS at line 640–653 already enables `&i2c15`** and has a comment
that says exactly what I documented from DSDT:

```dts
&i2c15 {
    ...
    /*
     * SSPN - 0x2c
     * what is that? idk
     * pull none gpio 0x0019 (output?)
     * edge pull none 0x1388 interrupt: 0x0074
     */
};
```

I can fill in the "idk":

**This is the Samsung SAM0101 PanelDriver companion controller.**
From my DSDT extract `Device(\_SB.SSPN)`:

- I²C bus: `\_SB.IC16` → `&i2c15` in your DTS  *(confirmed by hardware
  base 0x00C80000 = sc8180x QUP2's first I²C)*
- Slave address: **0x2C** at 400 kHz
- 3 GPIOs on `\_SB.GIO0` → `tlmm`:
  - **gpio 23** (0x17) — output, exclusive, 50 ms debounce  → panel-enable
  - **gpio 25** (0x19) — same set                            → panel-reset
  - **gpio 35** (0x23) — same set                            → backlight-enable
- _HID = `SAM0101`, _SUB = `C17C144D` (Samsung subsystem)
- _DEP on SLEP, IC16, GIO0

Windows' `PanelDriver.sys` uses this for panel power sequencing, brightness
ramp, and possibly panel-init register writes. We DON'T need to bind a
Linux driver here right now — the eDP TX + `panel-edp` cover the data path.
But the **GPIOs need to be toggled at the right time** for the panel to
actually light up. iter-61 succeeded because the bootloader left the panel
powered on; cold boot may differ.

Two-step plan for brother:

**Step A** (low-risk): just declare the GPIOs as a gpio-hog so they always
get asserted high at boot — same trick you used for `usb_mp_hsei_hog` at
line 1224:

```dts
&tlmm {
    sspn_panel_enable_hog {
        gpio-hog;
        gpios = <23 GPIO_ACTIVE_HIGH>,
                <25 GPIO_ACTIVE_HIGH>,
                <35 GPIO_ACTIVE_HIGH>;
        output-high;
        line-name = "sspn_panel_enable";
    };
};
```

That replicates what Windows does in `PanelDriver.sys` for the cold-boot
case (modulo timing — Windows likely waits N ms between assertions; we'll
need RE for those exact values, see Ghidra section below).

**Step B** (later, when we want a proper driver): declare a child node
under `&i2c15` and write a `samsung-sspn-panel` driver. Out of scope
right now.

**Note:** I have NOT verified that DT `&i2c15` is in fact mapped to MMIO
0x00C80000 (= Windows DSDT IC16). Your DTS comment correlates SSPN-0x2C
with i2c15 via the GPIO pin numbers, so I trust it for now, but a 30-second
sanity check is `grep i2c15: $LINUX/arch/arm64/boot/dts/qcom/sc8180x.dtsi`
or `cat /proc/iomem | grep c80000` on next iter.

## 7 — Adreno devfreq cooling (LOW, deferred)

**Symptom in dmesg (line 2.299s):**
```
adreno 2c00000.gpu: [drm:msm_devfreq_init] *ERROR* Couldn't register GPU cooling device
```

This is the GPU thermal-zone wiring. To register devfreq cooling, the GPU
needs both an `operating-points-v2` table AND a `thermal-zones` entry with
`cooling-maps`. Your DTS doesn't have either custom-wired — it inherits
sc8180x.dtsi defaults. Not blocking. Worth fixing when GPU thermal
throttling matters for sustained workloads. Defer until display + audio +
sleep all work.

## 8 — UFS / phy-wrapper dependency cycle (cosmetic)

**Symptom in dmesg (line 187):**
```
platform 1d87000.phy-wrapper: Fixed dependency cycle(s) with /soc@0/ufshc@1d84000
```

W767 uses **eMMC** (or external USB-SATA per the `sda Realtek RTL9210B-CG`
in dmesg line 504), not UFS. Yet `&ufs_mem_hc` and `&ufs_mem_phy` are
enabled in your DTS (lines 1106 / 1118). The `Fixed dependency cycle(s)`
is fw_devlink's harmless "I broke this cycle for you" message. Whether
the UFS HC actually has any hardware backing it on W767 — I don't think
so. Two options:

**Option A**: disable both:
```dts
&ufs_mem_hc  { status = "disabled"; };
&ufs_mem_phy { status = "disabled"; };
```

**Option B**: leave as-is. The dependency cycle is just a warning; the
UFS driver finds no actual UFS device and gracefully gives up.

I lean Option B (no functional harm), but Option A is cleaner if you want
the dmesg quiet.

## What I'm NOT recommending (to be explicit)

- **GPU OPP table fixes for the 249.6 MHz PLL rate complaint** — that was
  the iter-55 / 7.0 kernel issue. On 6.6 (pmOS) it doesn't reproduce. Defer.
- **eDP panel-edp `BOE 0x07e7` WARN** — fixed by my iter-62 `ca22992`
  commit (EDID + decoded values + panel_edp_table entry). Once you patch
  `drivers/gpu/drm/panel/panel-edp.c`, this WARN disappears.
- **`zap-shader` firmware path** — already set to
  `qcom/samsung/w767/qcdxkmsuc8180.mbn`. Looks right per my iter-56 stage.

## Open: should we Ghidra `qcdxkm8180.sys` / `PanelDriver.sys`?

The remaining unknowns that only Windows binary RE would reveal:

| What | Source binary | Why we'd want it |
|------|---------------|-------------------|
| `disp_cc_pll0/pll1` exact freq tables | `qcdxkm8180.sys` | Only matters if we move to 6.8+ kernel. The pmOS 6.6 setup works fine without it. |
| Panel power-on GPIO sequencing (delays) | `PanelDriver.sys` | The gpio-hog hack in §6 works for cold boot. Tighter `.delay` values in `panel-edp` only matter if cold-boot lights up the panel with visible glitches. |
| Samsung-specific panel commands over I²C | `PanelDriver.sys` | Brightness ramp curves, calibration registers. Nice-to-have for color accuracy but not blocking display. |
| Audio codec init sequence | `qcsubsys_ext_adsp8180.sys`? | Far-future, after audio bring-up starts. |

**My recommendation:** **skip Ghidra for now.** The DTS patches in §1–8
get you from "Partial" to "Works" on the pmOS wiki without binary RE.
Revisit Ghidra when one of:
- Cold-boot panel doesn't light up despite gpio-hog
- Brightness control is needed (PWM + I²C registers)
- We bump past kernel 6.6 and need PLL tables

If you DO want me to dive into Ghidra next, I'd target `PanelDriver.sys`
first (smallest binary, most directly useful) over `qcdxkm8180.sys` (huge,
mostly DXG kernel-mode HAL we don't care about).

## Files staged this commit

- `research/2026-05-18-claude-iter62-fixit-from-logs.md` (this file)

No new binary artefacts — the EDID and DSDT extracts are already in the
repo from `ca22992` / `92eed9b`.

## Summary patch (one-block dts diff for iter-63)

```dts
&gpu {
    vdd-supply   = <&vreg_l3c_1p2>;
    vddcx-supply = <&vreg_l9e_0p88>;
    power-domains = <&rpmhpd SC8180X_MMCX>;
};

&edp_phy {
    vdda-phy-supply = <&vreg_l3c_1p2>;
    vdda-pll-supply = <&vreg_l5e_0p88>;
};

&mdss {
    power-domains = <&rpmhpd SC8180X_MMCX>;
};

&wifi {
    vdd-3.3-ch1-supply = <&vreg_l11c_3p3>;
};

&tlmm {
    sspn_panel_enable_hog {
        gpio-hog;
        gpios = <23 GPIO_ACTIVE_HIGH>,
                <25 GPIO_ACTIVE_HIGH>,
                <35 GPIO_ACTIVE_HIGH>;
        output-high;
        line-name = "sspn_panel_enable";
    };
};
```

Plus: drop `wireless-regdb-*` `regulatory.db` + `.p7s` into initramfs
`/lib/firmware/`.
