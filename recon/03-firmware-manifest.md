# 03 — Firmware Manifest

**Source:** recursive scan of `C:\Windows\System32\DriverStore\FileRepository\` for `*.mbn`, `*.bN*`, `*.bin`.
**Raw data:** `_raw-fw-blobs.tsv` (~65 blobs).

## Top firmware payloads

| Blob | Owner | Function | mainline Linux path |
|---|---|---|---|
| `qcadsp8180.mbn` | qcsubsys (modem subsystem service loads it via PIL) | ADSP (Hexagon DSP1) image | `/lib/firmware/qcom/samsung/w767/qcadsp8180.mbn` |
| `qccdsp8180.mbn` | qcsubsys | CDSP (Hexagon DSP2) image | `/lib/firmware/qcom/samsung/w767/qccdsp8180.mbn` |
| `qcmpss8180_XEF.mbn` | qcsubsys | MPSS (modem subsystem) image — Samsung-tuned (`_XEF` suffix) | `/lib/firmware/qcom/samsung/w767/qcmpss8180_XEF.mbn` |
| `qcslpi8180.mbn` (if present) | qcsubsys | SLPI (sensor low-power image) | `/lib/firmware/qcom/samsung/w767/qcslpi8180.mbn` |
| `WLANMDSP.MBN` | qcwlan (oem144.inf) | WCN3998 WLAN firmware — loaded into MPSS shared memory | `/lib/firmware/qcom/samsung/w767/wlanmdsp.mbn` |
| `bdwlanu.b5f`, `bdwlanu.b58` | qcwlan | WiFi board-data files for specific board ID (5f, 58 = board variant codes) | `/lib/firmware/qcom/samsung/w767/bdwlan.bin` (or matching variant) |
| `bdwlan.b71`, `.b46`, `.b47`, `.b48`, `.b58`, `.b5f` etc. | qcwlan | WiFi board-data overlays per regulatory/channel-plan | All copied to `/lib/firmware/qcom/samsung/w767/` |
| `q6_fw.mbn`, `q6_fw_subsys.mbn` (if present) | q6v5 family | Generic Hexagon Q6 PIL loader image | bundled in qcadsp8180 |
| `qcdxhdcp.mbn`, `qcdxhdcptz.mbn` | display | HDCP image for DisplayPort altmode | not needed for boot |
| Camera tuning `com.qti.tuned.*.bin` + `com.qti.tuned.partron_hi1a1.bin` | qcam | Camera sensor tuning | not needed for boot |
| `cs35l41.bin` (if exists) | csaudio | CS35L41 amp tuning blob (Cirrus DSP firmware) | `/lib/firmware/cirrus/cs35l41-dsp1-spk-prot.wmfw` (mainline naming differs) |

## Where each blob ends up on Linux

Mainline Linux looks for SC8180X firmware under `/lib/firmware/qcom/sc8180x/<board>/`. The pmaports W767 port uses `/lib/firmware/qcom/samsung/w767/`. Iter-31+ confirms this path works.

Reference: `docs/00-hardware-combined.md` headline correction #5 ("Firmware layout").

## Caveats

- Some firmware files have NO direct mainline analog (e.g., Samsung-tuned MPSS variant `qcmpss8180_XEF.mbn` vs mainline-expected `qcmpss8180_oeman.mbn`). Symlink or rename if mainline driver checks the exact path.
- Board-data WLAN files (`bdwlan.bXX`) are board-variant-keyed. `b5f` is most commonly the right one for W767 (per pmaports `firmware-samsung-w767`); the others are leftover from a multi-board INF.
- Some `.bin` blobs are config/calibration NOT firmware (e.g., camera tuning is a config-blob, not Hexagon code). Don't blindly copy everything — see pmaports `firmware-samsung-w767` APKBUILD for the curated list of what to ship.

## How to refresh

```powershell
$out = "$PWD\recon\_raw-fw-blobs.tsv"
$rows = @("Path`tSizeBytes`tLastWriteTime")
Get-ChildItem -Path "$env:windir\System32\DriverStore\FileRepository\" -Recurse `
  -Include *.mbn,*.bin,*.b00,*.b01,*.b46,*.b47,*.b48,*.b58,*.b5f,*.b71 -ErrorAction SilentlyContinue |
  ForEach-Object {
    $rel = $_.FullName -replace [regex]::Escape("$env:windir\System32\DriverStore\FileRepository\"), ""
    $rows += "$rel`t$($_.Length)`t$($_.LastWriteTime.ToString('yyyy-MM-dd HH:mm:ss'))"
  }
$rows | Out-File -LiteralPath $out -Encoding utf8
```
