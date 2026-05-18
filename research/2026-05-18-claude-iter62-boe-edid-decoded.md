# BOE TE133FHE-TS0 EDID extracted + decoded

**For:** brother instance on Linux side
**Triggered by:** §1 of `research/2026-05-18-brief-iter61-pmos-pivot-for-brother.md`
**Date:** 2026-05-18 (evening, after iter-62 push)
**Sibling artifact:** `recon/edid-boe-07e7.bin`
  - 256 B total = 128 B base EDID block + 128 B all-zero padding (no
    extension block exists; Windows stores the field at a fixed 256 B size
    even when only the first 128 are populated)
  - SHA-256 `de7afa762abcc300bcf5c7b2f2cf5b5c4f97cfa77e9563fff1a260cae6ab98ab`
  - Base-block checksum verifies: sum of bytes 0..127 mod 256 = 0x00 ✓

## Provenance

Registry path the blob was read from:

```
HKLM\SYSTEM\CurrentControlSet\Enum\DISPLAY\BOE07E7\3&2fafef91&0&UID0\Device Parameters
    EDID  REG_BINARY  (256 bytes)
```

`3&2fafef91&0&UID0` matches the parent device path `ACPI(_SB_)#ACPI(GPU0)`
under `qcdxkm8180.sys`'s `BusRelations`, confirming this is the panel
attached to the W767's internal eDP TX.

## Identity (the only line you really need)

```
"BOE TE133FHE-TS0"   PnP BOE  Product 0x07E7  Mfg 2019-W14  EDID 1.4
1920x1080 @ 60.03 Hz  pixel clock 147.900 MHz  8 bpc  DisplayPort
```

So this is the **BOE TE133FHE-TS0** — a 13.3" FHD eDP panel, very close
sibling of the BOE NV133FHM family used in many ARM laptops. The "TE"
prefix appears to be a Samsung-OEM variant.

## Decoded base block (128 B)

