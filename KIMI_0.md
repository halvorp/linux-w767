# KIMI_0 — W767 iter-58 Hang Analysis

> **Date:** 2026-05-18
> **Trigger:** User asked for independent take on iter-58 silent hang after 12.93s
> **Method:** Read repo state, git history, DTS, kernel config, research docs; reached independent conclusions; do not trust user's prior diagnosis.

---

## 1. What I Read (Source Trail)

To form an independent view, I consumed the following artifacts in order:

1. **`README.md`** — Baseline status table: display+GPU = ✅ working in iter-17, but iter-50-55 regressed. iter-56-58 are the "display disabled" recovery attempts.
2. **Git log (`git log --oneline -20`)** — Shows the commit chain:
   - `0f194dd` iter-49 = LAST KNOWN WORKING (keyboard, USB, all 3 DSPs, wlan0 up)
   - `3fa38dd` iter-56 = disable gpu/mdss/dispcc/mdss_edp (to escape iter-50-55 hang)
   - `92eed9b` iter-56 = Windows-side DSDT dossier for GPU/display bring-up
   - `f33c11e` iter-57 = also disable &adreno_smmu and &gmu (iter-56 still hung in SMMU probe)
   - `bc6de34` iter-58 = re-enable &dispcc (clock controller only, no display consumers)
3. **`dts/sc8180x-samsung-w767.dts`** (iter-58 HEAD) — The live device tree:
   - `&dispcc { status = "okay"; };` ← iter-58 change
   - `&gpu { status = "disabled"; };`
   - `&adreno_smmu { status = "disabled"; };`
   - `&gmu { status = "disabled"; };`
   - `&mdss { status = "disabled"; };`
   - `&mdss_edp { status = "disabled"; };`
   - `&edp_phy` — **NOT explicitly disabled** (inherits from `sc8180x.dtsi`)
   - `&edp_phy` has clocks wired to `&dispcc` DISP_CC_MDSS_EDP_AUX_CLK / AHB_CLK / `&edp_ref_clk`
   - `&remoteproc_mpss { status = "okay"; }` — MPSS enabled
   - `&wifi { status = "okay"; }` — ath10k_snoc enabled
   - USB stack: `&usb_mp`, `&usb_prim`, `&usb_sec` all okay with QMP combo PHYs
   - `pmic-glink` at root with connector@0/1, no orientation-switch
4. **`w767-os/kernel/w767.config`** — Kernel config fragment:
   - `CONFIG_DRM_MSM=y` ← **This was `=m` in iter-49 (per iter-56 research)**
   - `CONFIG_DRM_MSM_DP=y`
   - `CONFIG_DRM_PANEL_EDP=y`
   - `CONFIG_PHY_QCOM_EDP=y`
   - `CONFIG_PHY_QCOM_QMP_COMBO=y`
   - `CONFIG_PSTORE_RAM=y` — ramoops backend present
   - `CONFIG_BT_HCIUART_QCA=y`
   - WiFi stack = modular (`CONFIG_ATH10K=m`, `CONFIG_ATH10K_SNOC=m`)
5. **`research/2026-05-18-reply-to-brother-iter56-still-hangs.md`** — iter-56 post-mortem:
   - iter-56 hung in `arm_smmu_device_reset()` at `2ca0000.iommu` because `&adreno_smmu` was still enabled while `&gpu` was disabled → GPU power island half-initialized.
   - iter-57 fix = disable `&adreno_smmu` + `&gmu`.
   - Key quote: "iter-49 had the same DTS state for `&adreno_smmu` (status=okay default) and `&gmu`, AND `&gpu` was status=okay. The arm-smmu driver also probed the Adreno SMMU then. Two differences plausibly mask the hang: (1) DRM_MSM was `=m` and never loaded, (2) `&gpu` being okay meant the genpd consumer count on `GPU_CX_GDSC` matched what the bootloader/UEFI/hyp expected."
6. **`research/2026-05-18-claude-iter56-gpu-bringup-answers.md`** — Windows-side RE dossier:
   - GPU needs `vdda-supply = LDO3_C`, `vddcx-supply = LDO9_E`
   - `disp_cc_pll0/pll1` rates hardcoded in Windows driver; MDP core clock PSTATE table discrete rates: 460/345/300/200/171.5/150/100/85.5/19.2/0 MHz
   - `249.6 MHz` (the Linux error) is **not in Windows table** → upstream `clk-disp-cc-sc8180x.c` freq_tbl likely missing the W767 eDP path
