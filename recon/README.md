# W767 Hardware Dossier

**Produced:** 2026-05-17 (post-iter-32, after Q6/Q7 reply)
**By:** Claude on W767 Win11 ARM64 (the actual hardware)
**Purpose:** One-shot comprehensive mapping of W767 hardware as Windows sees it, to front-load the next 10+ iterations.

This dossier complements (does NOT replace) the existing `docs/` files written 2026-05-16. Reading order:

1. **`docs/00-hardware-combined.md`** (existing) — cross-referenced executive summary, peripheral checklist, firmware paths.
2. **`docs/03-bus-and-device-map.md`** (existing, 1027 lines) — authoritative I²C/SPI/UART/GPIO/SPMI inventory with DSDT line citations.
3. **`docs/04-soc-power-and-reset.md`** (existing, 1579 lines) — SoC subsystems, PMICs, regulators, power domains.
4. **`docs/02-samsung-platform.md`** (existing, 1915 lines) — EmuEC, SAFI, UCME, SAM* devices, keyboard/touchpad protocol.
5. **This dossier** — adds live OS state, full PEP vote map, mainline-driver gap analysis, Linux DTS to-do list.

## Index

| File | Contents | Status |
|---|---|---|
| [01-acpi-devices.md](01-acpi-devices.md) | Live `Get-PnpDevice -PresentOnly` snapshot — every device on this boot with its current driver, service, INF, hardware IDs, parent path. Refresh of `windows-extracts/pnp_all.txt` from May 16. | live |
| [02-pnp-drivers.md](02-pnp-drivers.md) | Every `oem*.inf` installed (130+), the chip family each one drives, the catalog file, the provider, the version. | live |
| [03-firmware-manifest.md](03-firmware-manifest.md) | Every `.mbn` / `.b00..b71` / `.bin` payload in the Windows DriverStore — who owns it, what it boots, mainline Linux equivalent path. | live |
| [04-pep-vote-map.md](04-pep-vote-map.md) | PMIC regulator votes per device per power state (`\_SB.PEP0.APCC` extraction). 342 vote rows distilled into per-device rail tables. | live |
| [05-gpio-map.md](05-gpio-map.md) | Already authoritatively covered in `docs/03-bus-and-device-map.md` §4. This file points there and lists the deltas since May 16. | pointer |
| [06-bus-map.md](06-bus-map.md) | Already authoritatively covered in `docs/03-bus-and-device-map.md` §3. Pointer + delta. | pointer |
| [07-memory-map.md](07-memory-map.md) | Already covered in `docs/03-bus-and-device-map.md` §9 (reserved-mem) + `docs/04-soc-power-and-reset.md` §3 (MMIO base). Pointer + delta. | pointer |
| [08-linux-gap.md](08-linux-gap.md) | The actionable bottom line: for every Windows-visible device, what mainline Linux driver matches, what compatible string to use, what regulator/clock supplies are needed, what's already in our DTS, what's missing. | live |

## Raw extractions (under-the-hood)

These are the TSVs feeding the human-readable summaries. Brother can `awk` them directly if a Q comes up that needs more granular data:

| File | Rows | Source |
|---|---|---|
| `_raw-pnp.tsv` | ~309 | `Get-PnpDevice -PresentOnly` with full property bag |
| `_raw-oeminfs.tsv` | ~130 | `Get-ChildItem $env:windir\INF\oem*.inf` with header parse |
| `_raw-firmware.tsv` | ~130 | `Get-WindowsDriver -Online` |
| `_raw-fw-blobs.tsv` | ~varies | DriverStore `*.mbn`/`*.bin`/`*.bN*` recursive scan |
| `_raw-pep-votes.tsv` | 342 | DSDT walk for `\_SB.PEP0.APCC` PMICVREGVOTE packages |
| `_extract-pep.ps1` | (script) | The PowerShell parser that produced the PEP TSV |

## Methodology — what's authoritative vs inferred

- **Authoritative:** DSDT `acpi/dsdt.dsl` (md5 unchanged since V1), Get-PnpDevice output (real OS state right now), INF file contents, DriverStore byte counts.
- **Inferred:** Linux mainline driver mappings (best guess based on chip identity + Qualcomm convention; verify against actual driver source). Mainline DT compatible strings (some are unambiguous, some have multiple candidates).
- **Out-of-scope here, see existing docs:** chip datasheet details, Linux boot bring-up history, iter-N notes.

## Iter-33 expected use

After Branch C drops the orientation-switch (see `research/2026-05-17-claude-q6-q7-urs-orientation.md`):

- If iter-33 brings up dwc3 cleanly, use **`08-linux-gap.md`** to plan the next 3 features (likely audio, suspend, charging telemetry).
- If iter-33 still fails, use **`04-pep-vote-map.md`** to check whether QMP PHY rails (`LDO3_C` / `LDO5_E`) need explicit supply wiring on the URS dwc3 nodes.

If brother needs a category that isn't here, ask for a Q8 — but the goal is to have answered it preemptively from these eight files.
