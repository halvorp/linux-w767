# Walk DSDT line by line, build a flat table of (Device, Component, FState, Rail, Mode, uV, uA, Param4, Param5).
# FState convention in PEP0 ACPI: 0 = working (D0), 1 = off (D3), 2/3 = low-power intermediates.

$dsdt = "C:\Users\peter\Downloads\linux-w767-work\acpi\dsdt.dsl"
$outTsv = "C:\Users\peter\Downloads\linux-w767-work\recon\_raw-pep-votes.tsv"

$lines = Get-Content -LiteralPath $dsdt -Encoding ASCII
$N = $lines.Length

$device = ""
$component = ""
$fstate = ""

$rows = New-Object System.Collections.ArrayList
$null = $rows.Add("Device`tComponent`tFState`tRail`tMode`tMicrovolt`tMicroamp`tParam4`tParam5`tLine")

for ($i = 0; $i -lt $N; $i++) {
    $L = $lines[$i]

    if ($L -match '^\s*"DEVICE",\s*$') {
        # Next non-blank line is the device path string, possibly preceded by a numeric (e.g., DEVICE, 0x02, path).
        for ($j = $i + 1; $j -lt $N -and $j -lt $i + 4; $j++) {
            if ($lines[$j] -match '^\s*"(\\\\_SB[^"]+)"') {
                $device = $Matches[1] -replace '\\\\', '\'
                $component = ""
                $fstate = ""
                break
            }
        }
        continue
    }

    if ($L -match '^\s*"COMPONENT",\s*$') {
        if ($lines[$i + 1] -match '^\s*(0x[0-9A-Fa-f]+|\d+),?\s*$') {
            $component = $Matches[1]
            $fstate = ""
        }
        continue
    }

    if ($L -match '^\s*"FSTATE",\s*$') {
        if ($lines[$i + 1] -match '^\s*(0x[0-9A-Fa-f]+|\d+),?\s*$') {
            $fstate = $Matches[1]
        }
        continue
    }

    if ($L -match '"PMICVREGVOTE"') {
        # The next non-comment line is "Package (0x06)" and then 6 fields, one per line.
        $rail = ""; $mode = ""; $uv = ""; $ua = ""; $p4 = ""; $p5 = ""
        $found = 0
        for ($j = $i + 1; $j -lt $N -and $j -lt $i + 25; $j++) {
            $K = $lines[$j].Trim()
            if ($K -match '^"PPP_RESOURCE_ID_([^"]+)"') {
                $rail = $Matches[1]
                $found = 1
                continue
            }
            if ($found -ge 1 -and $K -match '^(0x[0-9A-Fa-f]+|\d+),?\s*$') {
                switch ($found) {
                    1 { $mode = $Matches[1] }
                    2 { $uv   = $Matches[1] }
                    3 { $ua   = $Matches[1] }
                    4 { $p4   = $Matches[1] }
                    5 { $p5   = $Matches[1] }
                }
                $found++
                if ($found -gt 5) { break }
            }
            if ($K -match '^\}') { break }
        }
        $null = $rows.Add("$device`t$component`t$fstate`t$rail`t$mode`t$uv`t$ua`t$p4`t$p5`t$($i + 1)")
        continue
    }
}

$rows | Out-File -LiteralPath $outTsv -Encoding utf8
"Wrote $($rows.Count - 1) PMICVREGVOTE rows to $outTsv"
