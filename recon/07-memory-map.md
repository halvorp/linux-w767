# 07 — Memory Map (pointer + deltas)

Authoritative locations:

- `docs/03-bus-and-device-map.md` §9 — Reserved-memory regions (16 entries: mainline + W767 board overrides).
- `docs/04-soc-power-and-reset.md` §3 — MMIO base addresses for every SoC subsystem.

## Quick reference for iter-33

Most-cited MMIO bases:

| Subsystem | Base | Length | Notes |
|---|---|---|---|
| URS0 (a6f8800 dwc3 sub-IP) | `0x0A600000` | `0x100000` | _CRS in `\_SB.URS0` |
| URS1 (a8f8800 dwc3 sub-IP) | `0x0A800000` | `0x100000` | _CRS in `\_SB.URS1` |
| usb_mp (a4f8800 internal xhci) | `0x0A400000` | `0x100000` | _CRS in `\_SB.USB2` |
| UFS host (QCOM24A5) | `0x01D84000` | per DSDT | IRQ 31 |
| QMP USB3 PHY (per URS) | (within URS window) | — | mainline DTSI derives base from SoC offsets |
| MDSS / DPU / eDP | per sc8180x.dtsi | — | iter-17 works |
| ADSP / CDSP / MPSS load addrs | per qcom_q6v5_* drivers + reserved-mem | — | iter-31 working |

## Reserved-memory regions (W767 specifics)

W767-specific reserved regions are already in `dts/sc8180x-samsung-w767.dts` from prior iters. Cross-check `docs/03-bus-and-device-map.md` §9 if iter-33 has any "no free memory region for X" symptoms.

## Useful one-liner

```bash
# Pull every Memory32Fixed range in DSDT:
grep -B1 'Memory32Fixed' acpi/dsdt.dsl | grep -E 'Device|Memory32Fixed|0x[0-9A-F]'
```

That gives every memory window each ACPI device claims, which is the static MMIO map.
