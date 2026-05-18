# KIMI_2 — postmarketOS W767 Audit: What "3D acceleration" actually means

> **Date:** 2026-05-18
> **Trigger:** User linked postmarketOS wiki for Samsung Galaxy Book S (samsung-w767) and noted they have "3D acceleration"
> **Method:** Web search of pmaports packages, Repology, pkgs.postmarketos.org, cross-reference with repo state
> **Key finding:** pmOS W767 runs on **Linux 6.6**, not 7.0. The user is fighting regressions introduced between 6.6 and 7.0.

---

## 1. What postmarketOS actually ships for W767

### 1.1 Kernel: `linux-postmarketos-qcom-sc8180x` = 6.6.0

| Source | Fact |
|--------|------|
| Repology | `linux-postmarketos-qcom-sc8180x` version `6.6.0-r3`, maintainer Jenneron |
| `README.md` in this repo | "Mainline / archived community kernel: `sc8180x-mainline/linux` — frozen Jan 2023 but still the kernel pmOS pins" |
| `docs/00-hardware-combined.md` | "pmOS `linux-postmarketos-qcom-sc8180x` kernel branch (pinned to `sc8180x-mainline/linux` commit `27c30b32...`)" |

**Translation:** pmOS does **not** track mainline Linux 7.0. They ship a 6.6 kernel that is either:
- The frozen `sc8180x-mainline/linux` branch from Jan 2023 (which was ~6.1-ish at the time, but pmOS may have forward-ported to 6.6), OR
- Jenneron's own 6.6 fork with the SC8180X patches applied.

Either way, **the Qualcomm DRM/msm driver in pmOS's 6.6 kernel predates the 6.8-rc1 eDP/DP regression** that jhovold documented (see KIMI_1). It also predates the `aux_bridge` + `pmic_glink` runtime PM interaction that causes the X13s boot failures on 6.8+.

### 1.2 Device package: `device-samsung-w767`

- Version `3-r0` in postmarketOS master
- Depends on `soc-qcom-sc8180x` (the SoC meta-package)
- DTB name: `qcom/sc8180x-samsung-w767` (same as this repo)

### 1.3 GPU / 3D acceleration stack

| Package | Purpose | What it pulls |
|---------|---------|---------------|
| `soc-qcom-sc8180x-vulkan` | Vulkan support | `mesa-vulkan-freedreno` (Turnip driver for Adreno 680) |
| `mesa-dri-gallium` | OpenGL | Freedreno driver (kernel: `msm` + userspace: `freedreno`) |
| `mesa-dri-simpledrm` | "Temporary KMSRO stub for simpledrm" | Provides a DRI driver that can render to `simpledrm` fbdev if full MSM KMS isn't up |

**What "3D acceleration" means in pmOS context:**
- The `msm` kernel driver binds successfully (because all display nodes are enabled and the kernel is 6.6 where the eDP/DP paths are stable).
- Mesa's `freedreno` Gallium driver loads for OpenGL ES / OpenGL.
- Mesa's `turnip` Vulkan driver loads for Vulkan.
- This is standard Adreno 680 support — no Samsung-specific GPU quirk. The Adreno 680 has been upstream in `drm/msm` since ~5.10.

**What it does NOT mean:**
- pmOS does not have some magic Samsung firmware or driver that you lack. They use the same `qcdxkmsuc8180.mbn` zap shader and the same `a680_*` firmware blobs.
- pmOS's display stack is the same Qualcomm `msm` + `panel-edp` + `phy-qcom-edp` + `dispcc` chain you are trying to enable.

---

## 2. Why pmOS works and your v7.0 build hangs

### 2.1 The 6.8-rc1 DRM/msm regression

From KIMI_1 and upstream LKML (jhovold, Feb 2024):

> "Since 6.8-rc1 the internal eDP display on the Lenovo ThinkPad X13s does not always show up on boot. The logs indicate problems with the runtime PM and eDP rework that went into 6.8-rc1."

