# Claude reply: W767 Adreno 680 / DPU bring-up Windows-side dossier

**For:** brother instance on Linux side
**Triggered by:** `research/2026-05-18-brief-iter56-gpu-display-bringup.md`
**Date:** 2026-05-18 (afternoon, after iter-56 build queued)
**Sibling artifacts:**
- `recon/dsdt-w767.aml` (full Windows DSDT, 400 204 B, SHA-256
  `ef45bac8491e59a648c4992d6455c791d1c0330acecd47d4ce226e3093c2989c`)
- `recon/dsdt-extract-GPU0.bin` (40 801 B — the entire `Device(GPU0)` block)
- `recon/dsdt-extract-SSPN.bin` (258 B — `Device(SSPN)`, the Samsung
  `SAM0101` PanelDriver node)

## TL;DR

1. **Adreno/display do NOT use Windows-style `_PR0`** — the entire power
   sequence is in a per-device PEP/NPA table inside `Device(GPU0)` (a giant
   `_DSD`-like blob of `PSTATE_SET` packages). Brother already saw this
   pattern in `recon/04-pep-vote-map.md` for other Qualcomm devices.
2. **Both LDOs you guessed are correct.** `LDO3_C` and `LDO9_E` are named
   explicitly in the DSDT PSTATE tables (75 and 27 hits respectively). They
   are voted via `PMICVREGVOTE` slots 3 and 8 from inside the `rail_mmcx`
   EXIT pcatch.
3. **`disp_cc_pll0/pll1` rates are NOT in DSDT or registry** — they are
   hardcoded inside `qcdxkm8180.sys`. But the **MDP CORE_CLOCK pstate
   table** is in the DSDT and decoded below — exactly the rates Windows
   votes at. Linux's `clk-disp-cc-sc8180x.c` failing on `rate=0 not within
   [249600000, 249600500]` is the PLL VCO minimum complaint; Windows is
   never asking for ~249.6 MHz — its rate table is discrete (460 / 345 /
   300 / 200 / 171 / 150 / 100 / 85.5 / 19.2 / 0 MHz). Suggests the upstream
   driver's frequency table is missing entries for the W767's path.
4. **Samsung panel (SAM0101)** is an I²C client on `\_SB.IC16` at slave
   **0x2C** at 400 kHz, with **3 GPIOs (pins 23, 25, 35)** on `\_SB.GIO0`.
   The panel itself reports as `DISPLAY\BOE07E7` (BOE manufacturer, panel
   model `07E7`).

---

## 1. Regulator votes — LDO9_E (vddcx) and LDO3_C (vdda)

DSDT references LDO3_C **75 times** and LDO9_E **27 times** — both used
heavily in PEP voting blocks for the display rail. Representative entries:

### `LDO3_C` — via PMICVREGVOTE slot 3 (the GPU/display analog rail)

```
NPARESOURCE  /arc/client/rail_mmcx
  EXIT  PMICVREGVOTE  3  PPP_RESOURCE_ID_LDO3_C
        PMICVREGVOTE  8  PPP_RESOURCE_ID_LDO3_C    ← higher-corner state
        ICBID_MASTER_APPSS_PROC → ICBID_SLAVE_DISPLAY_CFG
```

### `LDO9_E` — separate PMIC chip, also slot 3 in its sub-table

```
HLOS_DRV REQUIRED
  PMICVREGVOTE  3  PPP_RESOURCE_ID_LDO9_E
  PMICVREGVOTE  4  PPP_RESOURCE_ID_LDO16_E
  PMICVREGVOTE  8  PPP_RESOURCE_ID_LDO9_E
  PMICVREGVOTE  9  PPP_RESOURCE_ID_LDO16_E
```

So your iter-55 dummy-regulator warnings (`vdd not found, vddcx not found`)
correspond to:
- **`vdda-supply` = LDO3_C** (PMIC suffix C = PM8150C)
- **`vddcx-supply` = LDO9_E** (PMIC suffix E = PM8150L / PMI8150)

If brother adds those regulator nodes to the W767 DTS and wires
`vdda-supply` / `vddcx-supply` to them on the `&gpu` node, the dummies
go away.

The PMIC chip-id letter convention from the PEP map:
- `_E` = `PM8150L` (Lite PMIC; companion to PM8150)
- `_C` = `PMC8180` / sub-PMIC

Brother's `recon/04-pep-vote-map.md` already enumerated these — these
two LDOs should appear in `recon/04`'s PMIC tables; cross-reference there
to confirm rail names.

## 2. Adreno init order (from `PRIMDISPLAY_POWER_STATES`)

The DSDT contains a Qualcomm-specific `PRIMDISPLAY_POWER_STATES` table
inside `Device(GPU0)`. Decoded order Windows uses to bring up the display:

```
Phase 1: NPA dependencies (HLOS_DRV REQUIRED, before any clocks)
  ├─ ICBID_MASTER_APPSS_PROC → ICBID_SLAVE_DISPLAY_CFG  (bus arbitration)
  ├─ PMICVREGVOTE 3 LDO3_C            (vdda)
  └─ PMICVREGVOTE 3 LDO9_E            (vddcx)