7. **`research/2026-05-18-brief-iter56-gpu-display-bringup.md`** — The brief that triggered the dossier:
   - iter-49 = full success
   - iter-50→55 = `DRM_MSM=y`, eDP nodes okay, firmware staged → hangs before `/init`
   - iter-56 strategy = disable display completely, get wifi logger working, re-enable piece-by-piece
8. **GRUB configs (`w767-phase1.conf`, `w767-phase2.conf`)** — Confirm cmdline includes `earlycon=efifb keep_bootcon`, `clk_ignore_unused`, `pd_ignore_unused`, `efi=noruntime`, etc.

---

## 2. The User's Current Symptom (iter-58)

From the prompt:
- Kernel reaches ~12.93s uptime with `initcall_debug` + `loglevel=8`
- After 12.93s: **efifb goes completely BLACK**, USB host LED stops blinking
- No new MAC joins LAN (wifi logger never comes up)
- 5+ minutes = total silent hang
- `video=efifb:nodefer fbcon=map:0` did not change behavior
- `nosimpledrm` is not a real kernel param (user noted)

User's belief:
- iter-57 disabled Adreno power island cleanly
- iter-58 re-enabled `&dispcc` so USB-C combo phys could resolve clocks
- DRM_MSM=y was the culprit in iter-50-55, but maybe diagnosis was incomplete

---

## 3. Independent Analysis — Question by Question

### A. Is the screen blanking normal efifb retirement, or a kernel death?

**Conclusion: Kernel death. Not normal console handoff.**

Reasoning:
- `keep_bootcon` is on the cmdline. This explicitly prevents the earlycon from unregistering at `console_init()` time. The EFI framebuffer console should persist until something *else* takes over—or indefinitely.
- Even if `simpledrm` or `fbcon` shadowed the console, the USB host LED stopping is independent of display state. A blinking LED is a userspace or kernel-timer activity; if it stops, scheduling has ceased.
- A normal efifb→simpledrm handoff on arm64 produces a *brief* flicker or black gap (~100-500ms) then the new framebuffer paints. It does not produce a permanent black screen with a dead USB LED.
- The combo of black screen + dead LED = either (a) a hard hang in atomic context, (b) an NMI watchdog firing silently (user has `panic=0`, so it might just halt), or (c) a CPU stall that RCU suppressed (`rcupdate.rcu_cpu_stall_suppress=1` is on the cmdline—this hides the stack trace).

**On the `CONFIG_DRM_SIMPLEDRM` + `CONFIG_FB_EFI` + `CONFIG_SYSFB` triangle:**
They *can* fight in the following way: if `SYSFB` + `FB_EFI` registers `fb0` from the EFI GOP info, and then `DRM_SIMPLEDRM` also registers from the same UEFI framebuffer, you get two framebuffers backing the same memory. When `DRM_KMS_HELPER` later tries to take over, it may tear down `simpledrm`, which tears down `fbcon`, but if the real DRM master (`msm`) never binds because its nodes are disabled, there's nothing left to repaint. However, this alone would still leave the kernel *running* (you'd just have no console). The dead USB LED rules this out as the sole explanation.

**Verdict:** The black screen is a *symptom* of the same hang that killed the USB LED. It is not the root cause.

### B. Does DRM_MSM=y still touch anything dangerous with all display nodes disabled?

**Conclusion: Yes. The built-in DRM_MSM infrastructure enables PHY and panel drivers that can now probe because `&dispcc` is on.**

Reasoning:
- `CONFIG_DRM_MSM=y` pulls in `drivers/gpu/drm/msm/msm_drv.c`, which has `module_init(msm_drm_init)`. This registers platform drivers for `msm`, `dpu`, `mdss_dp`, `edp`, etc.
- More importantly, `CONFIG_DRM_MSM_DP=y` and `CONFIG_PHY_QCOM_EDP=y` register the `qcom,sc8180x-edp-phy` driver.
- In the DTS, `&edp_phy` is **not** explicitly disabled. If `sc8180x.dtsi` sets it to `status = "okay"` by default, then iter-58 is the first time it has resolvable clocks (because `&dispcc` is now on).
- The `phy-qcom-edp` driver (`drivers/phy/qualcomm/phy-qcom-edp.c`) performs PLL calibration, TX lane initialization, and AUX channel setup at probe time. These operations involve register polls with timeouts. If the eDP controller (`&mdss_edp`) is disabled, there is no DPU to coordinate the power-domain sequencing (GDSC footswitch, interconnect vote, MMCX corner). The PHY may spin waiting for a hardware state that never arrives.
- Additionally, `CONFIG_DRM_PANEL_EDP=y` registers the `panel-edp` driver. The `&mdss_edp` node in the DTS still has its `aux-bus { panel { compatible = "edp-panel"; } }` child, but the parent `&mdss_edp` is disabled. The panel driver should not probe. But if there is a bug in the panel or aux-bus core that probes regardless, it could try to do DPCD AUX reads over a dead controller.

