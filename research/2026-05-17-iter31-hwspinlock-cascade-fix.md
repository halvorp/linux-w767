# iter-31: HWSPINLOCK_QCOM=y — single-flip cascade fix

**Date:** 2026-05-17
**Status:** built, awaiting boot

## What iter30b's photo revealed (deferred-list contents)

```
a600000.usb            dwc3: failed to initialize core
a800000.usb            dwc3: failed to initialize core
4080000.remoteproc     platform: wait for supplier /smp2p-mpss/slave-kernel
8300000.remoteproc     platform: wait for supplier /smp2p-cdsp/slave-kernel
17300000.remoteproc    platform: wait for supplier /smp2p-lpass/slave-kernel
86000000.smem          qcom-smem: failed to retrieve hwlock      <-- root cause
smp2p-cdsp             qcom_smp2p: unable to allocate local smp2p item
smp2p-lpass            qcom_smp2p: unable to allocate local smp2p item
smp2p-mpss             qcom_smp2p: unable to allocate local smp2p item
smp2p-slpi             qcom_smp2p: unable to allocate local smp2p item
```

The whole cascade has a single root: `qcom-smem: failed to retrieve hwlock`.

Reason: `CONFIG_HWSPINLOCK_QCOM=m`. **Fourth iteration of the same trap** (after `USB_DWC3=m`, `USB_STORAGE=m`, `QCOM_SMP2P=m`). Initramfs has no module loader → driver doesn't load → SMEM driver can't get its TCSR mutex hwspinlock → everything downstream defers.

## What iter-31 changes

Single critical flip + two cheap helpers:

```
HWSPINLOCK_QCOM=y     # THE root-cause fix
RESET_QCOM_PDC=y      # PDC reset controller (cheap to add while we're here)
RPMSG_QCOM_GLINK_RPM=y  # RPM glink channel (older-SoC fallback path; cheap)
```

DTS unchanged. Firmware unchanged. Initramfs unchanged from iter30b apart from version-string sed.

## Expected cascade

| Currently broken | After HWSPINLOCK_QCOM=y |
|---|---|
| `86000000.smem: failed to retrieve hwlock` | SMEM init OK, partition table loaded |
| `smp2p-{cdsp,lpass,mpss,slpi}: unable to allocate local smp2p item` | All four slave-kernel suppliers register |
| `4080000/8300000/17300000.remoteproc` deferred | All three rprocs probe, qcom_q6v5_pas loads firmware |
| `ath10k_snoc 18800000.wifi` blocked on QMI | ATH10K reaches ADSP's QMI services → `wlan0` |
| `a600000.usb` + `a800000.usb` dwc3 "failed to initialize core" | rpmh power-controller fully working → QMP USB PHY powers up → dwc3 cores init → URS controllers bring up root hubs → `/dev/sda` enumerates on either USB-C port |

The dwc3-URS recovery is speculative but plausible: rpmh shares power-vote state via SMEM, so a broken SMEM hobbles every power-domain consumer including QMP USB PHY.

## What stays the same as iter30b

- Kernel = iter-30 + only HWSPINLOCK_QCOM/RESET_QCOM_PDC/RPMSG_QCOM_GLINK_RPM flipped
- DTS = unchanged
- Initramfs = same diagnostic display (smem/smp2p/typec/aux/deferred sections), just relabeled iter-31

## If iter-31 cascades fully

We'll likely have a daily-driver-capable bring-up: keyboard + wifi + USB-storage all working. Next iters become feature-add work (display via drm/msm, audio via CS35L41, touchpad, suspend) rather than bring-up debug.

## If iter-31 fixes SMEM/remoteprocs but NOT dwc3-URS

The dwc3 "failed to initialize core" is a separate hypothesis. Plan B:
- Look at `dmesg | grep -iE 'qmp|dwc3|usb_prim|usb_sec'` for the specific init failure mode.
- Possibly need to enable a USB PHY power-domain link in DTS, or re-check QMP PHY init order.
- Brother could check Windows-side QMP USB PHY power-up sequence.

## Files

- Config: `w767-os/kernel/iter31-fedora-hybrid.config`
- Initramfs: `w767-os/initramfs/layout-iter31/init`
- Image: `/tmp/w767-iter31.img` (local only, 784 MB)
- Root-cause photo: previous iter30b screen capture (deferred-list section)
