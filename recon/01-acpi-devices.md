# 01 — Live ACPI Device Snapshot

**Source:** `Get-PnpDevice -PresentOnly` run on this W767 today (2026-05-17).
**Raw data:** `_raw-pnp.tsv` (~210 rows; ~85 ACPI-rooted out of 135 visible — 50 skipped due to property-extraction errors. Refresh by re-running the PowerShell snippet at the bottom of this file).

**Refreshes** `windows-extracts/pnp_all.txt` (12856 lines, May 16) and `windows-extracts/pnp_details.txt` (27291 lines, May 16). The May 16 dumps remain the deepest extraction; this file is the *current OS state diff*.

## ACPI device inventory (this boot)

For machine-readable form: `awk -F'\t' '$4 ~ /^ACPI/' recon/_raw-pnp.tsv`.

For each ACPI device, the TSV columns are:

```
Status  Class  FriendlyName  InstanceId  Service  InfPath  InfSection  HardwareIds  Parent  LocationPaths
```

So: `awk -F'\t' '$4 ~ /^ACPI\\QCOM/ {print $4 "\t" $5 "\t" $6}' recon/_raw-pnp.tsv` gives `QCOM<id>` → service + INF.

### Key cross-references (the things most likely asked)

| Symbol | Where to find it |
|---|---|
| What service binds `QCOM<id>`? | `awk -F'\t' '$4 ~ /QCOM<id>/ {print $5, $6}' _raw-pnp.tsv` |
| What chip is `oemNN.inf`? | `awk -F'\t' '$1=="oemNN.inf"' _raw-oeminfs.tsv` (see [02-pnp-drivers.md](02-pnp-drivers.md)) |
| What firmware does oemNN ship? | grep firmware section of the INF: `Select-String -Path C:\Windows\INF\oemNN.inf -Pattern '\.mbn\|\.bin'` |
| What does `\_SB.XXX` do at PEP? | `awk -F'\t' '$1 ~ /XXX/' _raw-pep-votes.tsv` (see [04-pep-vote-map.md](04-pep-vote-map.md)) |

## What changed since the May 16 dumps

Nothing structurally. Same chip set, same 309 PnP devices, same `OK` status everywhere. The May 16 dumps are still authoritative for completeness; this snapshot exists so future Qs against current OS state ("does X still appear?") have a fresh reference instead of comparing against a 4-month-old dump.

## How to refresh

```powershell
$out = "$PWD\recon\_raw-pnp.tsv"
$rows = New-Object System.Collections.ArrayList
$null = $rows.Add("Status`tClass`tFriendlyName`tInstanceId`tService`tInfPath`tInfSection`tHardwareIds`tParent`tLocationPaths")
foreach ($d in (Get-PnpDevice -PresentOnly | Sort-Object InstanceId)) {
  $get = { param($k) try { (Get-PnpDeviceProperty -InstanceId $d.InstanceId -KeyName $k -ErrorAction Stop).Data } catch { '' } }
  $svc = & $get 'DEVPKEY_Device_Service'
  $inf = & $get 'DEVPKEY_Device_DriverInfPath'
  $sec = & $get 'DEVPKEY_Device_DriverInfSection'
  $hwids = (& $get 'DEVPKEY_Device_HardwareIds') -join ';'
  $parent = & $get 'DEVPKEY_Device_Parent'
  $loc = (& $get 'DEVPKEY_Device_LocationPaths') -join ';'
  $clean = { param($s) "$s" -replace '[\r\n\t]',' ' }
  $null = $rows.Add("$($d.Status)`t$($d.Class)`t$(& $clean $d.FriendlyName)`t$($d.InstanceId)`t$svc`t$inf`t$sec`t$(& $clean $hwids)`t$parent`t$(& $clean $loc)")
}
$rows | Out-File -LiteralPath $out -Encoding utf8
```

## See also

- [02-pnp-drivers.md](02-pnp-drivers.md) — INF/CAT/provider per oem*.inf
- [03-firmware-manifest.md](03-firmware-manifest.md) — firmware blobs in DriverStore
- `docs/03-bus-and-device-map.md` — original cross-referenced inventory with DSDT line numbers
- `docs/02-samsung-platform.md` — SAM* device deep-dive
- `windows-extracts/pnp_details.txt` — May 16 deep dump (still authoritative for full property bags)