**Key distinction from iter-49:**
- iter-49 had `DRM_MSM=m` AND `&gpu`/`&mdss`/`&mdss_edp` enabled. The module was never loaded, so no MSM code ran at all. The display nodes being enabled in the DTS was harmless because the matching platform drivers weren't registered.
- iter-58 has `DRM_MSM=y` AND `&dispcc` enabled. The platform drivers are registered at boot. Even though `&gpu`/`&mdss`/`&mdss_edp` are disabled, `&edp_phy` may still be enabled by default in the DTSI. The PHY driver now runs and does hardware pokes.

### C. What probes typically run after 12.93s, and which are landmines?

The boot timeline at ~12.93s with `initcall_debug` usually corresponds to:
- Post-core_initcall / device_initcall phase
- Deferred probe workqueue starts clearing backlog
- Platform drivers for USB, remoteproc, PHY, DRM, WiFi begin binding

**Specific landmines on SC8180X mainline:**

1. **`phy-qcom-edp` (`&edp_phy`)** — **HIGHEST SUSPICION**
   - Previously `-EPROBE_DEFER` because `&dispcc` was off (clocks unresolved).
   - Now `&dispcc` is on, clocks resolve, probe proceeds.
   - Driver does `edp_phy_init()` → `edp_phy_lane_init()` → PLL lock polls.
   - Without the DPU (`&mdss` disabled) coordinating `mdss_core_gdsc`, the PHY hardware may be in a bad power state. Register polls can hang the CPU bus if the clock/reset domain is gated.
   - Evidence: iter-56 research mentions `88ec000.phy` and `88e2-4000.phy` as USB-C combo phys that were returning `-517` (deferred probe) when `dispcc` was disabled. With `dispcc` on, they resolve. The eDP PHY (`aec2a00`) is in the same clock domain.

2. **`qcom-qmp-combo-phy` (USB-C DP lanes)** — **SECOND HIGHEST**
   - The QMP combo driver manages both USB3 and DisplayPort lanes.
   - With `dispcc` enabled, the DP side may now try to power up.
   - The `aux_bridge` errors seen in iter-56 (`aux_bridge.aux_bridge.0: error -19: failed to acquire drm_bridge`) were expected with `mdss_edp` disabled. But in iter-58, the combo PHY driver may proceed further and attempt DP lane training or AUX bridge attachment. If it encounters a hardware state mismatch, it can hang.
   - The user noted the five phys returning `-517` on iter-57 now resolve. Resolving means they are *probing now*.

3. **`qcom_q6v5_mss` (MPSS / modem)** — **MODERATE SUSPICION**
   - MPSS firmware load (`qcmpss8180_XEF.mbn`) involves PIL (Peripheral Image Loader) authentication, which may interact with the secure world (TZ).
   - If the MPSS image has a mismatch or the IPA (interconnect) isn't ready, the remoteproc can wedge in a polling loop.
   - However, MPSS was working in iter-49, and the DTS memory region and firmware path are unchanged. Unlikely to be the *new* failure unless `&dispcc` re-enabling changed the interconnect bandwidth vote in a way that starves MPSS.

4. **`ath10k_snoc` (WiFi)** — **LOW SUSPICION**
   - The driver itself is a modular module (`CONFIG_ATH10K_SNOC=m`). It won't load until userspace or initramfs triggers it. If the hang is at 12.93s, this is probably before `modprobe ath10k_snoc`.
   - However, the platform device `&wifi` is enabled. The core `ath10k` driver does not probe at boot unless requested. Safe.

5. **`pmic-glink` / UCSI** — **LOW**
   - The root `pmic-glink` node in the DTS has connectors with no `orientation-switch`. This configuration was intentionally simplified in iter-33 to avoid deferred probe loops.
   - The `ucsi-qcom` or `qcom-typec` drivers should not hang; they may log warnings but won't hard-lock.

6. **`dwc3-qcom` / `xhci-plat`** — **MODERATE**
   - USB3 host controllers. They depend on the QMP PHYs. If the QMP PHY probe hangs, dwc3 hangs in deferred probe. But dwc3 itself has robust error paths.
   - The `usb_mp` controller (internal MCU keyboard) was the focus of iter-26/27/28. It works. `usb_prim`/`usb_sec` were also stabilized by iter-40.