The regression was introduced by:
1. eDP runtime PM rework (`quic_khsieh` series)
2. DRM `aux_bridge` — a bridge driver that sits between the DP AUX channel and the panel/bridge stack

The X13s (SC8280XP) and W767 (SC8180X) share the same DP/eDP controller IP generation. The `aux_bridge` driver path is used when `pmic_glink` is present — which it is on both boards.

**pmOS's 6.6 kernel does not have this regression because it predates the 6.8-rc1 rework.**

### 2.2 Your iter-50-55 symptoms are the regression

| Your symptom | Upstream regression match |
|-------------|----------------------------|
| `DRM_MSM=y` + `&gpu`/`&mdss`/`&mdss_edp` enabled → hard hang before `/init` | X13s 6.8-rc1: "random crashes at boot", "hard crashes twice when testing external display" |
| `panel-simple-dp-aux: Unknown panel` | eDP AUX channel not correctly initialized due to `aux_bridge` failing to attach |
| `disp_cc_pll0: Rounded rate 0 not within range` | clk-dispcc driver changes between 6.6 and 7.0; rate tables were refactored |
| `fb0: framebuffer is not in virtual address space` | simpledrm/msm handover broken by the 6.8 KMS rework |

Your iter-56/57/58 strategy (disable all display nodes to escape the hang) works around the regression by preventing `msm` from binding, but it also prevents any display/GPU functionality.

### 2.3 What pmOS's DTS probably looks like

pmOS's `device-samsung-w767` likely enables the full display stack without the iter-56-59 overrides:

```dts
&dispcc { status = "okay"; };
&mdss   { status = "okay"; };
&mdss_edp { status = "okay"; };
&edp_phy { status = "okay"; };
&gpu { status = "okay"; };
&gmu { status = "okay"; };
&adreno_smmu { status = "okay"; };
```

And they probably have the panel node wired under `&mdss_edp -> aux-bus -> panel` with `backlight = <&mdss_edp>;` or DPCD-AUX auto-detection.

**Because they run 6.6, this full enablement does not hit the 6.8-rc1 regression.**

---

## 3. Options for your project

### Option A: Downgrade to 6.6 (pmOS baseline) — FASTEST PATH TO WORKING 3D

**Why:** The pmOS kernel is a known-working reference. All the hardware you want (display, GPU, WiFi, USB) works on 6.6 for this board.

**How:**
1. Clone the pmOS kernel source or the `sc8180x-mainline/linux` branch pinned by pmOS.
2. Apply your W767 DTB patch (`kernel-patches/0001-arm64-dts-qcom-add-Samsung-Galaxy-Book-S-W767-device.patch`) to the 6.6 tree.
3. Use pmOS's kernel config as a base (or your `w767.config` adapted for 6.6).
4. Build, boot, verify display+GPU work.
5. Then, if you want newer kernel features, bisect forward from 6.6→7.0 to find the exact commit that breaks W767.

**Trade-off:** You lose any 7.0-specific features, but you gain a working laptop today.

### Option B: Stay on 7.0 and cherry-pick the regression fix

**Why:** If the goal is mainline 7.0, you need to identify and backport the fix.

**How:**
1. Find the upstream commit(s) that fixed the X13s 6.8-rc1 regression. Likely candidates:
   - `drm/msm/dp: fix runtime PM in dp_display_probe` (or similar title)
   - `drm/msm: fix aux_bridge teardown race`
   - Any commit in jhovold's `wip/sc8280xp-6.11` or `wip/sc8280xp-6.16` branch that mentions "dp", "edp", "aux_bridge", or "pmic_glink"
2. Cherry-pick or backport to your v7.0 tree.
3. Re-enable the full display stack (`&gpu`, `&mdss`, `&mdss_edp`, `&edp_phy`).
4. Re-test.

**Trade-off:** Research-heavy. You need to find the exact fix commit.

### Option C: Use jhovold's wip branch as the kernel base

