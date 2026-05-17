# iter-32: URS dwc3 pmic-glink endpoint chain + diag script

**Date:** 2026-05-17
**Status:** built, awaiting boot
**Source of fix:** mainline `arch/arm64/boot/dts/qcom/sc8180x-lenovo-flex-5g.dts` — same SoC, working URS dwc3 reference

## What iter-31 left

After iter-31's cascade fix (`HWSPINLOCK_QCOM=y`):
- SMEM/SMP2P/CDSP/ADSP all running
- iter-31 deferred list dropped from 10 → 2
- The 2 remaining: `a600000.usb` + `a800000.usb` (`dwc3: failed to initialize core`)
- No `wlan0` (ath10k probed but QMI handshake hasn't fired)

The dwc3 URS failure is independent of SMEM; it's a DT-graph issue.

## The fix — port the pmic-glink endpoint chain from Lenovo Flex 5G

Mainline `sc8180x-lenovo-flex-5g.dts` (same SoC, same dual-URS topology) shows what's needed:

```
pmic-glink/connector@0 {
    ports {
        port@0 { reg = <0>; pmic_glink_con0_hs: endpoint {
            remote-endpoint = <&usb_prim_dwc3_hs>; }; };
        port@1 { reg = <1>; pmic_glink_con0_ss: endpoint {
            remote-endpoint = <&usb_prim_qmpphy_out>; }; };
    };
};
&usb_prim_qmpphy { orientation-switch; };
&usb_prim_qmpphy_out { remote-endpoint = <&pmic_glink_con0_ss>; };
&usb_prim_dwc3_hs    { remote-endpoint = <&pmic_glink_con0_hs>; };
```

(plus symmetrical for `usb_sec`/`connector@1`.)

Our iter-31 `pmic-glink/connector@0` had only `power-role = "dual"` — **no ports, no endpoints.** Modern `dwc3-qcom` reads this graph to learn orientation and role, and won't initialize the dwc3 core until the chain is satisfied.

iter-32 ports the full chain. Skipped the SBU mux endpoints for now (only needed for DP altmode which we don't have).

### DTS changes

- `pmic-glink/connector@0`: added `ports { port@0 (HS), port@1 (SS) }` referencing `usb_prim_dwc3_hs` + `usb_prim_qmpphy_out`
- `pmic-glink/connector@1`: same for `usb_sec_*`
- `&usb_prim_qmpphy` + `&usb_sec_qmpphy`: added `orientation-switch;`
- `&usb_prim_qmpphy_out`, `&usb_sec_qmpphy_out`: added `remote-endpoint` back-pointing to the pmic-glink SS endpoint
- `&usb_prim_dwc3_hs`, `&usb_sec_dwc3_hs`: added `remote-endpoint` back-pointing to the pmic-glink HS endpoint

DTB grew by ~600 bytes — confirms the endpoint chain landed.

## /bin/diag script — one-word shell dump

Per user request to avoid typing long greps, the iter-32 initramfs ships `/bin/diag`. Type `diag` at the shell and it prints:

- summary (uname, uptime, USB, NET, remoteproc state+fw, typec, auxiliary, deferred-full)
- dmesg slices: dwc3+qmpphy+URS, ath10k+wifi+QMI+qrtr, remoteproc+q6v5+smp2p+smem+hwspinlock, pmic_glink+typec+ucsi
- regulator summary (head 30)

That gives a one-shot photographable answer to "what's broken now."

## Expected iter-32 outcomes

| What lands | Means |
|---|---|
| Both URS dwc3 init successful, /dev/sda enumerates, typec class populated | DTS chain fix was the right call. Bonus: PD/role-switch works. |
| URS dwc3 init succeeds, `/dev/sda` shows, but `wlan0` still missing | DTS was the URS fix; ath10k is a separate problem (race vs ADSP, or QMI not firing). Still huge progress. |
| URS dwc3 still fails | Endpoint chain wasn't enough — maybe missing supplies/clocks on QMP PHY, or driver needs `qcom,sc8180x-dwc3` compatible already does this. Use `diag` for the full failure trace. |

## What changed in this iter

- DTS: pmic-glink endpoint chain + orientation-switch on QMP PHYs (the fix)
- Initramfs: relabeled iter-32, added `/bin/diag` script
- Kernel: unchanged from iter-31 (still has all SMP2P/HWSPINLOCK/USB-storage flips)

## Files

- DTS: `dts/sc8180x-samsung-w767.dts` (pmic-glink + usb_prim/sec endpoint sections)
- Initramfs: `w767-os/initramfs/layout-iter32/init`, `w767-os/initramfs/layout-iter32/bin/diag`
- Image: `/tmp/w767-iter32.img` (784 MB, local only)
- Reference: `arch/arm64/boot/dts/qcom/sc8180x-lenovo-flex-5g.dts` (mainline)
