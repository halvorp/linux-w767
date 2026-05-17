# iter-28: dwc3-qcom-legacy probe instrumentation

**For:** all instances + future-us
**Triggered by:** iter-26 (gpio + IRQ trim) and iter-27 (compatible override) both showed zero observable change. usb_mp never reaches the point of registering with the interconnect. Brother's QcXhciFilter reverse (commit fba1a4f) eliminated the Windows-side "secret sauce" hypotheses; the failure is Linux-side, in early stages of `dwc3_qcom_probe`.

**Status:** building.

## What's in iter-28

| Component | Source | State |
|---|---|---|
| **Kernel** | Built from Fedora 7.0.0-62.fc45 `.config` (extracted from kernel-devel RPM, 8500+ flags) with `USB_DWC3=y`, `USB_DWC3_QCOM=y`, `PHY_QCOM_QMP_USB=y`, `PHY_QCOM_USB_SNPS_FEMTO_V2=y`, `USB_XHCI_PLATFORM=y` flipped from `=m` to `=y`. Necessary because Fedora ships dwc3 as modules, and our minimal initramfs has no module loader. | iter-28 specific build |
| **dwc3-qcom-legacy.c** | Patched with `pr_emerg` at every step + every error return in `dwc3_qcom_probe`. Diff at `kernel-patches/iter28-diag/0001-...patch` | iter-28 specific |
| **DTB** | iter-27 base (no DT changes) | unchanged from iter-27 |
| **Initramfs** | iter-24 refresh loop (continuous on-screen dmesg) | unchanged |
| **Cmdline** | `console=tty0 loglevel=8 consoleblank=0 nomodeset rdinit=/init ... earlycon=efifb keep_bootcon ...` (iter-24 cmdline, unchanged) | unchanged |

## What the screen will show

The /init refresh loop filters dmesg for keywords including `dwc3` and `usb`. Every dwc3-qcom-legacy probe attempt now prints lines like:

```
[ 5.123] DWC3-W767: ===== probe BEGIN for a8f8800.usb =====
[ 5.124] DWC3-W767: a8f8800.usb STEP1 reset_control_array_get_optional...
[ 5.124] DWC3-W767: a8f8800.usb STEP1 OK
[ 5.125] DWC3-W767: a8f8800.usb STEP2 reset_assert...
[ 5.125] DWC3-W767: a8f8800.usb STEP2 OK
[ 5.130] DWC3-W767: a8f8800.usb STEP3 reset_deassert...
[ 5.130] DWC3-W767: a8f8800.usb STEP3 OK
[ 5.131] DWC3-W767: a8f8800.usb STEP4 clk_init (count=8)...
[ 5.140] DWC3-W767: a8f8800.usb STEP4 OK (got 8 clocks)
[ 5.141] DWC3-W767: a8f8800.usb STEP5 ioremap qscratch...
[ 5.141] DWC3-W767: a8f8800.usb STEP5 OK
[ 5.142] DWC3-W767: a8f8800.usb STEP6 setup_irq...
[ 5.150] DWC3-W767: a8f8800.usb STEP6 OK (num_ports=2)
[ 5.151] DWC3-W767: a8f8800.usb STEP7 of_register_core...
[ 5.155] DWC3-W767: a8f8800.usb STEP7 OK
[ 5.156] DWC3-W767: a8f8800.usb STEP8 interconnect_init...
[ 5.157] DWC3-W767: a8f8800.usb STEP8 OK
[ 5.160] DWC3-W767: ===== probe SUCCESS for a8f8800.usb =====

[ 8.456] DWC3-W767: ===== probe BEGIN for a4f8800.usb =====
[ 8.457] DWC3-W767: a4f8800.usb STEP1 reset_control_array_get_optional...
[ 8.457] DWC3-W767: a4f8800.usb STEP1 OK
[ 8.458] DWC3-W767: a4f8800.usb STEP2 reset_assert...
[ 8.458] DWC3-W767: a4f8800.usb STEP2 OK
[ 8.470] DWC3-W767: a4f8800.usb STEP3 reset_deassert...
[ 8.470] DWC3-W767: a4f8800.usb STEP3 OK
[ 8.480] DWC3-W767: a4f8800.usb STEP4 clk_init (count=6)...
[ 8.490] DWC3-W767: a4f8800.usb STEP4 FAIL: -517
              ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^
              this kind of line is what we need on the photo
```

A `STEPN FAIL: -<errno>` line tells us exactly which step blocks `usb_mp`. Common errno values:
- `-517` (EPROBE_DEFER) — supplier not ready yet, will retry
- `-2` (ENOENT) — DT property missing
- `-12` (ENOMEM) — alloc failure
- `-22` (EINVAL) — bad argument (often a missing required field)
- `-110` (ETIMEDOUT) — hardware stuck
- `-19` (ENODEV) — device not present

Also: if `usb_mp` probe never gets called at all, **no `DWC3-W767: ===== probe BEGIN for a4f8800.usb =====` line will appear**. That'd mean the driver didn't even bind — possibly because of_match failed even via the `qcom,dwc3` fallback, or the device wasn't registered at all (DT misparse, status=disabled inheritance, etc.).

## Significance of the Fedora-`=m` finding

When inspecting Fedora's `.config` for iter-28 prep, discovered Fedora ships `USB_DWC3=m` and `USB_DWC3_QCOM=m`. Implication for the iter-25 boot result we previously analyzed:

> iter-25 dmesg showed `sync_state pending due to a8f8800.usb` and we interpreted that as "usb_sec probed but stuck in sync_state wait." **That interpretation was wrong.**

What `sync_state pending` actually means: the supplier (interconnect/clock) is ready to call its `sync_state` callback but is waiting for the consumer to be in a state where the sync is safe. The consumer device being **registered** (DT node parsed → platform_device created) is enough to trigger the wait. **No driver bind required.**

So in iter-25, with Fedora's `dwc3-qcom` as a module that never loaded, NONE of the USB controllers actually had a driver bound. The sync_state messages were just registered platform_devices waiting for drivers that never came. usb_sec didn't "partially probe" — neither it nor usb_mp probed at all.

iter-28 fixes this by flipping the dwc3 stack to `=y` so the drivers are present at boot and probe runs.

## Possible iter-28 outcomes

| Photo shows | Interpretation | iter-29 fix path |
|---|---|---|
| All three usb_*.usb reach SUCCESS | The whole approach (Fedora config + dwc3 built-in) just works. Internal keyboard + touchpad should enumerate. | Land the build config as the canonical Linux build; done. |
| usb_prim + usb_sec SUCCESS, usb_mp FAIL at STEP N | usb_mp's specific failure mode is exposed. We know which step + what errno. | DT property fix, missing CONFIG, or driver quirk depending on which step. |
| All three FAIL identically at some step | A general SC8180X bring-up gap we haven't hit before. | Investigate at that specific step. |
| usb_mp never shows "probe BEGIN" | Driver didn't bind. DT match issue. | Check compatible/OF match path. |
| Nothing changes (no DWC3-W767 lines at all) | Something is preventing dwc3-qcom-legacy from even loading. Or screen refresh too slow. | Look at our config; maybe USB_DWC3_QCOM didn't actually get =y. |

## Files

- Patch: `kernel-patches/iter28-diag/0001-dwc3-qcom-legacy-pr_emerg-probe-instrumentation.patch`
- Boot photo will land at `research/photos/2026-05-17-iter28-*` once captured
