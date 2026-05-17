# 06 — Bus Map (pointer + deltas)

The authoritative bus inventory is `docs/03-bus-and-device-map.md` §1–§5:

- §1 — Executive summary (22 QUP serial engines, 11 I²C controllers, 2 UARTs, 2 SPIs, 8 BAMs, 17 I²C slaves)
- §2 — QUP wrapper map (qupv3_id_0/1/2 and which engines belong to each)
- §3 — I²C controllers and their slaves
- §4 — TLMM GPIO inventory (see [05-gpio-map.md](05-gpio-map.md))
- §5 — SPI, UART, SLIMbus, SPMI

## Deltas / corrections since May 16

1. **CS35L41 amps confirmed SPI, not I²C** — DSDT _CRS on `\_SB.SPI1` with 4 chip selects at 4 MHz (CS 0,1,2,3); amps on CS 0/1. `\_SB.SPI4` cs=0 @ 24 MHz for high-speed sync. Updated `reference_w767_hardware.md` memory. Mainline DT binding `cirrus,cs35l41` supports SPI natively.
2. **USB-PD controller at I²C 0x09 on `\_SB.IC20`** — this is the chip whose CC status the EmuEC reads to expose `EMEC.CCST`/`CCS2`. Q6 reply has full chain.
3. **`\_SB.IC10`/`IC19`/`IC12` PMIC cluster** — confirmed primary PMIC/charger/gauge cluster at addresses 0x33/0x25/0x1A; mirrored on alt buses for SKU fallback.

## Useful one-liners

```bash
# Every I²C slave on a specific controller:
awk -F'\t' '$0 ~ /IC10/' docs/03-bus-and-device-map.md   # quick scan in the existing doc
```

```powershell
# Live: what's bound to which I²C controller from Windows side?
Get-PnpDevice -PresentOnly | Where-Object InstanceId -match 'ACPI\\.*\\.*I2C|ACPI\\IC1|ACPI\\IC2' | Select Status,InstanceId
```

There are no significant gaps in the May 16 bus map; nothing new has been discovered since then that wasn't already cited. This file exists so brother knows to reference `docs/03-bus-and-device-map.md` and not re-extract.
