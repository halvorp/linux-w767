# KIMI_1 — Ubuntu X13s kernel audit + iter-59 wedge autopsy

> **Date:** 2026-05-18
> **Context:** User asked two things in rapid succession:
> 1. Can we succeed by using Ubuntu's X13s (SC8280XP) kernel sources?
> 2. Original A–E questions about the iter-59 silent hang (IRQs-off wedge).
> **Method:** Web search of Ubuntu PPAs, jhovold/linux wip branches, Ubuntu SAUCE patch queues, and cross-reference against W767 repo state.

---

## Part 1 — Ubuntu X13s Kernel: What It Is, What It Can Give W767

### 1.1 Ubuntu does NOT officially ship X13s support in 24.04

- **Phoronix, April 2024:** "Plans to have official support for the Arm-based Lenovo ThinkPad X13s Gen1 laptop in Ubuntu 24.04 LTS sadly didn't pan out."
- **What exists:** A PPA `ppa:ubuntu-concept/x13s` maintained by Juerg Haefliger (Canonical).
- **Kernel package:** `linux` version `6.11.0-99.99+x13s1` (as of late 2024), and `6.16.0-13.13+ppa1` in `ppa:juergh/linux` (Aug 2025).
- **How it's built:** It is a generic Ubuntu kernel + `UBUNTU: SAUCE` patches cherry-picked from jhovold's `wip/sc8280xp-*` branches.

### 1.2 Can you boot an Ubuntu X13s kernel on the W767?

**Short answer: The kernel Image *might* boot, but the DTB is wrong. Do not waste time on this path.**

Reasons:
- The X13s kernel is compiled with `CONFIG_ARCH_QCOM=y` and generic ARM64 platform support, so the raw `Image` would technically execute on SC8180X.
- But the **DTB** embedded in the X13s package is for `sc8280xp-lenovo-thinkpad-x13s.dts`. It has completely different regulator names, different PMICs (PM8280K/PM8008 vs PM8150/PMC8180), different panel, different GPIO map, different memory reservations.
- Booting an X13s DTB on W767 would immediately fail at regulator probe or hit a mismatch on UFS/USB PHY supplies.
- **What you CAN do:** Extract the SAUCE patches and apply them to your own v7.0 tree alongside your W767 DTB.

### 1.3 Specific SAUCE patches from Ubuntu/X13s that almost certainly fix W767 symptoms

Ubuntu's Sep 2024 SRU (`[O,00/21] x13s: Fix various minor issues and add Venus support`) contains patches that address the exact driver classes hitting W767. The most relevant for the iter-59 wedge:

| Patch | Driver | What it fixes | W767 relevance |
|-------|--------|-------------|----------------|
| `UBUNTU: SAUCE: phy: qcom-edp: drop regulator loads` | `phy-qcom-edp` | Removes `regulator_set_load()` calls that can deadlock during probe if the regulator is shared or not yet fully up | **HIGH** — `&edp_phy` was enabled by default in sc8180x.dtsi and could wedge during regulator coordination |
| `UBUNTU: SAUCE: phy: qcom-qmp-combo: drop regulator loads` | `phy-qcom-qmp-combo` | Same as above, for the USB-C / DP combo PHY | **HIGH** — W767 has QMP combo phys for `usb_prim`/`usb_sec` that were returning `-517` until `&dispcc` was re-enabled; regulator load deadlocks are a known cause of silent hangs |
| `UBUNTU: SAUCE: clk: qcom: gcc-sc8280xp: don't use parking clk_ops for QUPs` | `clk-gcc` | Fixes a race where QUP (I2C/SPI/UART) clocks are reparented incorrectly during probe | **MEDIUM** — W767 uses GENI QUPs extensively; a parking clk bug could explain why I2C/serial probes succeed but something shortly after hangs |
| `UBUNTU: SAUCE: arm64: dts: qcom: sc8280xp-x13s: disable PCIe perst pull downs` | DTS | Prevents GPIO pinctrl from fighting the bootloader | **LOW** — W767 PCIe is different, but the pattern (bootloader pinctrl conflict) is relevant |

Additionally, the **Jan 2025 SRU** (`[SRU,O,v2,0/8] Lenovo X13s fails to boot kernel 6.11`) includes:
- `phy: qcom: qmp-combo: move driver data initialisation earlier`
- `serial: qcom-geni: fix polled console corruption`
- `serial: qcom-geni: disable interrupts during console writes`