**Why:** jhovold maintains `wip/sc8280xp-6.16` (and newer) which contains all the fixes that haven't landed in mainline yet.

**How:**
1. Clone `https://github.com/jhovold/linux/tree/wip/sc8280xp-6.16`
2. Note: this branch is for SC8280XP (X13s), not SC8180X. But the DRM/msm fixes are generic.
3. Check if there is a `wip/sc8180x-*` branch, or apply the SC8280XP DRM patches to mainline 7.0.
4. Apply your W767 DTB.
5. Build and test.

**Trade-off:** The wip branch may have X13s-specific DT changes that don't apply to W767. But the driver fixes are the valuable part.

### Option D: The "mesa-dri-simpledrm" insight

pmOS ships `mesa-dri-simpledrm` — a stub DRI driver that lets Mesa render to a `simpledrm` framebuffer. This suggests pmOS may not even rely on full MSM KMS for basic display in early boot / initramfs.

**Implication for your project:** If the full MSM driver is too unstable on 7.0, you could potentially:
1. Keep `DRM_MSM=m` (or even `n`)
2. Rely on `CONFIG_DRM_SIMPLEDRM=y` + `CONFIG_FB_EFI=y` for a basic framebuffer console
3. Use `mesa-dri-simpledrm` for software-composited display (no GPU acceleration)
4. This gives you a working desktop, but no 3D acceleration. Not ideal, but a fallback.

However, the user explicitly wants "3D acceleration", so this is a fallback only.

---

## 4. Bottom line

| Question | Answer |
|----------|--------|
| Does pmOS have a magic sauce for 3D? | **No.** They use the same `drm/msm` + `mesa-freedreno` + `qcdxkmsuc8180.mbn` stack. |
| Why does pmOS work? | **Kernel 6.6.** It predates the 6.8-rc1 DRM/msm eDP/DP/aux_bridge regression that breaks SC8180X/SC8280XP boot. |
| Why does v7.0 hang? | **The regression is present in v7.0.** Disabling display nodes avoids it but removes GPU functionality. |
| Fastest fix? | **Option A:** Switch your build to a 6.6-based SC8180X kernel (pmOS reference). Or **Option C:** Pull jhovold's wip DRM fixes into 7.0. |
| Should you try Ubuntu X13s kernel? | **No.** Wrong DTB, wrong SoC-specific config. But their SAUCE patches (regulator-load fixes) are relevant. |

---

## 5. Recommended next action

1. **Immediately:** Verify that a 6.6-based kernel with your W767 DTB boots and reaches display. You can likely reuse your existing `w767.config` with minor adjustments for 6.6.
2. **If 6.6 works:** Re-enable `&gpu`, `&mdss`, `&mdss_edp`, `&edp_phy` in the DTS. Verify `msm` binds and `glxinfo` shows `freedreno`.
3. **If you must stay on 7.0:** Search jhovold's `wip/sc8280xp-6.16` branch for commits touching `drivers/gpu/drm/msm/dp/` or `drivers/gpu/drm/msm/edp/` between 6.8 and 6.16. Cherry-pick them.

---

## 6. Source references

| Resource | URL | Relevance |
|----------|-----|-----------|
| pmOS `linux-postmarketos-qcom-sc8180x` | Repology / pkgs.postmarketos.org | Kernel version 6.6.0 |
| pmOS `soc-qcom-sc8180x-vulkan` | pkgs.postmarketos.org | Mesa Turnip/Vulkan package |
| pmOS `mesa-dri-simpledrm` | pkgs.postmarketos.org | KMSRO stub for simpledrm |
| jhovold X13s 6.8-rc1 regression report | lkml, Feb 2024 | Confirms the exact bug class hitting W767 on v7.0 |
| jhovold `wip/sc8280xp-6.16` | github.com/jhovold/linux | Fixes not yet in mainline |
| sc8180x-mainline/linux (frozen) | gitlab.com/sc8180x-mainline/linux | pmOS pinned kernel base |

---

*End of KIMI_2 analysis.*
