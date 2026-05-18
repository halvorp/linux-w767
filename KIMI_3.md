# KIMI_3 — iter-61 autopsy: single missing module blocks wlan0

> **Date:** 2026-05-18
> **Source:** `research/iter61-logs/` + `SUMMARY-for-kimi.md`
> **Outcome:** Boot succeeds end-to-end on 6.6 kernel. Display + GPU + ADSP + CDSP all work. **MPSS offline → no wlan0.** Single root cause identified.

---

## 1. Empirical confirmation of the 6.6 hypothesis

| Subsystem | iter-58 (v7.0, display disabled) | iter-61 (6.6, display enabled) |
|-----------|----------------------------------|--------------------------------|
| Boot to `/init` | ❌ hard wedge at ~12.9s | ✅ success |
| SMMU probe | ❌ `2ca0000.iommu` hang (GPU island) | ✅ `preserved 0 boot mappings` clean |
| msm-drm bind | ❌ not reached | ✅ `dpu hw rev 0x50010001` at 2.31s |
| GPU (Adreno 680) | ❌ not reached | ✅ `bound 2c00000.gpu (ops a3xx_ops)` at 2.31s |
| eDP PHY | ❌ `&edp_phy` disabled as workaround | ✅ probes with dummy regulators |
| Panel | ❌ not reached | ✅ `Unknown panel BOE 0x07e7, using conservative timings` |
| DRM card | ❌ not reached | ✅ `card0-eDP-1` + `renderD128` present |
| ADSP | ✅ working | ✅ `state: running` |
| CDSP | ✅ working | ✅ `state: running` |
| MPSS | ✅ working (iter-49) | ❌ `state: offline` |
| wlan0 | ✅ working (iter-49) | ❌ missing |

**The KIMI-2 diagnosis was correct:** v7.0 has a DRM/msm eDP/DP/aux_bridge regression that causes hard hangs when the full display stack is enabled. Linux 6.6 (pmOS baseline) does not have this regression. Switching to 6.6 immediately unlocked display + GPU.

---

## 2. iter-61 root cause: missing `rmtfs_mem` kernel module

### 2.1 The failure chain (exact dmesg + log evidence)

**Step 1 — `rmtfs_mem.ko` not loaded**

`collect/modules.txt` (live module list at boot) shows 31 loaded modules. `rmtfs_mem` is **not in the list**.

**Step 2 — rmtfs daemon dies immediately**

`collect/var-log/rmtfs.log`:
```
failed to open /dev/qcom_rmtfs_mem1: No such file or directory
falling back to uio access
failed to open /dev/qcom_rmtfs_uio1: No such file or directory
falling back to /dev/mem access
failed to mmap: Invalid argument
```

**Step 3 — MPSS QMI handle fails**

`collect/dmesg.txt` line 370:
```
[    1.999726] qcom_q6v5_pas 4080000.remoteproc: failed to initialize qmi handle
```

Same line appears for CDSP and ADSP at 2.02s and 2.03s, but those recover later (see below).

**Step 4 — MPSS stays offline**

`collect/remoteproc-detail.txt`:
```
=== remoteproc0 ===
name:     4080000.remoteproc
state:    offline
firmware: qcom/samsung/w767/qcmpss8180_XEF.mbn
```

`remoteproc1` (CDSP) and `remoteproc2` (ADSP) are `state: running`. This proves the failure is **MPSS-specific**, not a generic remoteproc problem.

**Step 5 — no wlan0**

`collect/net.txt` shows only `lo`. No `wlan0`.

`ath10k_snoc` is loaded (`modules.txt` line 1) but without the MPSS-hosted WLAN QMI service, it never registers the netdev.

### 2.2 Why ADSP/CDSP survive but MPSS doesn't

ADSP and CDSP do **not** require `rmtfs` for their core operation. They may log a QMI handle failure for auxiliary services (e.g., PDR, sensor proxy), but their firmware authentication path does not go through the Remote File System Accessor (RFSA). The init script's kick loop (`echo start > /sys/class/remoteproc/remoteprocN/state`) eventually brings them up.

MPSS, however, is the **modem subsystem**. Its firmware (`qcmpss8180_XEF.mbn`) is authenticated and its file system (NV/config partitions) is accessed via QMI to `rmtfs`. When `rmtfs` is dead, MPSS gets no RFSA service → `qcom_q6v5_pas` releases the remoteproc device → state stays `offline`.

The dmesg confirms this:
```
[    1.999726] qcom_q6v5_pas 4080000.remoteproc: failed to initialize qmi handle
[    2.019993] remoteproc remoteproc0: releasing 4080000.remoteproc
[    2.212100] remoteproc remoteproc0: 4080000.remoteproc is available
```

