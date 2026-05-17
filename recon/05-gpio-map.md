# 05 — GPIO Map (pointer + deltas)

The authoritative GPIO inventory is `docs/03-bus-and-device-map.md` §4 ("TLMM, GIO0, and PMIC GPIOs"). It enumerates every TLMM/GIO0/PM01 pin actually resourced by some ACPI device, with the consumer device, direction, and purpose. As of 2026-05-16 it had:

- 46 distinct TLMM pins resourced in DSDT
- 8 TLMM pins firmware-reserved (`gpio-reserved-ranges = <0 4>, <126 4>`)
- 2 pmc8180c PMIC GPIOs claimed (gpio8 = BL_EN, gpio10 = BL_PWM)
- 27 EmuEC GPIO consumers (192 main event, 26/41/42/etc — see `docs/02-samsung-platform.md`)
- 4 QMP PHY-side GPIOs

## Findings since May 16 (relevant to iter-33)

1. **No orientation GPIOs exist on this board** — confirmed by Q6 reply (`research/2026-05-17-claude-q6-q7-urs-orientation.md`). Don't search.
2. **HSEI / GIO0 pin 35 (MOD1) appears in `\_SB.USB2._CRS Connection()`** but is **never written by AML**. iter-26's gpio-hog (output-high) is a viable workaround; per Q1 it may not even be needed.
3. **PD GPIO 41 (C1 edge) and 42 (C2 edge)** on `\_SB.GIO0` are the *only* lines that fire `EmuEC.CBLN` for USB-C connector status notifications. They feed the EmuEC, NOT the SoC dwc3/typec stack.

## Verifying a pin's role

```powershell
# Find every DSDT line that uses GIO0 pin N:
Select-String -Path "$PWD\acpi\dsdt.dsl" -Pattern "GIO0.*0x00$N|, $N,|Pin $N" -Context 0,4
```

(replace `$N` with the decimal pin number)

For TLMM mode/drive/pull config: those are NOT in DSDT in user-readable form — they're in the per-driver registry blob (`HKLM\SYSTEM\CurrentControlSet\Services\<driver>\Parameters\PinConfiguration`). If the iter-33 work needs a specific pin's pull state, that's a registry dig from Windows.