| Offset       | Field                  | Value                                |
|--------------|------------------------|---------------------------------------|
| 0x00–0x07    | Header magic           | `00 FF FF FF FF FF FF 00` ✓          |
| 0x08–0x09    | Manufacturer ID        | 0x09E5 = **BOE**                     |
| 0x0A–0x0B    | Product ID (LE)        | 0x07E7                                |
| 0x0C–0x0F    | Serial number          | 1 (placeholder, not unique)           |
| 0x10         | Week of manufacture    | 14                                    |
| 0x11         | Year of manufacture    | 2019  (raw 0x1D + 1990)               |
| 0x12–0x13    | EDID version           | 1.4                                   |
| 0x14         | Video input definition | 0xA5 → **digital, 8 bpc, DisplayPort**|
| 0x15         | H screen size          | 29 cm (294 mm via DTD#1)              |
| 0x16         | V screen size          | 17 cm (165 mm via DTD#1)              |
| 0x17         | Gamma                  | 2.2                                   |
| 0x18         | Feature support        | 0x02 → preferred-timing bit only      |
| 0x19–0x22    | Chromaticity           | (not decoded — Linux uses defaults)   |
| 0x23–0x24    | Established Timings    | none                                  |
| 0x26–0x35    | Standard Timings       | all unused (0x0101 placeholders)      |
| 0x36–0x47    | DTD#1 (preferred)      | **see below**                         |
| 0x48–0x59    | DTD#2                  | all-zero (no monitor descriptor)      |
| 0x5A–0x6B    | DTD#3 → 0xFC           | Monitor name **"BOE LCD"**            |
| 0x6C–0x7D    | DTD#4 → 0xFE           | ASCII string **"TE133FHE-TS0"**       |
| 0x7E         | Extension flag         | 0x00 (no extension blocks)            |
| 0x7F         | Checksum               | 0x11 (verified mod-256 sum = 0)       |

### Preferred timing (DTD#1) — full decode

Raw 18 bytes: `c6 39 80 18 71 38 28 40 30 20 36 00 26 a5 10 00 00 1a`

| Parameter            | Value         | Computed from                              |
|----------------------|---------------|--------------------------------------------|
| Pixel clock          | **147 900 kHz** (147.900 MHz) | `0x39C6 × 10 kHz`         |
| H active             | 1920          | hi nibble 0x7 (b7-4 of 0x71) << 8 \| 0x80  |
| H blanking           | 280           | hi nibble 0x1 << 8 \| 0x18                 |
| H front porch        | 48            | 0x30, hi bits b7-6 of 0x00 = 0             |
| H sync pulse         | 32            | 0x20, hi bits b5-4 of 0x00 = 0             |
| H back porch         | **200**       | hblank − hfront − hsync                     |
| V active             | 1080          | hi nibble 0x4 (b7-4 of 0x40) << 8 \| 0x38  |
| V blanking           | 40            | hi nibble 0x0 << 8 \| 0x28                 |
| V front porch        | 3             | hi nibble 0x3 (b7-4 of 0x36)               |
| V sync pulse         | 6             | lo nibble 0x6 (b3-0 of 0x36)               |
| V back porch         | **31**        | vblank − vfront − vsync                     |
| H sync polarity      | **POSITIVE**  | bit 1 of flags 0x1A = 1                    |
| V sync polarity      | **NEGATIVE**  | bit 2 of flags 0x1A = 0                    |
| Sync scheme          | digital sep   | bits 4-3 of 0x1A = 11                      |
| H image size         | 294 mm        | 0x26 + (b7-4 of 0x10 = 0x1) << 8           |
| V image size         | 165 mm        | 0xA5 + (b3-0 of 0x10 = 0x0) << 8           |
| H total              | 2200          | hactive + hblank                            |
| V total              | 1120          | vactive + vblank                            |
| Refresh rate         | **60.03 Hz**  | 147 900 000 / (2200 × 1120) = 60.03        |

### Bandwidth budget (for the eDP link-rate selection)

```
pixel_clock × bpp = 147.9 MHz × 24 bpc = 3549.6 Mbps  (raw)
× 10/8 (8b10b)    = 4437 Mbps          (line rate needed)

  RBR  (1.62 Gbps/lane):  4 lanes × 1620 = 6480 Mbps  ✓
  HBR  (2.70 Gbps/lane):  2 lanes × 2700 = 5400 Mbps  ✓
  HBR  (2.70 Gbps/lane):  4 lanes × 2700 = 10800 Mbps ✓
```

So the panel works at **HBR / 2 lanes** or **RBR / 4 lanes**. SC8180X's
eDP TX has 4 lanes wired so 4-lane RBR is the most-permissive fallback;
DPCD probing should let `drm/msm` pick the lowest-power option.

## Drop-in entry for `drivers/gpu/drm/panel/panel-edp.c`

Add this in the `enum edp_panels` and the matching struct entry. The
existing pattern in mainline (see e.g. `boe_nv133fhm_n62`) is:

```c
static const struct drm_display_mode boe_te133fhe_ts0_mode = {
        .clock = 147900,
        .hdisplay = 1920,
        .hsync_start = 1920 + 48,        /* hfront */
        .hsync_end   = 1920 + 48 + 32,   /* hfront + hsync */
        .htotal      = 2200,             /* hfront + hsync + hback + hact */
        .vdisplay = 1080,
        .vsync_start = 1080 + 3,         /* vfront */
        .vsync_end   = 1080 + 3 + 6,     /* vfront + vsync */
        .vtotal      = 1120,
        .flags = DRM_MODE_FLAG_PHSYNC | DRM_MODE_FLAG_NVSYNC,
};

static const struct panel_desc boe_te133fhe_ts0 = {
        .modes = &boe_te133fhe_ts0_mode,
        .num_modes = 1,
        .bpc = 8,
        .size = {
                .width  = 294,
                .height = 165,
        },
        .delay = {
                /* conservative defaults; tighten once Windows' PanelDriver.sys is REd */
                .hpd_absent_delay = 200,
                .prepare        = 50,
                .enable         = 200,
                .unprepare      = 500,
        },
};
```

And in the `panel_edp_table[]`:

```c
{
        .name = "BOE TE133FHE-TS0",
        .panel_id = drm_edid_encode_panel_id('B', 'O', 'E', 0x07E7),
        .panel_desc = &boe_te133fhe_ts0,
},
```

With this entry, `panel-simple-dp-aux` will recognize the panel from its
EDID instead of emitting the `Unknown panel BOE 0x07e7, using
conservative timings` WARN — which is the symptom we saw in iter-61
dmesg.

## Notes for the iter-63+ panel work

1. **The `.delay` block above is a placeholder.** Optimal panel-power
   sequencing values (`prepare` / `enable` / `unprepare`) live inside
   `PanelDriver.sys` on the Windows side — RE pass needed if you want
   timings tighter than the conservative defaults. Until then, the values
   I picked are safe for a BOE eDP panel.
2. **No extension block.** Some BOE panels ship a 256-byte EDID with
   a DisplayID / CEA extension carrying DPCD link-rate hints. This one
   does not. Linux will DPCD-probe at link train time.
3. **The chromaticity bytes at 0x19–0x22 weren't decoded.** Brother can
   pull them later if color-managed pipelines start mattering; they don't
   affect boot or basic display.
4. **DTD#2 is all-zero.** That's a not-quite-spec-compliant filler
   (technically should be a "dummy descriptor" with tag 0x10), but Linux
   and Windows both accept it. No action needed.

## What still helps next

Remaining items from my iter-56 §8:

- IC16 MMIO base in `recon/06-bus-map.md` — still useful for SAM0101
  companion wiring (though the EDID alone unblocks the no-companion case).
- `qcdxkm8180.sys` PLL string-grep — long shot, low priority.
- 0x088E0000 region identity — long-term curiosity.

I'm switching focus next to reading the iter-61 dmesg + collect dump
to surface more concrete fix-it items (regulators showing dummy,
deferred probes, orphaned clocks, etc.). Report incoming.
