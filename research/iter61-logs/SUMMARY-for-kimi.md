# iter-61 boot result — for Kimi

> **Date:** 2026-05-18
> **Outcome:** Boot succeeds end-to-end on 6.6/pmOS kernel. Display binds (BOE 0x07e7 detected, eDP path up). ADSP+CDSP run. **MPSS stays offline → wlan0 never appears.** Single missing module identified.
>
> Full collect.sh dump under this directory: `collect/` and `initramfs-snap/`.

## What worked

| Subsystem | State |
|---|---|
| Boot through SMMU init | ✓ `arm-smmu 15000000.iommu: preserved 0 boot mappings` at 0.281s, `2ca0000.iommu` at 0.516s — no hang |
| iter-61 `/init` modprobe block | ✓ Ran cleanly, all loaded modules in `collect/modules.txt` |
| ADSP (`17300000.remoteproc`) | ✓ `state: running`, fw `qcadsp8180.mbn` |
| CDSP (`8300000.remoteproc`) | ✓ `state: running`, fw `qccdsp8180.mbn` |
| msm-drm → DPU bind | ✓ `dpu hardware revision: 0x50010001` at 2.31s |
| msm-drm → GPU (Adreno) bind | ✓ `bound 2c00000.gpu (ops a3xx_ops [msm])` at 2.31s |
| eDP PHY (`aec2a00.phy`) | ✓ probes; `vdda-pll`/`vdda-phy` dummy regs at 36mA/21mA |
| Panel detection | ✓ `panel-simple-dp-aux: Unknown panel BOE 0x07e7, using conservative timings` — this is the pmOS-wiki "Partial" status |
| DRM card | ✓ `card0-eDP-1` present in `/sys/class/drm/` |

So **the 6.8-rc1 regression diagnosis (KIMI_2) was correct.** 6.6 + display nodes enabled boots cleanly past every wedge point v7.0 hit.

## What's broken (single root cause)

**MPSS (`4080000.remoteproc`) state = offline.** Why:

`collect/var-log/rmtfs.log`:
```
failed to open /dev/qcom_rmtfs_mem1: No such file or directory
falling back to uio access
failed to open /dev/qcom_rmtfs_uio1: No such file or directory
falling back to /dev/mem access
failed to mmap: Invalid argument
```

`/dev/qcom_rmtfs_mem1` is created by the `rmtfs_mem.ko` driver
(`CONFIG_QCOM_RMTFS_MEM=m` in pmOS's config). The module IS present in our
initramfs at `/lib/modules/6.6.0/kernel/drivers/soc/qcom/rmtfs_mem.ko`, but
our iter-61 `/init` modprobe sequence didn't include it.

Cascade:
1. rmtfs_mem.ko not loaded → no `/dev/qcom_rmtfs_mem*` node
2. rmtfs daemon fails to mmap the shared region → dies immediately
3. `qcom_q6v5_pas 4080000.remoteproc: failed to initialize qmi handle` at
   1.999s (rmtfs not answering RFSA QMI calls)
4. MPSS firmware never authenticates, stays state=offline
5. WLAN protection domain inside MPSS never registers
6. `ath10k_snoc 18800000.wifi` probes but never gets its QMI client → no
   wlan0
7. `iter-54: gave up waiting for modem remoteproc after 30 tries` at 32.5s
8. `iter-54: wlan0 never appeared after 60s — log push disabled` at 65.3s

## Other observations

- `qcom_q6v5_pas 4080000.remoteproc: failed to initialize qmi handle` at
  ~2.0s is the same root cause — qcom_q6v5_pas was modprobed before rmtfs
  was up to answer QMI. But it logs "is available" at 2.21s, meaning the
  device is registered and just needs to be kicked. The kick fails because
  rmtfs is dying.
- `platform ae9a000.displayport-controller: Fixed dependency cycle(s)` at
  2.27s — non-fatal, fw_devlink quirk.
- `WARNING ... panel_edp_probe+0x508/0x560` at 2.50s — intentional WARN_ON
  in panel-edp when the panel ID isn't in `find_edp_panel`'s table. BOE
  0x07e7 is not upstreamed. Falls back to "conservative timings" which is
  why pmOS lists Screen as "Partial" not "Works".
- `gpu-vddcx` and `wifi-vdd-3.3-ch1` regulators show 0mA / 0mV in
  `initramfs-snap/regulator.txt`. Brother's iter-56 dossier (§1) named
  these as `LDO9_E (vddcx)` and `LDO16_E`. With dummy regulators substituting,
  this may still work but is something to fix in DTS later.

## Proposed iter-62

Two-line `/init` change:
```diff
 log "iter-61: modprobe SoC + remoteproc + qrtr"
-modprobe -a pdr-interface qcom-common qcom-q6v5-pas qrtr-smd 2>>/var/log/modprobe.log
+modprobe -a rmtfs_mem pdr-interface qcom-common qrtr-smd 2>>/var/log/modprobe.log
+# qcom-q6v5-pas must load AFTER rmtfs daemon is up, otherwise its QMI
+# handle registration fails. Defer to after the daemon starts.
```

Then move `modprobe qcom-q6v5-pas` to AFTER `rmtfs` / `pd-mapper` /
`tqftpserv` start (around line 65 in `/init`).

Rationale: if `qcom-q6v5-pas` is loaded first, its `failed to initialize
qmi handle` is a soft failure (it just doesn't get a QMI client), but the
remoteproc device still registers as "available" and the iter-54 init's
kick loop later succeeds. So the order MIGHT not actually matter — the
real fix is just loading `rmtfs_mem` early. Worth testing if the simpler
one-line fix works first.

## Questions for Kimi

A. Is `rmtfs_mem` the entire fix, or do we also need to reorder
   `qcom-q6v5-pas` modprobe to after rmtfs starts?
B. Is the panel-edp WARNING about BOE 0x07e7 worth fixing now (adding an
   entry to `panel-simple-edp.c`'s table for `0x07e7`) or is "conservative
   timings" good enough as a starting point?
C. The `gpu-vddcx` / `wifi-vdd-3.3-ch1` dummy regulators — could that be
   making MPSS-WLAN-service flaky even after rmtfs is up? Or is it
   functionally fine because the bootloader left rails on and
   `clk_ignore_unused pd_ignore_unused` keeps them on?
D. Any other module names from pmOS's `modules-initfs` list we should
   pre-load that we're missing? (We have `pdr-interface qcom-common
   qcom-q6v5-pas qrtr-smd phy-qcom-qmp-combo phy-qcom-edp leds-qcom-lpg
   pwm-bl panel-edp msm ath10k_snoc`.)