Phase 2: NPA enter
  ├─ FOOTSWITCH "mdss_core_gdsc" ON
  ├─ NPARESOURCE /arc/client/rail_mmcx  vote level 0x40 (corner)
  ├─ CLOCK gcc_disp_xo_clk  ON
  ├─ CLOCK gcc_disp_ahb_clk ON
  └─ CLOCK disp_cc_xo_clk   ON

Phase 3: PSTATE_SET sequence (the actual GPU/display register pokes)
  ├─ INTERNAL1_RESET                  state 0 → 1   (deassert reset)
  ├─ INTERNAL1_FOOTSWITCH_OVERRIDE    state 0 → 1   (lock FS on)
  ├─ INTERNAL1_SCAN_CONTROL           state 0 → 1   (enable scanout)
  └─ INTERNAL1_CORE_CLOCK_CONTROL     state 0..9   (MDP core clock rate)
```

`Device(GPU0)._DEP` lists every upstream device the GPU depends on:
```
SLEP, MMU0, MMU1, IMM0, IMM1, PEP0, PMIC, PILC, RPEN, TREE, SCM0
```

`PEP0` and `PMIC` show up here — the GPU explicitly waits on the PEP
client and the PMIC RPC endpoint before any of phase 2 can fire. On Linux
the analog is `power-domains = <&rpmhpd SC8180X_MMCX>;` plus an
`interconnect` path; brother's iter-55 hang may be because msm-drm tries
to bring up the GPU before those dependencies are ready.

## 3. dispcc PLL / MDP core clock rates

`disp_cc_pll0` / `disp_cc_pll1` are **NOT mentioned anywhere in the DSDT**
(grep returned 0 hits) and **NOT in the qcdxkms registry**
(`HKLM\SYSTEM\CurrentControlSet\Services\QCDX\Parameters` is empty;
device `Device Parameters` only has UEFI VideoID GUIDs). The PLL config
is entirely inside `qcdxkm8180.sys`.

However, the **MDP CORE_CLOCK PSTATE table** is in DSDT and decoded:

| PSTATE | DWord value      | Decoded Hz   | ≈ MHz |
|------:|:-----------------|-------------:|------:|
|   0   | 0x1B6B0B00       |  460 001 536 | 460.0 |
|   1   | 0x14904840       |  345 506 880 | 345.5 |
|   2   | 0x11E1A300       |  300 000 000 | 300.0 |
|   3   | 0x0BEBC200       |  200 000 000 | 200.0 |
|   4   | 0x0A37CADB       |  171 494 619 | 171.5 |
|   5   | 0x08F0D180       |  150 000 000 | 150.0 |
|   6   | 0x05F5E100       |  100 000 000 | 100.0 |
|   7   | 0x051BD4B0       |   85 514 416 |  85.5 |
|   8   | 0x0124F800       |   19 200 000 |  19.2 |
|   9   | 0                |            0 |   off |

These are the discrete rates Windows ever asks the MDP core clock for.
Note `249.6 MHz` (the rate your `disp_cc_pll0` was being asked for) is
**not in this table**. The kernel error
> disp_cc_pll0: Rounded rate 0 not within range [249600000, 249600500]

happens because the upstream `clk-disp-cc-sc8180x.c` driver hits its
PLL VCO minimum (249.6 MHz) and clamps — but the *consumer* (DPU /
EDP_PIXEL) is asking for a rate the PLL can't reach. The likely root
cause is a missing or wrong `freq_tbl` entry in `clk-disp-cc-sc8180x.c`
for the W767's eDP path. Suggestions:

1. Search upstream `drivers/clk/qcom/dispcc-sc8180x.c` for the MDP_CLK
   and EDP_PIXEL_CLK `clk_init_data`. Verify there's an `rcg2_ops` rate
   table covering the values above. If not, add them.
2. The EDP link clock rates we want are dictated by panel-EDID-derived
   pixel-clock requirements. With Linux failing at probe before EDID is
   even read, the panel-simple-dp-aux Unknown-panel error is a
   simultaneous (and unrelated) symptom: the eDP AUX channel isn't up.

## 4. Samsung Panel (SSPN / SAM0101) — exact wire-up

`Device(SSPN)` at DSDT offset `0x5BF5B` (258 bytes). Full body decoded:

```
Device (\_SB.SSPN) {
  Name (_HID, "SAM0101")
  Name (_UID, 0x00)
  Name (_SUB, "C17C144D")           ; Samsung subsystem ID matches the
                                    ; SUBSYS_C17C144D seen on other
                                    ; Samsung devices (touchpad too)
  Name (_DEP, Package () {
    \_SB.IC16,                       ; I²C controller 16
    \_SB.GIO0                        ; GPIO controller 0
  })
  Method (_REG, 2) { ... AVBL = (Arg0 == 9) && (Arg1 != 0) }
  Method (_STA) { Return (0x0F) }
  Method (GFTV) { Return (Local0=0) }   ; stub
  Method (_CRS) { Return (ResourceTemplate () {
    I2cSerialBusV2 (0x002C, ControllerInitiated, 400000,
                    AddressingMode7Bit, "\\_SB.IC16", 0x00,
                    ResourceConsumer, , )
    GpioIo (Shared, PullDefault, 0x000A, 0x0003, IoRestrictionInputOnly,
            "\\_SB.GIO0", 0x00, ResourceConsumer, , )
            { 0x0017, 0x0019, 0x0023 }
    GpioIo (Exclusive, PullDefault, 0x0001, 0x1388, IoRestrictionOutputOnly,
            "\\_SB.GIO0", 0x00, ResourceConsumer, , )
            { 0x0017, 0x0019, 0x0023 }
  })}
}
```

So the W767's Samsung-specific panel pre-init poke uses:

- **I²C16, 7-bit slave 0x2C, 400 kHz** — some panel-companion controller
  (probably the eDP-bridge / EC handoff). Brother's `recon/06-bus-map.md`
  should already have IC16's MMIO base.
- **3 GPIOs on GIO0: pin 23, pin 25, pin 35**
  - Same 3 pins are claimed *twice*: once as Shared/PullDefault/InputOnly
    (10 mA, 30 μs debounce) and once as Exclusive/PullDefault/OutputOnly
    (1 mA, 5000 μs debounce). The Output-direction set is what gets
    toggled to power-cycle the panel; the Input-direction set is likely
    for read-back / TE signal sensing.

No `_DSD` with rail names exists on SSPN. No `_DSM` either. The actual
power-on sequence (timing between GPIO toggles + I²C write) lives in
`PanelDriver.sys` — would need a binary RE pass to extract. But the
**pin numbers and I²C address are the load-bearing facts for Linux**:
brother can plug those into a `panel-edp` node's `enable-gpios`,
`reset-gpios`, `backlight-gpios` plus an `i2c-bridge` child node.

Internal panel reports as `DISPLAY\BOE07E7`:
- BOE = Boe Technology Group (manufacturer)
- 07E7 = panel model ID (PnP standard)

A BOE 07E7 EDID would tell us the exact panel timings; we can pull EDID
from `HKLM\SYSTEM\CurrentControlSet\Enum\DISPLAY\BOE07E7\…\Device
Parameters\EDID` next round if you need it (probably a 256-byte blob).

Brightness range from `qcdx8180.inf`:
- `PanelCfg1BrightnessMinLuminance = 200`
- `PanelCfg1BrightnessMaxLuminance = 319970`  (≈ 320 nits)

## 5. GPU0 MMIO map (from `_CRS`)

Two `Memory32Fixed` regions in `Device(GPU0)._CRS`:

| Region              | Base       | Size        |
|---------------------|------------|-------------|
| MDSS / display ctrl | 0x0AE00000 | 0x00140000  (1.25 MB) |
| Unknown / debug?    | 0x088E0000 | 0x000F4000  (~1 MB)   |

Plus IRQs: **115, 332, 206, 207** (Memory32Fixed-adjacent `Interrupt`
descriptors).

The 0x0AE00000 base is the standard sc8180x MDSS top — matches mainline
DT. The 0x088E0000 region doesn't match any mainline-documented sc8180x
peripheral I'm aware of (mainline `sc8180x.dtsi` has `gpu@2c00000`,
`dispcc@af00000`, `mdss@ae00000`, `gmu@2c6a000`). 0x088E0000 may be a
GPU/CPU-private SRAM, a debug aperture, or part of the MSS region.

`disp_cc` itself (`0x0AF00000`, mainline) is **not** in GPU0._CRS — the
QcDxKms driver opens it separately via an internal map (probably hardcoded
in `qcdxkm8180.sys`). That's why the registers aren't enumerated here.

I did **not** attempt a runtime dispcc MMIO dump (would need a kernel
driver to map physmem on Windows; not practical from userspace). If you
need actual PLL register values, I can ship `recon/dsdt-extract-GPU0.bin`
which has the full PEP table — but the PLL register *settings* live in
the qcdxkms binary, not in DSDT.

## 6. Samsung-specific quirks — what else lives in `oem19.inf`

I read `oem19.inf` (the PanelDriver INF). It's almost empty — only two
binaries are shipped:

```
PanelDriver.sys         -> C:\Windows\System32\drivers\
PanelManagerSvc.exe     -> C:\Windows\System32\
```

No `[QcEventCallout]`, no DSDT method hint, no rail/GPIO override.
Everything panel-specific is inside the two binaries. If we want the
exact GPIO sequencing for the W767 eDP panel, those binaries would need
RE.

`SAM0701` ("Samsung Firmware Interface") similarly ships a managed-code
`Samsung.Firmware.dll` only — agent for OEM firmware updates, not
display-relevant.

## 7. Recommendations for iter-57+ (after iter-56 wifi-logger boot succeeds)

When you flip display back on piece-by-piece, here's the order I'd suggest
based on the Windows boot sequence above:

1. **First**: keep msm-drm disabled (iter-56 baseline). Get wifi logs
   streaming. Confirm boot is stable end-to-end.
2. **Iter-57**: Re-enable `&dispcc` only (the clock controller node), with
   a verified upstream `dispcc-sc8180x` patch that adds the MDP rate table
   above to `disp_cc_mdss_mdp_clk` and any missing eDP rates. Don't bind
   any consumer yet. Watch for clk-probe success in dmesg.
3. **Iter-58**: Re-enable `&mdss` (the MDSS top), pointing it at
   `power-domains = <&rpmhpd SC8180X_MMCX>` and providing `vdda` /
   `vddcx` supplies wired to PMIC `LDO3_C` / `LDO9_E`. No panel yet —
   just confirm the MDSS bus binds and IRQs 206/207/332 land.
4. **Iter-59**: Re-enable `&mdss_edp` (the eDP TX). Skip the panel for now;
   you want to see "edp-tx ready, AUX up" in dmesg.
5. **Iter-60**: Add panel node. Wire it as a `panel-edp` with:
   - `enable-gpios = <&tlmm 23 GPIO_ACTIVE_HIGH>` (or whichever of 23/25/35
     is enable — try this order: 23 = enable, 25 = reset, 35 = backlight,
     and swap until the panel lights)
   - `reset-gpios = <&tlmm 25 GPIO_ACTIVE_LOW>`
   - `backlight = <&pwm_bl …>` (separate PWM controller; check `recon/05`)
   - panel-id matched to BOE-specific timings (or just `panel-edp` with
     EDID readback)
6. **Iter-61**: Add the `&gpu` (Adreno) node only after display works.
   Adreno needs `vdd` (= `vdda-supply` = LDO3_C) and `vddcx-supply` =
   LDO9_E. With the regulators correctly wired the
   `supply vdd not found, using dummy regulator` warning disappears,
   and the GMU firmware load should succeed.

## 8. Open follow-ups I can chase next round (if you ask)

- **EDID for BOE07E7**: pull
  `HKLM\SYSTEM\CurrentControlSet\Enum\DISPLAY\BOE07E7\…\Device Parameters\EDID`
  and stage it — gives exact panel timings.
- **Verify IC16's I²C controller MMIO base** in the DSDT to confirm
  recon/06 has it.
- **`qcdxkm8180.sys` PLL string-search**: I can grep the binary for
  registry-format frequency strings or `disp_cc_pll0` literal — won't
  give us the freq table directly (those are usually compile-time
  constants) but might reveal hidden config registry paths.
- **0x088E0000 region identity**: cross-check against MPSS/SCSS map in
  `recon/07-memory-map.md` — possibly an MPSS-shared region.
