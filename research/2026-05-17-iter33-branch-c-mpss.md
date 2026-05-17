# iter-33: Branch C (revert URS connector graph) + MPSS firmware bake

**Date:** 2026-05-17 (late)
**Status:** built, awaiting boot
**Based on:** brother's Q6/Q7 reply at commit `035a854`

## Two fixes in one iter

### 1. Branch C — strip iter-32's URS connector graph

Brother's findings:
- **No TLMM orientation GPIOs exist on W767.** USB-C CC status routes via Samsung EmuEC OperationRegion (`\_SB.EMEC.CCST` for connector 1, `CCS2` for connector 2) → SafiDrv UCSI emulator. Linux's `pmic_glink_ucsi` can't reach this Samsung-private path.
- **Windows leaves URS dwc3 cores dormant at idle** — only powers them up when a USB-C cable is detected. Uses plain Microsoft `urssynopsys.sys` + URS class extension. No special PHY init, no GPIO dance, no PMIC LDO toggle.
- iter-32's `orientation-switch;` on QMP PHYs made them advertise a `typec_switch` provider waiting for a consumer (`pmic_glink_altmode`) that itself couldn't acquire orientation → cascade defer → `dwc3: failed to initialize core`.

So **the cure is to remove the medicine**: strip the orientation-switch declaration, strip the connector port graph, leave `dr_mode = "host"` and let dwc3 come up as plain xhci host.

DTS changes (vs iter-32):
- `&usb_prim_qmpphy` + `&usb_sec_qmpphy`: removed `orientation-switch;`
- Removed `&usb_prim_qmpphy_out`, `&usb_sec_qmpphy_out`, `&usb_prim_dwc3_hs`, `&usb_sec_dwc3_hs` overlays (no remote-endpoint).
- `pmic-glink/connector@0/@1`: removed `ports { ... }` subnodes — back to bare `usb-c-connector` with just `power-role/data-role = "dual"`.
- `pmic-glink` itself stays — `altmode`/`power-supply`/`ucsi` auxiliary devices are still useful for battery/PD-charging telemetry that the PMIC firmware handles directly.

DTB shrank from 84015 → 83412 bytes — confirms the chain is stripped.

### Trade-offs (per brother's brief)

- ❌ No DisplayPort over USB-C — needs altmode graph which we're skipping
- ❌ No Linux-side PD renegotiation — EmuEC's firmware-boot contract is whatever we get
- ⚠ ~50% of USB-C plug-ins land on the wrong SS data lane — flip the cable orientation if a device doesn't enumerate
- ✅ dwc3 cores should come up clean
- ✅ Plain xhci host bring-up; USB-C drives work

### 2. Bake MPSS firmware (`qcmpss8180_XEF.mbn`, 75 MB)

Brother's bonus WiFi recon revealed: WCN3998 WLAN on W767 is **MPSS-managed** (enumerated as a QCMS child of QCOM041E modem subsystem on Windows). ath10k_snoc needs QMI services from MPSS, but MPSS has been `offline` in every iter so far because the firmware wasn't in our initramfs (`/lib/firmware/qcom/samsung/w767/qcmpss8180_XEF.mbn` missing).

DTS already wires `&remoteproc_mpss` with the correct firmware-name. iter-33 just stages the actual blob.

Initramfs grew from 9.8 MB → 48 MB (cpio.gz). Image size unchanged at 784 MB (ESP was already provisioned generously).

## Expected iter-33 outcomes

| Per-frame display | Means |
|---|---|
| `--- remoteproc ---` shows `modem (mpss): running` (was `offline` before) | MPSS firmware load succeeded |
| Either `usb3` or both new buses appear in USB list | dwc3 URS cores came up as plain hosts |
| `/dev/sda` enumerates when boot drive plugged into either USB-C port | Branch C worked + USB-storage path live |
| `wlan0` appears in NET section | MPSS → QMI → ath10k_snoc chain wired |
| `typec class` empty | Expected — no role-switch arbitration this iter |

If even one of {URS dwc3, wlan0} lands, this is a milestone. If both land, we're at functional bring-up.

## What's NOT in this iter

- No further config changes (HWSPINLOCK_QCOM=y from iter-31 still active)
- No further firmware adds beyond MPSS (SLPI / video DSP not yet wired to anything)
- WiFi compatible left as inherited `qcom,wcn3990-wifi` from sc8180x.dtsi — if MPSS comes up and ath10k still can't bind, iter-34 might need to override to `qcom,wcn3998-wifi` per brother's note

## Files

- DTS: `dts/sc8180x-samsung-w767.dts` (iter-32 changes reverted; pmic-glink connectors back to bare; QMP PHY blocks back to vdda supplies only)
- Initramfs: `w767-os/initramfs/layout-iter33/init` + `w767-os/initramfs/layout-iter33/bin/diag`
- New firmware: `lib/firmware/qcom/samsung/w767/qcmpss8180_XEF.mbn` (75 MB)
- Image: `/tmp/w767-iter33.img` (local only, 784 MB)
- Brother's source brief: `research/2026-05-17-claude-q6-q7-urs-orientation.md` (commit `035a854`)