**Summary of landmine ranking:**
| Rank | Driver | Why it's dangerous now |
|------|--------|----------------------|
| 1 | `phy-qcom-edp` | `&edp_phy` likely enabled by default in DTSI; now has clocks from `&dispcc`; probes and pokes hardware without DPU coordination |
| 2 | `qcom-qmp-combo-phy` (DP side) | DP lanes now power up because `dispcc` resolves; may hang waiting for `drm_bridge` or AUX state |
| 3 | `qcom_q6v5_mss` | PIL load can wedge if SoC state changed due to `dispcc` re-enabling |
| 4 | `dwc3-qcom` | Hangs only if PHY above hangs first |

### D. Escape hatches for output past the screen blank

**Existing mechanisms:**

1. **ramoops (`CONFIG_PSTORE_RAM=y`)** — **BEST POST-MORTEM**
   - Region at `0x9b500000` (1 MiB), reserved-memory node present.
   - With `CONFIG_PSTORE_CONSOLE=y`, the kernel log is mirrored into this RAM region.
   - After the hang, force a hard reset (hold power button). Boot any kernel that can mount pstore.
   - `mount -t pstore none /sys/fs/pstore; cat /sys/fs/pstore/console-ramoops-0`
   - **Caveat:** If the hang is a bus stall inside an atomic context, `printk()` may not flush to the pstore buffer. But the `pstore` console backend uses a ring buffer that is updated synchronously on each `printk`, so it usually survives.

