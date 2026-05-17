# iter-30: SMP2P + URS unblock

**Date:** 2026-05-17
**Status:** built, awaiting boot
**Based on:** brother's Q1 (`research/2026-05-17-claude-q1-usb-port-map.md`) + Q5 (`research/2026-05-17-claude-q5-wifi-ldo10.md`)
**Correction from user:** W767 has ONE USB-C on each side (left + right), not both on the LEFT as brother's PLD reading suggested. ACPI `PLD_GroupPosition` doesn't match chassis-geometry in this case.

## What iter-30 changes

### Config (saved at `w767-os/kernel/iter30-fedora-hybrid.config`)

Two clusters of `=m` → `=y` flips:

**Cluster A — the SMP2P chain (the iter-29 remoteproc-defer fix):**
- `QCOM_SMP2P` — provides `/smp2p-*/slave-kernel` supplier nodes that ADSP/CDSP/MPSS wait on. THE root cause.
- `QCOM_APCS_IPC` — mailbox controller smp2p uses (`mboxes = <&apss_shared N>`)
- `QCOM_IPCC` — newer mailbox (some peripherals reference it)
- `QCOM_PD_MAPPER` — Protection Domain mapper (ath10k_snoc QMI lookup uses this)
- `QCOM_PDR_HELPERS` — Protection Domain Restart helpers
- `QCOM_SOCINFO` — exports SoC info to sysfs (cosmetic, cheap)

**Cluster B — URS / Type-C plumbing (potential fix for "third dwc3 doesn't bring up root hub"):**
- `TYPEC`, `TYPEC_UCSI` — base USB-C / UCSI infrastructure
- `QCOM_PMIC_GLINK` — Qualcomm PMIC GLINK driver (selects PDR_HELPERS, drives the role-switch chain over GLINK to ADSP)
- `UCSI_PMIC_GLINK` — UCSI implementation that talks via PMIC GLINK
- `DRM_AUX_HPD_BRIDGE` — needed by PMIC_GLINK select

Brother's theory: our DTS forces `dr_mode = "host"` on both URS dwc3 controllers but never wires `usb-role-switch` / `connector` endpoints. Modern `dwc3-qcom` may silently defer waiting for type-C plumbing that doesn't exist as a driver. With `PMIC_GLINK=y` + the existing `pmic-glink { connector@0 { compatible = "usb-c-connector"; ... }; connector@1 { ... }; }` in our DTS (lines 121-128), the role-switch arbitrator now actually has a driver — the URS controllers should be able to settle into a stable "host-mode" state and bring up root hubs.

### DTS — unchanged

After auditing per brother's Q5 brief:
- `vdd-0.8-cx-mx-supply = <&vreg_l1e_0p75>` was already wired correctly (line 1257). `vreg_l1e_0p75` is exactly the LDO1_E at 0.752 V that Windows votes per DSDT. No change.
- `vdd-3.3-ch1-supply` stays commented out — brother confirmed Windows never touches LDO10_C; the dummy regulator is correct.
- `&usb_prim_dwc3` / `&usb_sec_dwc3` keep `dr_mode = "host"` — no role-switch endpoint chain added this iter. Wait to see if `PMIC_GLINK=y` alone unblocks them.

### Initramfs (layout-iter30/init)

Updated the post-30-frame shell-drop banner to reflect brother's findings:
- W767 has one USB-C per side
- Keyboard MCU is on usb_mp (internal)
- iter-29 brought up only one URS controller
- Hint: try the OTHER side USB-C if /dev/sda missing

Per-frame display unchanged from iter-29.1 (still dumps q6v5/remoteproc/ath10k greps).

## Expected outcomes

| What lands | Means |
|---|---|
| All three `remoteproc*` show `running`, `wlan0` appears, `/dev/sda` shows on first port try | Full success — SMP2P fix + PMIC_GLINK fix both worked. |
| ADSP/CDSP/MPSS all `running` but no `wlan0` | SMP2P fix worked; ath10k has a deeper issue (firmware mismatch, board-data issue, additional regulator). Inspect dmesg `ath10k_snoc` lines. |
| One remoteproc `running`, others still deferred | SMP2P loaded partially. Inspect which slave-kernel still pending. |
| All three still deferred | SMP2P didn't take. Verify `dmesg | grep smp2p`. Likely another hidden dep. |
| /dev/sda missing on both ports | URS controller didn't come up despite PMIC_GLINK. Will need to add connector endpoint chain in iter-31. |

## Files

- Config: `w767-os/kernel/iter30-fedora-hybrid.config`
- Initramfs: `w767-os/initramfs/layout-iter30/init`
- Brother's source briefs: `research/2026-05-17-claude-q1-usb-port-map.md`, `research/2026-05-17-claude-q5-wifi-ldo10.md`
- Image: `/tmp/w767-iter30.img` (local only, 784 MB)
