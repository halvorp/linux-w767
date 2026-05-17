# iter-29: first WiFi attempt + USB-storage so we can finally get dmesg back

**Date:** 2026-05-17
**Status:** built, awaiting boot

## What iter-28 left us with

Internal keyboard works (commit `27ba44f`). Boot photo was the only
data because the iter-24 `/init`'s ESP write-back loop failed silently:
`USB_STORAGE=m` and `VFAT_FS=m` in the Fedora hybrid config, and our
minimal initramfs has no module loader, so no `/dev/sda1` and no mount.

## What iter-29 changes

### Config (saved at `w767-os/kernel/iter29-fedora-hybrid.config`)

Flipped from `=m` to `=y`:

| Subsystem | Configs |
|---|---|
| WiFi base | `RFKILL`, `CFG80211`, `MAC80211` |
| ath10k    | `ATH10K`, `ATH10K_SNOC`, `ATH10K_CE` |
| QMI/MHI   | `QRTR`, `QRTR_SMD`, `MHI_BUS` |
| Remoteproc| `QCOM_Q6V5_PAS`, `QCOM_Q6V5_COMMON`, `QCOM_RPROC_COMMON`, `QCOM_PIL_INFO`, `QCOM_SYSMON`, `QCOM_AOSS_QMP`, `QCOM_WCNSS_PIL`, `QCOM_WCNSS_CTRL` |
| SMEM/RPMSG| `QCOM_SMEM`, `RPMSG`, `RPMSG_CHAR`, `RPMSG_NS`, `RPMSG_QCOM_GLINK`, `RPMSG_QCOM_GLINK_SMEM`, `RPMSG_QCOM_SMD` |
| USB-storage| `USB_STORAGE`, `USB_UAS` |
| FAT       | `FAT_FS`, `VFAT_FS`, `NLS_CODEPAGE_437`, `NLS_ISO8859_1`, `NLS_ASCII` |

Discovered dep chains along the way:
- `CFG80211=y` needs `RFKILL=y` first (RFKILL was `=m`)
- `QCOM_Q6V5_PAS=y` needs `QCOM_AOSS_QMP=y` first (tristate dep at same level)
- `ATH10K_SNOC=y` needs the whole wireless stack already `=y`

The pattern: `scripts/config --enable` then `make olddefconfig`, then check
what got demoted back to `=m` and flip its blocker. Two rounds usually.

### Initramfs (staged at `w767-os/initramfs/layout-iter29/`, 19 MB)

Includes `/lib/firmware/`:
- `qcom/samsung/w767/qcadsp8180.mbn` (W767 ADSP firmware, 11 MB)
- `qcom/samsung/w767/qccdsp8180.mbn` (W767 CDSP firmware, 3 MB)
- `ath10k/WCN3990/hw1.0/wlanmdsp.mbn` (WiFi modem stub, 4 MB)
- `qca/crbtfw01.tlv`, `qca/crnv01.bin` (BT, ~100 KB total)

### `/init` changes

- Mounts debugfs (we want `devices_deferred` and `regulator_summary`)
- Refresh loop displays `--- NET ---` and `--- remoteproc ---` sections
- Dumps `state` + `firmware` for each remoteproc on screen
- dmesg filter widened: adds `ath10k|wifi|wlan|wcn|qcom_q6v5|qrtr|mhi|remoteproc|regulator`
- After 30 frames (~90s), `exec /bin/sh` — interactive shell on tty0 since
  the keyboard works as of iter-28. User can finally type `dmesg`,
  `cat /sys/class/remoteproc/*/state`, etc.
- ESP write-back loop unchanged, but USB_STORAGE=y means it should now
  actually succeed and land `iter29-snap-tryN/` directories on the drive.

## Expected boot outcomes

| What lands | Interpretation |
|---|---|
| `remoteproc0..N` shows `running`, ath10k_snoc binds, `wlan0` appears | Full success. Configure with `iw dev wlan0 scan`, etc. |
| ADSP `running` but no `wlan0` | Probe race with QMI services — try `initcall_debug` cmdline next iter, or add explicit `qcom,smd-edge` wiring. |
| ADSP shows `offline` / `error` | Firmware loading failure. Check dmesg for `qcom_q6v5_pas: error -X loading qcadsp8180.mbn`. Check pstore-saved dmesg from prior boot too. |
| Nothing related shows | Either config didn't take (verify Image is iter-29 not iter-28) or DT path broken. |
| Kernel panics | Probably regulator/clock missing for WCN3990. Boot photo gives the BUG line. |

## Files

- Config: `w767-os/kernel/iter29-fedora-hybrid.config`
- Initramfs layout: `w767-os/initramfs/layout-iter29/init`
- Rootless image builder: `w767-os/scripts/build-usb-image-rootless.sh`
- Image: `/tmp/w767-iter29.img` (local only, 784 MB)