2. **Remove `rcupdate.rcu_cpu_stall_suppress=1`** — **HIGHEST DIAGNOSTIC VALUE**
   - This flag is currently hiding the stack trace. In iter-56, the research explicitly noted: "deferred_probe in a tight loop, starved RCU, and silently NMI'd the kernel."
   - If the same thing is happening in iter-58, dropping this flag will produce:
     ```
     INFO: rcu_sched self-detected stall on CPU
     ... stack trace pointing to the offending driver
     ```
   - The user has `panic=0`. If an RCU stall fires, with `panic=0` the kernel just prints and keeps going (or halts if it's too bad). Better: set `panic=10` so it reboots into ramoops capture.

3. **netconsole over USB-Ethernet** — **UNLIKELY TO HELP**
   - Requires USB host to be fully up and a dongle enumerated. If the hang is at 12.93s, USB may not have finished probe yet. Not a reliable escape hatch.

4. **netconsole over wlan0** — **IMPOSSIBLE**
   - `ath10k_snoc` is a module. Not loaded at 12.93s.

5. **Serial console (`console=ttyAMA0` or `ttyMSM0`)** — **UNAVAILABLE**
   - The W767 has no exposed serial header. UART13 is Bluetooth.

**Recommended cmdline tweak for next boot:**
```
# Remove:
rcupdate.rcu_cpu_stall_suppress=1
panic=0

# Add:
panic=10
```
This makes an RCU stall or NMI watchdog trigger a reboot in 10s, dumping to ramoops.

### E. ONE next thing to change

**Make `DRM_MSM=m` again.**

**Why:**
- iter-49 had `DRM_MSM=m`. Everything worked (USB, DSPs, WiFi, keyboard).
- iter-50-55 changed it to `DRM_MSM=y` and display nodes were enabled. It hung.
- iter-56-58 kept `DRM_MSM=y` and disabled display nodes to escape the hang. But the hang persists (iter-58).
- The salient difference between iter-49 (works) and iter-58 (hangs) is **not** `&dispcc` being on/off—it's that iter-49 had `DRM_MSM=m`, so the MSM driver infrastructure never registered at boot. iter-58 has `DRM_MSM=y`, so the MSM infrastructure registers, and now that `&dispcc` is on, the eDP PHY (and possibly DP infrastructure) attempts hardware pokes without the DPU being there to coordinate.
- If we switch back to `DRM_MSM=m`, we restore the iter-49 state: no MSM code runs at boot, no PHY pokes, no DP infrastructure probing. The USB, DSP, and WiFi bring-up should proceed exactly as before.
- Once the system boots and the wifi logger streams, we can `modprobe msm` *after* init and watch exactly where it hangs. This gives us the iter-57+ display bring-up ladder the research already laid out, but with live logs.

**Secondary option (if user insists on keeping `DRM_MSM=y`):**
Explicitly disable `&edp_phy` in the DTS:
```dts
&edp_phy {
    status = "disabled";
};
```
This prevents the `phy-qcom-edp` driver from probing, which is the most likely direct hang source. But `DRM_MSM=m` is the more complete and diagnostic approach.

---

## 4. Independent Conclusions (do not trust user's)

| User's belief | Independent assessment |
|-------------|------------------------|
| "iter-57 cleanly disabled the Adreno power island" | **True** — disabling `&gpu` + `&gmu` + `&adreno_smmu` does take the island offline. iter-56 post-mortem confirms this. |
| "iter-58 re-enabled dispcc so USB-C combo phys could get clocks" | **Partially true but misleading** — `&dispcc` also feeds `&edp_phy` and the display PHY wrapper. The USB-C combo phys resolving their clocks is a side effect, not the only effect. |
| "DRM_MSM=y was what hung iter-50-55, but maybe diagnosis was incomplete" | **Diagnosis was incomplete** — `DRM_MSM=y` *with all display consumers enabled* hung because the DPU tried to bind and failed on PLL/rate/GPU supplies. But `DRM_MSM=y` *with display consumers disabled* is now hanging because the **PHY/subsystem init path** (edp_phy, dp core) still probes and pokes hardware without the DPU to coordinate power. The root cause is the same: MSM built-in infrastructure assumes a coordinated display stack. Removing the consumers but keeping the framework is like removing the engine but keeping the starter motor engaged. |
| "`video=efifb:nodefer` didn't change anything" | **Expected** — this parameter is for fbcon deferred takeover, not for preventing kernel hangs. The black screen is a symptom, not the disease. |

---

## 5. Actionable Prescription for iter-59

1. **In `w767-os/kernel/w767.config`:**
   ```diff
   -CONFIG_DRM_MSM=y
   +CONFIG_DRM_MSM=m
   ```
   Keep `CONFIG_DRM_MSM_DP=y` and `CONFIG_PHY_QCOM_EDP=y` as built-in or modular as needed; the key is `DRM_MSM=m` so `msm_drv.c` doesn't register at `module_init`.

2. **In cmdline (for the boot attempt):**
   ```diff
   -rcupdate.rcu_cpu_stall_suppress=1 panic=0
   +panic=10
   ```
   This ensures that if there is *still* a hang from another source, we get an RCU stall stack trace or a watchdog reboot into ramoops.

3. **Do NOT change the DTS.** Keep `&dispcc` enabled, `&gpu`/`&mdss`/`&mdss_edp`/`&gmu`/`&adreno_smmu` disabled. We want to isolate the `DRM_MSM=y` → `=m` delta.

4. **Expected outcome:**
   - Boot reaches `/init`
   - WiFi logger starts streaming
   - USB host LED keeps blinking
   - Screen may show the boot log until `fbcon` takes over, then either stay on `fb0` (if `simpledrm` registers) or go black if there's no DRM master. But the system should be *alive*.

5. **Next step after success:**
   - `modprobe msm` from the shell after boot.
   - Watch `dmesg` in real time (via the wifi logger) to see exactly which MSM sub-driver hangs.
   - Then iterate the display bring-up ladder from the research (iter-57→62 renumbered) with live telemetry.

---

## 6. What I Did NOT Verify

- I did not check the exact `sc8180x.dtsi` upstream default for `&edp_phy` status. If it is `disabled` by default, then `phy-qcom-edp` is not the hang source, and the QMP combo PHY DP side or something else (MPSS, dwc3-qcom) is more likely. But `DRM_MSM=m` is still the correct first step regardless, because it removes the largest change from the known-working state.
- I did not check whether `CONFIG_DRM_SIMPLEDRM` alone (independent of MSM) could cause the black screen if it registers and then there's no DRM master. This is a plausible secondary effect but would not explain the dead USB LED.
- I did not verify the upstream linux-msm/linux-next state for SC8180X. The user asked about recent upstream patches; I have no live web access in this turn to check the exact state of `clk-disp-cc-sc8180x.c` in `linux-next`. My analysis is based on the repo's iter-56 research, which documented the rate-0/249.6 MHz bug. If upstream has since fixed it, the display bring-up ladder will be easier—but it doesn't change the iter-58 hang diagnosis.

---

## 7. Bottom Line

The iter-58 hang is almost certainly caused by `DRM_MSM=y` enabling the MSM display infrastructure to probe hardware (especially the eDP PHY and DP core) at boot, while the coordinating DPU/GPU nodes are disabled. This leaves the PHY in an uncoordinated power state where register polls can bus-stall the CPU.

**The fix is to restore `DRM_MSM=m` (the iter-49 state), boot to a working shell with wifi logs, then `modprobe msm` under live observation to identify the exact sub-driver that needs patching.**

---

*End of KIMI_0 analysis.*
