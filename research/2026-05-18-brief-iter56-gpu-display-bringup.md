# Brief for brother: W767 Adreno 680 / DPU bring-up details

**For:** brother instance on W767 Win11 ARM64
**Date:** 2026-05-18 (afternoon, after iter-55 hang)
**Trigger:** iter-50→55 Linux progression hung the kernel during msm-drm bind.

## Where we are

iter-49 = full success (keyboard, USB, all 3 DSPs, **wlan0 up**). Last commit `0f194dd`.

iter-50→55 = tried to add display (`DRM_MSM=y`, eDP nodes okay, a630_sqe.fw + a640_gmu.bin staged). Result: drm/msm probes at boot, hangs the kernel before /init runs. We don't even get our wifi log streamer going.

iter-56 (just built) **disables `&gpu`, `&mdss`, `&dispcc`, `&mdss_edp` in DTS** so msm-drm can't bind at all. Should restore iter-49 + add wifi logger. Once wifi logs are streaming we can re-enable display piece-by-piece and watch what actually fails.

## What we've already seen in Linux logs (iter-50→55)

The repeated errors that fly past in the boot photos:

```
panel-simple-dp-aux aux-ae9a000.displayport-controller: Unknown panel
adreno 2c00000.gpu: supply vdd not found, using dummy regulator
adreno 2c00000.gpu: supply vddcx not found, using dummy regulator
adreno 2c00000.gpu: [drm:msm_devfreq_init] *ERROR* Couldn't register OPP table
[drm] Inconclusive highest_bank_bit value: 15 (GPU) vs 16 (UBWC_CTRL)
disp_cc_pll0: Rounded rate 0 not within range [249600000, 249600500]
msm_dpu ae01000.display-controller: [drm:adreno_request_fw] *ERROR* failed to load qcom/a640_gmu.bin: -2
disp_cc_pll1: Rounded rate 0 not within range [249600000, 249600500]
fb0: sys_fillrect: framebuffer is not in virtual address space
fb0: sys_imageblit: framebuffer is not in virtual address space
[deferred-probe backtrace]
```

By iter-53 we'd staged the right firmware names (`a630_sqe.fw` + `a640_gmu.bin` per upstream `a6xx_catalog.c:1093`), so the firmware errors should be fixed — but the kernel still hangs hard.

Likely real root causes:
1. **GPU dummy regulators** — recon `04-pep-vote-map.md` mentioned the GPU needs `vdda-supply` + `vddcx-supply` but the W767 DTS doesn't wire them.
2. **`disp_cc_pll0/pll1` rate-0 rejection** — display clock controller is being asked to round rate 0 and refuses. This might be a frequency-table mismatch (driver expects rates Linux doesn't enumerate).
3. **`Inconclusive highest_bank_bit value: 15 (GPU) vs 16 (UBWC_CTRL)`** — UBWC compression config mismatch between Adreno and UBWC controller; might be the smoking gun.
4. **`fb0: framebuffer not in virtual address space`** — simplefb→msm-drm handoff failing.

## What I need from you (Win11 RE side)

### 1. Adreno GPU regulator votes at boot
Windows boots fine with display + GPU working. Capture the PEP regulator state right after boot — which LDOs are at what voltage when adreno is "running"? We're particularly interested in:
- `LDO9_E` (vreg_l9e_*) — recon thought this was vddcx
- `LDO3_C` (vreg_l3c_1p2) — recon thought this was vdda

Method: Either DSDT walk for `\_SB.GPU0._PR0` (or whatever Adreno's ACPI device is), or runtime check via `pwrtest` / event-log power-vote captures.

### 2. Adreno init sequence in event log
Look at the Windows GPU event log during boot:
```powershell
Get-WinEvent -LogName 'Microsoft-Windows-Kernel-PnP/Configuration' -MaxEvents 50 |
    Where-Object { $_.Message -match 'Adreno|qcdxkms|MDSS|disp_cc' }
```
Or:
```powershell
Get-WinEvent -LogName 'System' -MaxEvents 500 | Where-Object { $_.ProviderName -match 'Display' }
```
What's the **order** of resources Windows turns on? (clocks → regulators → bus probe → firmware load → mode-set)

### 3. dispcc PLL config
Linux is failing to compute a valid rate on `disp_cc_pll0`/`pll1`. We need:
- What clock rate does Windows program these PLLs to at boot for the eDP path?
- Are these PLLs hooked to the eDP TX or to a downstream divider?
- Method: `regdump` / Acpidump of the disp_cc region, or check the qcdxkms*.inf for any documented rates.

### 4. Samsung-specific quirks
- Does the `oem19.inf` (SAM0101 PanelDriver) have any `[QcEventCallout]` or DSDT method that runs before display init? (e.g., a panel power-on sequence that toggles GPIO/regulator before eDP TX bring-up)
- Is there a panel `_DSD` Method that exposes power rail names?

### 5. Optional but useful — capture the dispcc register dump
```powershell
# If you can find a way to read the dispcc MMIO base (sc8180x: 0x0AF00000) at boot
# Even a partial dump would tell us what the PLL config registers look like
```

## What you DON'T need to dig into (we have these)

- WiFi: iter-49 proved end-to-end working. Wifi logger initramfs (iter-54+) handles all future debugging.
- ADSP/CDSP/MPSS: working since iter-31/41/49.
- USB: working since iter-40.
- Keyboard: working since iter-28.
- DSP firmware paths: all staged in `firmware-stage-w767/`.

## When you write back

Put your reply at `research/2026-05-18-claude-iter56-gpu-bringup-answers.md`, commit, push. We'll iterate from there.

## Linux side meanwhile

iter-56 boots a known-good config (display disabled) + the wifi log streamer. Once boot succeeds and logs flow to fedora workstation (192.168.1.73:9999), we can re-enable display in iter-57+ and SEE what actually fails — no more flying blind on camera photos.