"releasing" means the driver gave up. "is available" means the device is registered in sysfs but the driver won't touch it again until explicitly kicked. The init script's kick loop (`iter-54: starting pd-mapper, rmtfs, tqftpserv` at 2.28s) runs, but since rmtfs is already dead, the kick does nothing useful for MPSS.

---

## 3. Answers to SUMMARY questions A–D

### A. Is `rmtfs_mem` the entire fix, or do we also need to reorder `qcom-q6v5-pas`?

**`rmtfs_mem` is the entire fix.** Reordering `qcom-q6v5-pas` is not required.

Reasoning:
- The dmesg shows `qcom_q6v5_pas` fails QMI handle at 1.999s for **all three** remoteprocs (MPSS, CDSP, ADSP).
- ADSP and CDSP **still** end up `state: running` despite the early QMI failure. This means the QMI handle failure is **not fatal** for basic bring-up; the drivers either retry internally or the init script's kick loop succeeds later.
- MPSS is the only one that stays offline because MPSS **uniquely** depends on `rmtfs` for RFSA. Without rmtfs, the init script's kick loop is hitting a dead QMI service.
- Therefore, the correct fix is to ensure `rmtfs_mem.ko` is loaded **before** the rmtfs daemon starts. The order of `qcom_q6v5_pas` vs rmtfs daemon is irrelevant as long as rmtfs is alive when the kick loop runs.

### B. Is the panel-edp WARNING about BOE 0x07e7 worth fixing now?

**No. Fix it after wlan0 works.**

The WARNING at 2.50s is:
```
WARNING: panel_edp_probe: panel BOE 0x07e7 not in known table, using conservative timings
```

This is a `WARN_ON()` in `drivers/gpu/drm/panel/panel-edp.c` when the panel ID isn't in the hardcoded `find_edp_panel()` lookup table. The panel still works — it falls back to VESA DMT conservative timings. This is exactly why pmOS lists the W767 screen as "Partial" rather than "Works".

Fixing it means adding a `struct edp_panel` entry for BOE 0x07e7 to `panel-edp.c` with the correct timings (which you could extract from the Windows EDID dump if your brother gets it). But the panel is functional today. Don't get distracted by polish when the networking stack is down.

### C. Dummy regulators for `gpu-vddcx` and `wifi-vdd-3.3-ch1` — problem?

**Not a blocker for MPSS/wlan0, but should be fixed for stability.**

`gpu-vddcx` (LDO9_E) is the GPU's core voltage. With a dummy regulator, the GPU may be running at an undefined voltage. The fact that `msm` bound and `renderD128` appeared suggests the bootloader left the rail at a viable voltage. But under load (3D rendering), the GPU could glitch or hang. Fix this in the DTS by wiring `vddcx-supply = <&vreg_l9e_0p88>;` on `&gpu`.

`wifi-vdd-3.3-ch1` is a board-side rail that may not exist on W767. The `&wifi` node in your DTS has `vdd-3.3-ch1-supply` commented out with "not used?". If ath10k_snoc probes without it, it's probably fine. The real blocker is the missing MPSS QMI service, not a regulator.

**However:** `clk_ignore_unused` + `pd_ignore_unused` are keeping bootloader-configured clocks on. This is masking the fact that some regulators aren't explicitly enabled by Linux drivers. If you ever drop those cmdline flags (which you shouldn't on SC8180X), the missing regulator wiring could cause regressions.

**Verdict:** Wire `vdda-supply` and `vddcx-supply` on `&gpu` in the next DTS iteration. Don't block wlan0 bring-up on it.

### D. Any other missing modules from pmOS's initramfs list?

**Only `rmtfs_mem`.** Your current initramfs modprobe list is comprehensive.

From the SUMMARY, your current list is:
```
pdr-interface qcom-common qcom-q6v5-pas qrtr-smd
phy-qcom-qmp-combo phy-qcom-edp leds-qcom-lpg pwm-bl panel-edp msm ath10k_snoc
```

pmOS's `johan_defconfig` initramfs modules (from KIMI-1) include:
```
nvme phy_qcom_qmp_pcie pcie_qcom       # X13s-specific (NVMe root, not W767)
phy_qcom_qmp_ufs ufs_qcom              # W767 uses UFS, but these may be built-in
i2c_hid_of i2c_qcom_geni               # keyboard
leds_qcom_lpg pwm_bl                   # backlight
qrtr pmic_glink_altmode gpio_sbu_mux phy_qcom_qmp_combo gpucc_sc8280xp dispcc_sc8280xp
phy_qcom_edp panel_edp msm             # display
```

Your list already covers the W767 equivalents. The only missing one was `rmtfs_mem`.