The `qmp-combo: move driver data initialisation earlier` patch is especially relevant. The X13s 6.11 boot failure (LP: #2089237) manifested as a **complete boot hang** — the same symptom class as W767 iter-58/59. The fix moved `drvdata` assignment before any call that could fail/defer, preventing a NULL dereference in later cleanup paths.

### 1.4 johan_defconfig — the X13s "known working" config fragment

jhovold publishes `johan_defconfig` in his wip branches. Key entries for display initramfs support:

```
# For keyboard input and (more than 30 seconds of) display in initramfs:
i2c_hid_of i2c_qcom_geni          # keyboard
leds_qcom_lpg pwm_bl              # backlight
qrtr pmic_glink_altmode gpio_sbu_mux phy_qcom_qmp_combo
gpucc_sc8280xp dispcc_sc8280xp    # clock controllers
phy_qcom_edp panel_edp msm        # display stack
```

**Critical observation:** In `johan_defconfig`, these are **modules** (`=m`) except the core SoC glue. The comment explicitly says "more than 30 seconds of display in initramfs" — implying that without these modules in the initramfs, the display stack may not survive past early boot. This validates the W767 iter-49 finding that `DRM_MSM=m` worked.

### 1.5 What the X13s 6.8-rc1 regression teaches us

jhovold reported (`lkml, Feb 2024`) that since 6.8-rc1, the X13s eDP display **did not always show up on boot**. The root cause was the interaction between:
1. `pmic_glink` + `aux_bridge` (the DRM aux bridge that bridges AUX channel to DP/eDP)
2. Runtime PM rework in the eDP driver

The W767 DTS has a root-level `pmic-glink` node with `connector@0/1` and **no orientation-switch** (iter-33 simplification). The iter-56 boot log shows:
```
aux_bridge.aux_bridge.0: error -19: failed to acquire drm_bridge
```
This is the **same** orphan bridge symptom that X13s hit. On X13s it caused `-16` (`-EBUSY`) or NULL dereference crashes. On W767 with `DRM_MSM=y` and no actual display consumers, it may be causing the silent wedge by leaving the aux bridge in a half-initialized state that later blocks a spinlock or regulator in its teardown path.

**Abhinav Kumar's diagnosis (Qualcomm):** "I think the issue is either a combination of [pm runtime eDP] along with DRM aux bridge … OR just the latter as even that went in around the same time. That's why perhaps this issue was not seen with the chromebooks we tested on as they do not use pmic_glink (aux bridge)."

W767 **does** use pmic_glink. This is a direct match.

### 1.6 Bottom line on Ubuntu/X13s kernel

| Approach | Viability | Notes |
|----------|-----------|-------|
| Boot Ubuntu X13s `.deb` kernel + X13s DTB on W767 | ❌ Broken | Wrong board DTB |
| Boot Ubuntu X13s `Image` + W767 DTB | 🟡 Possible but wasteful | You'd still need to verify all SC8180X drivers are compiled in; Ubuntu's config may lack SC8180X-specific options |
| **Apply X13s SAUCE patches to your v7.0 tree** | ✅ Best ROI | The regulator-load and qmp-combo fixes are SoC-family-generic and likely fix the wedge |
| **Use johan_defconfig as a config reference** | ✅ Useful | Validates `DRM_MSM=m`, `CONFIG_PHY_QCOM_EDP=m`, etc. |
| **Track jhovold `wip/sc8280xp-6.16` for DRM fixes** | ✅ High value | Any eDP/DP/aux_bridge fix for SC8280XP will likely also fix SC8180X |

---

## Part 2 — Answers to iter-59 A–E

### A. Wedge-with-IRQs-off: what driver class is most likely?

**Answer: PHY regulator-load deadlock, followed by aux_bridge teardown race.**

The exact symptom (no panic, caps lock dead, 10+ min silent) requires:
1. A spinlock or raw spinlock taken with local IRQs disabled
2. A code path inside that lock that never returns

Candidates ranked by likelihood given iter-59 state (`&edp_phy` disabled, `&dispcc` enabled, DRM_MSM=y):

1. **`phy-qcom-qmp-combo` (USB-C/DP PHY)** — **#1 suspect**
   - Now that `&dispcc` is on, the combo PHY driver has resolvable clocks and probes its DP side.
   - The DP side does regulator + interconnect + clock initialization.
   - Ubuntu's SAUCE patch `phy: qcom-qmp-combo: drop regulator loads` exists because this **exact** driver was deadlocking on regulator `set_load()` during probe on X13s.
   - W767 uses the same QMP combo PHY IP (just a different SoC revision).

2. **`aux_bridge` + `pmic_glink` teardown** — **#2 suspect**
   - The `aux_bridge` driver registered at boot but failed to acquire a `drm_bridge` (expected, since `&mdss_edp` is disabled).
   - If `aux_bridge` leaves a refcount or a workqueue in a bad state, a later probe (e.g., when dwc3 or typec tries to query orientation) may take a mutex that `aux_bridge` is already holding via its failed-probe cleanup path.
   - The X13s 6.8 regression was a NULL dereference in `drm_aux_bridge_attach`; a silent spinlock variant is plausible if the cleanup path is different in v7.0.

3. **`qcom-pmic-glink` UCSI path** — **#3 suspect**
   - The W767 root `pmic-glink` node has `connector@0/1` with no `orientation-switch`. The `ucsi-qcom` driver may still probe and try to read connector state via RPMSG.
   - If the RPMSG channel to the ADSP hasn't finished bringing up `pmic_glink_altmode`, the UCSI driver can spin waiting for a response.
   - However, this would typically show in `dmesg` as deferred probe, not a hard wedge.

4. **MPSS PIL / `qcom_q6v5_mss`** — **#4 suspect**
   - MPSS firmware authentication can take the SCM (secure channel) path. If the SCM call hangs in TrustZone, the calling CPU is wedged.
   - But MPSS worked in iter-49 with the same firmware, and the hang occurs before any obvious MPSS log.

**Ruling out `phy-qcom-edp`:** You disabled it in iter-59, so it's off the table. Good move.

### B. Cheapest way to convert a silent wedge into something recoverable

**Answer: Rebuild with HARDLOCKUP_DETECTOR + BOOTPARAM_HARDLOCKUP_PANIC.**

This is the only reliable path. Here's why the other options are weaker:

- **`nosmp` / `maxcpus=1`:** SC8180X is a big.LITTLE octa-core (4+4). Many Qualcomm drivers (SMMU, interconnect, remoteproc) assume SMP and may misbehave or still deadlock on CPU 0. Also, if the deadlock is in a hardware register poll, it doesn't matter how many CPUs are online—the polling CPU is gone.
- **RCU stall NMI:** You already dropped `rcu_cpu_stall_suppress=1`. But RCU stall detection fires when a CPU hasn't context-switched for ~21s. If the wedged CPU took a raw spinlock with IRQs off, it isn't even servicing timer interrupts. RCU may not detect it as a "stall" because the scheduler clock isn't ticking. The HARDLOCKUP detector uses the perf/NMI watchdog, which fires independently of interrupts.
- **Patch the driver:** You don't yet know which driver to patch.

**Recommended kconfig changes:**
```
CONFIG_HARDLOCKUP_DETECTOR=y
CONFIG_HARDLOCKUP_DETECTOR_PERF=y
CONFIG_BOOTPARAM_HARDLOCKUP_PANIC=y
# Optionally:
CONFIG_SOFTLOCKUP_DETECTOR=y
CONFIG_BOOTPARAM_SOFTLOCKUP_PANIC=y
```

With these, a CPU spinning for >10s with IRQs off gets NMI'd by another CPU, the NMI handler detects the lockup, and `panic=10` reboots into ramoops. You then get a full stack trace in `/sys/fs/pstore/console-ramoops-0`.

### C. Are `clk_ignore_unused` + `pd_ignore_unused` masking the real symptom?

**Answer: They could be, but you cannot drop them on SC8180X.**

- **Why they exist:** The SC8180X/SC8280XP GCC has clocks that the bootloader leaves on but Linux's `clk_disable_unused()` misidentifies as "unused" because the consumer drivers haven't probed yet. If the kernel gates them off, the consumer driver later finds a dead clock and hangs. This is a documented Qualcomm bring-up quirk.
- **Could they mask a bug?** Yes. If a clock is supposed to be explicitly managed by a driver (e.g., `disp_cc_mdss_edp_aux_clk`), but `clk_ignore_unused` leaves it on from the bootloader, the driver may skip its own init sequence and later make incorrect assumptions about the hardware state.
- **Should you drop them for a test?** Only as a **diagnostic** with `initcall_debug` and a serial logger. Without them, the kernel will likely hang *earlier* (at ~2-5s instead of ~12s), but the hang location may reveal which clock/power-domain was actually needed. Since you don't have serial, this is risky—you might just get a black screen with even less info.
- **Better approach:** Keep the flags for now, but add `dyndbg="file drivers/clk/qcom/* +p; file drivers/soc/qcom/rpmhpd.c +p"` to see exactly which clocks/domains are being skipped.

### D. How to bisect which probe after 12.93s is wedging

**Answer: `initcall_blacklist=` is viable, but `deferred_probe_timeout=1` + `initcall_debug` is faster.**

#### Option 1: `initcall_blacklist` (if you can guess the initcall)

The kernel prints initcall names with `initcall_debug`. Look at the last few successful initcalls in the visible log. The next one in the same level (`device_initcall`, `late_initcall`, etc.) is likely the wedge.

To find the exact names:
```bash
# On the build host, in the kernel source:
grep -n 'device_initcall\|fs_initcall' drivers/phy/qualcomm/phy-qcom-qmp-combo.c
grep -n 'device_initcall' drivers/usb/typec/pmic-glink-altmode.c
grep -n 'device_initcall' drivers/phy/qualcomm/phy-qcom-edp.c
grep -n 'device_initcall' drivers/gpu/drm/msm/*.c
```

Then blacklist:
```
initcall_blacklist=phy_qcom_qmp_combo_init,pmic_glink_altmode_init,aux_bridge_init
```

If boot proceeds past 12.93s, one of those is the culprit.

#### Option 2: `deferred_probe_timeout=1` + `initcall_debug`

Set `deferred_probe_timeout=1` (instead of 10). This forces the deferred probe workqueue to give up after 1s and print all devices that are still deferred. If the wedge is actually a deferred-probe loop (not a hard lockup), this will surface it.

However, your symptom is a **hard** wedge (no CPU activity), not a deferred-probe flood. So Option 1 is more likely to help.

#### Option 3: Modularize everything suspicious

Instead of blacklisting initcalls, rebuild with the suspected drivers as modules:
```
CONFIG_PHY_QCOM_QMP_COMBO=m
CONFIG_PHY_QCOM_EDP=m          # already disabled in DTS, but =m for safety
CONFIG_DRM_MSM=m               # already discussed
CONFIG_TYPEC_QCOM_PMIC=m
```

If the boot reaches init with these as modules, you can `modprobe` them one by one and observe the hang in real time via the wifi logger. This is the **most diagnostic** approach.

### E. ONE cheapest next change

**Answer: `CONFIG_DRM_MSM=m` + `CONFIG_PHY_QCOM_QMP_COMBO=m` + `CONFIG_PHY_QCOM_EDP=m` (with `&edp_phy` already disabled in DTS).**

**Why:**
1. It costs **only a kconfig edit**, no kernel source patching.
2. It is the **most diagnostic** change: if boot reaches `/init`, you know the hang is in one of those two drivers.
3. You can then `modprobe phy_qcom_qmp_combo` and watch it wedge live, with the wifi logger capturing the exact line.
4. It aligns with the **known-working iter-49 state** (`DRM_MSM=m`) and with jhovold's `johan_defconfig` (QMP combo and EDPH as modules).
5. If it still wedges even as modules (i.e., during `modprobe`), you have a live shell to run `echo 1 > /sys/kernel/debug/tracing/tracing_on` or attach `kdb`.

**Second choice (if you can't rebuild kconfig right now):**
Disable `&usb_prim_qmpphy` and `&usb_sec_qmpphy` in the DTS temporarily:
```dts
&usb_prim_qmpphy { status = "disabled"; };
&usb_sec_qmpphy { status = "disabled"; };
```
This prevents the QMP combo PHY from probing at all, isolating it as a suspect. But you'll lose USB3 SuperSpeed (USB2 via HS PHY might still work). Only do this as a one-shot test.

---

## Part 3 — Concrete Action List

### Immediate (next build)
1. **Edit `w767-os/kernel/w767.config`:**
   ```diff
   -CONFIG_DRM_MSM=y
   +CONFIG_DRM_MSM=m
   -CONFIG_PHY_QCOM_QMP_COMBO=y
   +CONFIG_PHY_QCOM_QMP_COMBO=m
   -CONFIG_PHY_QCOM_EDP=y
   +CONFIG_PHY_QCOM_EDP=m
   ```
2. **Add lockup detection:**
   ```
   +CONFIG_HARDLOCKUP_DETECTOR=y
   +CONFIG_HARDLOCKUP_DETECTOR_PERF=y
   +CONFIG_BOOTPARAM_HARDLOCKUP_PANIC=y
   +CONFIG_SOFTLOCKUP_DETECTOR=y
   ```
3. **Keep `&edp_phy { status = "disabled"; }`** from iter-59.
4. **Rebuild, boot, verify `/init` is reached and wifi logger streams.**

### If boot succeeds (expected)
5. **`modprobe phy_qcom_qmp_combo`** via the running shell. Watch wifi logger.
6. If it wedges here → you have the smoking gun. Apply the Ubuntu SAUCE patch `phy: qcom-qmp-combo: drop regulator loads` (or the 6.11 SRU fix `move driver data initialisation earlier`).
7. If it doesn't wedge → `modprobe msm`. Watch for the DPU/GPU init failure. If it wedges, the issue is in DRM_MSM core with the orphan display nodes.

### If boot still wedges even with everything modularized
8. The hang is in a **built-in** driver that isn't one of the above. Rebuild with `initcall_debug` and read the visible log for the last successful `device_initcall` name. Blacklist the next one(s).
9. Or, use the ramoops + HARDLOCKUP path: after the reboot, mount pstore and read the stack trace.

### Medium-term (borrow from X13s)
10. Clone `https://github.com/jhovold/linux/tree/wip/sc8280xp-6.16` (or latest).
11. Cherry-pick or diff the following into your v7.0 tree:
    - `UBUNTU: SAUCE: phy: qcom-edp: drop regulator loads`
    - `UBUNTU: SAUCE: phy: qcom-qmp-combo: drop regulator loads`
    - `UBUNTU: SAUCE: clk: qcom: gcc-sc8280xp: don't use parking clk_ops for QUPs` (adapt for `gcc-sc8180x.c`)
    - The 6.11 SRU `phy: qcom: qmp-combo: move driver data initialisation earlier`
12. Diff `johan_defconfig` against your `w767.config` for any missing options (e.g., `CONFIG_DRM_PANEL_EDP=m`, `CONFIG_BACKLIGHT_PWM=y`, `CONFIG_LEDS_QCOM_LPG=y`).

---

## Appendix — Relevant Patch / Source URLs

| Resource | URL | What it contains |
|----------|-----|-----------------|
| jhovold/linux wip branch | `https://github.com/jhovold/linux/tree/wip/sc8280xp-6.16` | X13s-specific fixes, `johan_defconfig` |
| Ubuntu X13s PPA | `ppa:ubuntu-concept/x13s` | `linux` package with SAUCE patches |
| Juerg Haefliger PPA | `ppa:juergh/linux` | Newer kernel builds (6.16+) |
| X13s 6.11 SRU | `https://patchwork.ozlabs.org/project/ubuntu-kernel/list/?submitter=71819` | `qmp-combo: move driver data init earlier`, `serial: qcom-geni` fixes |
| X13s Sep 2024 SAUCE | `https://patchwork.ozlabs.org/project/ubuntu-kernel/cover/20240903084010.3746280-1-juerg.haefliger@canonical.com/` | `phy: drop regulator loads`, Venus, camera fixes |
| X13s 6.8-rc1 regression LKML | `https://lkml.rescloud.iu.edu/2402.1/05993.html` | jhovold's report on `aux_bridge` + eDP PM regression |
| jenneron/linux W767 WIP | `https://gitlab.com/jenneron/linux/-/commit/76f402e8` | Original community W767 DTS from 2022 (for reference comparison) |

---

*End of KIMI_1 analysis.*
