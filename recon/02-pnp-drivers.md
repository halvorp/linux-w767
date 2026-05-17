# 02 — PnP Drivers (Installed `oem*.inf` Index)

**Source:** `Get-ChildItem $env:windir\INF\oem*.inf` + header parse + `Get-WindowsDriver -Online`.
**Raw data:** `_raw-oeminfs.tsv` (130 rows) and `_raw-firmware.tsv` (130 rows).

130 OEM drivers installed. Each entry below is the chip (or platform feature) it drives, the catalog file, the provider, and the version.

## Grouping by provider

| Provider | Count | What they do |
|---|---|---|
| Qualcomm Technologies, Inc. / Qualcomm Incorporated | ~70 | SoC subsystem drivers — SPMI PMICs, qcADC, qcrmnet, qcsubsys, q6v5 PIL loaders, qcwlan, qcbluetooth, camera ISP, audio, GPIO, sensors-bridge, fastrpc, GLINK, etc. |
| Samsung Electronics Co., Ltd. | ~12 | EmuEC, ModemCtrl, GalaxyBookDriver, KbdHelper, AUDD, panel driver, Wi-Fi SAR, etc. |
| Cirrus Logic | 2 | CS35L41 amp loader + control panel |
| Dolby | ~5 | Dax3 Atmos audio post-processing |
| Egis Technology | 1 | Fingerprint sensor (no Linux equivalent) |
| ASIX, HHD Software, Microsoft (Hello, etc.) | misc | Non-platform — USB Ethernet, USB protocol-analyzer hooks, Hello biometrics |

For the raw mapping: `awk -F'\t' '{print $1 "\t" $4 "\t" $5}' _raw-oeminfs.tsv | sort -k2`.

## Critical Samsung INFs (most relevant for Linux work)

| INF | Catalog | Chip / feature | Linux equivalent |
|---|---|---|---|
| `oem9.inf` | `emuec.cat` | **EmuEC.sys** — Samsung embedded-controller emulator. Translates ACPI EC ops → I²C/SPI/GPIO writes against 8 PMIC/PD/charger chips. See `docs/02-samsung-platform.md`. | None (write thin shim) |
| `oem10.inf` | `GalaxyBookDriver_Space.cat` | SAMM0610 platform glue — manages Samsung OEM hotkeys/LEDs | None |
| `oem15.inf` | `kbdhelper.cat` | Upper filter on the SPACE keyboard composite (OSD only — does not affect keys) | None (usbhid handles keys) |
| `oem16.inf` | `mcfg_subsys_ext8180.cat` | Samsung modem cellular config (CC) tables | Bundled in MPSS firmware blob already |
| `oem17.inf` | `modemctrl.cat` | ModemCtrl (SAM0602) | None |
| `oem19.inf` | (not enumerated in head) | SAM0101 PanelDriver (eDP BOE07E7) | `panel-edp` covers it |
| `oem36.inf` | (audio) | AUDD service (Aqstic + CS35L41) | `snd-soc-wcd9340` + `cirrus,cs35l41` |
| `oem134.inf` | (modem) | qcsubsys → Snapdragon X24 LTE Modem | `qcom_q6v5_mss` (PIL loader) |
| `oem144.inf` | `qcwlan8180.cat` | qcwlan → QCMS WiFi child | `ath10k_snoc` (`qcom,wcn3998-wifi`) |
| `oem146.inf` | (qc) | QcXhciFilter on QCOM04A6 | Skip — it's pure WMI/ETW (already reversed) |
| `oem152.inf` | (sam) | SAM0606 SecPmic3p (PMIC overlay) | Standard SPMI PMIC bindings cover it |
| `oem158.inf` | (sam) | UcmEm (SAM0605) | Skip — Linux UCSI native |
| `oem160.inf` | (sam) | VHIDEvent (SAMM0901 SVBI) — system-keys via ACPI Notify | New small platform driver (~250 LoC, see research/2026-05-17-claude-keyboard-protocol.md) |

## Critical Qualcomm INFs (subsystem mapping)

| INF | Chip / feature | Linux compatible |
|---|---|---|
| `oem30.inf` | qcADC (Hexagon ADC under QCOM0411/0412) | Bundled into ADSP PIL |
| `oem102.inf` | qcpmicapps8180 — Apps-side PMIC management | `spmi-pmic-arb` covers it |
| `oem103.inf` | qcpmicglink8180 — GLINK to RPMH | `qcom_pmic_glink` |
| `oem104.inf` | qcpmicgpio8180 — PMIC GPIOs | `pinctrl-spmi-mpp` / `pinctrl-spmi-gpio` |
| `oem116.inf` | qcSensors8180 — sensor bridge | Sensors via SLPI (`qcom_q6v5_pas`) |
| `oem119.inf` | qcslimbus8180 — SLIMbus for audio codec | `slim-qcom-ngd-ctrl` |
| `oem139.inf` | qcuart (under QCOM0418) | `qcom-geni-serial` |

## How to refresh

```powershell
$out = "$PWD\recon\_raw-oeminfs.tsv"
$rows = @("InfName`tCatalog`tClass`tProvider`tDriverVer")
Get-ChildItem "$env:windir\INF\oem*.inf" | ForEach-Object {
  $c = Get-Content $_.FullName -Encoding UTF8
  $cat = ($c | Select-String -Pattern '^\s*CatalogFile\s*=\s*(.+?)\s*$').Matches[0].Groups[1].Value
  $cls = ($c | Select-String -Pattern '^\s*Class\s*=\s*(.+?)\s*$').Matches[0].Groups[1].Value
  $prv = ($c | Select-String -Pattern '^\s*Provider\s*=\s*(.+?)\s*$').Matches[0].Groups[1].Value
  $ver = ($c | Select-String -Pattern '^\s*DriverVer\s*=\s*(.+?)\s*$').Matches[0].Groups[1].Value
  $rows += "$($_.Name)`t$cat`t$cls`t$prv`t$ver"
}
$rows | Out-File -LiteralPath $out -Encoding utf8
```