**One addition to consider:** `qcom_q6v5_mss` is the MSS/MPSS-specific remoteproc driver. In some configurations, `qcom_q6v5_pas` is the generic PAS driver that handles ADSP/CDSP/SLPI, while `qcom_q6v5_mss` is specific to the modem. Check if your kernel config has `CONFIG_QCOM_Q6V5_MSS=y/m`. If it's a separate module, you need to load it too. In 6.6, it may be folded into `qcom_q6v5_pas`, but verify with:
```bash
find /lib/modules/6.6.0 -name '*q6v5*mss*' -o -name '*rmtfs_mem*'
```

---

## 4. Proposed iter-62 — concrete patch

### 4.1 Initramfs `/init` fix

In `w767-os/initramfs/layout-iter45/init` (or whichever layout the build script currently uses):

Find the daemon start block (~line 62) and add `modprobe rmtfs_mem` before it:

```diff
  log "iter-45: starting pd-mapper, rmtfs, tqftpserv"
+ modprobe rmtfs_mem 2>/dev/null || log "iter-62: rmtfs_mem modprobe failed"
+ sleep 1
  /usr/bin/pd-mapper          >/var/log/pd-mapper.log   2>&1 &
  /usr/bin/rmtfs              >/var/log/rmtfs.log       2>&1 &
  /usr/bin/tqftpserv          >/var/log/tqftpserv.log   2>&1 &
```

**Why `sleep 1`:** `rmtfs_mem` probe creates `/dev/qcom_rmtfs_mem1` via `devtmpfs`. The uevent may need a moment to propagate. In practice it's synchronous, but a 1s sleep is cheap insurance.

**Alternative if modprobe isn't available in the initramfs:**
Ensure `rmtfs_mem` is either:
- Built-in (`CONFIG_QCOM_RMTFS_MEM=y`) instead of modular, OR
- Added to an explicit `modprobe` call in the initramfs.

### 4.2 Kernel config fix (if you prefer built-in)

In `w767-os/kernel/w767.config`:
```diff
  CONFIG_QCOM_RMTFS_MEM=m
+ # or =y if you want the device node present before /init runs
```

Building it in (`=y`) is actually the cleaner path — the device node appears as soon as `devtmpfs` is mounted, before any daemon starts. But `=m` + explicit `modprobe` in `/init` also works.

### 4.3 Expected iter-62 outcome

| Stage | Expected result |
|-------|-----------------|
| Boot | Same as iter-61 (display up, ADSP/CDSP running) |
| rmtfs daemon | Starts successfully, opens `/dev/qcom_rmtfs_mem1` |
| pd-mapper | Registers protection domains from `.jsn` files |
| tqftpserv | Handles runtime firmware fetches |
| MPSS kick loop | Succeeds, `echo start > remoteproc0/state` triggers QMI RFSA auth |
| MPSS state | Transitions `offline → running` |
| ath10k_snoc | Gets WLAN QMI service → binds `wlan0` |
| WiFi logger | Starts streaming to 192.168.1.73:9999 |

---

## 5. Validation checklist for iter-62 boot

After boot, verify with these commands (either via the shell or via collect.sh):

```bash
# 1. rmtfs daemon is alive
pgrep -f rmtfs

# 2. rmtfs device node exists
ls -la /dev/qcom_rmtfs_mem*

# 3. MPSS is running
cat /sys/class/remoteproc/remoteproc0/state
# Expected: running

# 4. QRTR nodes show wlanfw service
cat /sys/kernel/debug/qrtr/nodes
# Expected: entries including the wlanfw service ID

# 5. wlan0 exists
ip link show wlan0

# 6. WiFi can scan
iw dev wlan0 scan | head -20
```

If any of steps 1-3 fail, the fix didn't land. If step 3 passes but 4-5 fail, the issue is QRTR/WLAN service registration (check `tqftpserv` and `pd-mapper` logs).

---

## 6. What to do if iter-62 still fails

If `rmtfs_mem` is loaded and the daemon starts, but MPSS still stays offline, the next suspects are:

1. **PD-mapper `.jsn` files missing or wrong path** — verify `/lib/firmware/qcom/samsung/w767/*.jsn` exist and `pd-mapper.log` shows them being parsed.
2. **MPSS firmware corruption** — verify SHA256 of `qcmpss8180_XEF.mbn` against the known-good copy from `firmware-stage-w767/`.
3. **RMTFS backing files missing** — the init script touches `/boot/modem_fs1`, `/boot/modem_fs2`, `/boot/modem_fsc`, `/boot/modem_fsg`. If the ESP isn't mounted at `/boot` in the initramfs, rmtfs may have no backing store. Check `rmtfs.log` for "no such file" on these.
4. **QMI service ID mismatch** — Samsung's MPSS may use a different QMI service ID for WLAN than the standard Qualcomm one. This would need `ath10k_snoc` debug (`dyndbg="file drivers/net/wireless/ath/ath10k/* +p"`) to diagnose.

---

*End of KIMI_3 analysis.*
