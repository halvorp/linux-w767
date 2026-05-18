# dispcc PLL tables are NOT in `qcdxkm8180.sys` — investigation summary

**Date:** 2026-05-18
**Triggered by:** KIMI_2 §3 / brother's iter-56 §3 claim "The PLL config is entirely inside `qcdxkm8180.sys`"
**Outcome:** That claim is empirically wrong. The PLL programming goes through SCM (TrustZone secure-monitor call) — the actual M/N/L/alpha tables live in TZ firmware, not in this binary. **Don't burn more hours on this driver looking for PLL tables.**

## Evidence

### 1. Anchor found: dispcc clock-name registry at PE offset `0x1ae268`

72-byte struct per slot, format:
```
char name[56];   // null-terminated, null-padded — matches mainline clk-disp-cc-sc8180x.c exactly
u32  instance_lo;
u32  instance_hi;
```

Full names enumerated (matches upstream Linux dispcc-sc8180x clock enum 1:1):
```
disp_cc_mdss_byte0_clk / byte1_clk
disp_cc_mdss_pclk0_clk / pclk1_clk
disp_cc_mdss_esc0_clk  / esc1_clk
disp_cc_mdss_dptx{0..3}_{pixel0,pixel1,link,crypto,aux,link_intf,usb_router_link_intf}_clk
```

Plus debug-printf strings: `Core Clock Vote: %iMHz`, `Pixel clock frequency: %i.%iMhz`, `EDPPixelClockFrequency`, `DSIBitClockFrequency` — confirms this driver requests clock rates by name.

### 2. None of the dispcc/mdss MMIO base addresses appear as immediates

Searched as u32 LE AND u64 LE — zero hits for all of:
- `0xAE00000` (MDSS top)
- `0xAE01000` (MDP)
- `0xAE9A000` (eDP controller)
- `0xAEC2A00` (eDP PHY)
- `0xAF00000` (dispcc base)
- `0xAF01000` (dispcc PLL0 / PLL1 offsets)

If qcdxkm8180.sys programmed the PLL via direct MMIO writes, the base address would have to appear somewhere. It doesn't.

### 3. Zero PLL-register-name strings

Regex searches for `PLL_[A-Z_]+`, `Pll[A-Z]\w+`, `[Dd]ispcc_\w+`, `mdss_pll\w*` — all zero hits.

If the driver did `iowrite32(val, base + PLL_L_VAL)` style calls with named offsets, we'd expect to see those names in debug strings. We don't.

### 4. The actual clock-request mechanism: SCM

Strings present in binary:
```
SCMInit
SCMDeInit
SCMInterface
SCMajorVersion
SCMinorVersion
```

Plus `rabin2 -l` shows the driver imports only `ntoskrnl.exe`, `hal.dll`, `ksecdd.sys`, `wdfldr.sys` — standard Windows kernel modules. **No Qcom*.dll, no QcDispcc*.sys, no clock DLL.**

This proves the clock-rate-setting path is:

```
qcdxkm8180.sys
    → SCMInterface (SCM call into TrustZone, secure-world)
    → TZ firmware (signed; on the device at /lib/firmware/qcom/sc8180x/tz.mbn or similar)
    → AOP firmware (runs on the Always-On Processor, programs RPMh)
    → RPMh hardware → dispcc PLL register writes
```

The M/N/L/alpha values per output rate are in the **TZ firmware blob**, which is signed and not practical to reverse-engineer.

## Implications for the W767 Linux port

1. **The "Rate 0 not within VCO range" upstream Linux bug** (in `drivers/clk/qcom/dispcc-sc8180x.c`, present from kernel 6.8 onwards per KIMI_2's reading) is a Linux-side issue. **The fix lives in upstream Linux**, not in any Windows-side recoverable data. Either:
   - Wait for upstream
   - Empirically extend `freq_tbl[]` in `clk-disp-cc-sc8180x.c` based on Lenovo Flex 5G community work (same SoC) or the rates we can guess from Windows DSDT's MDP CORE_CLOCK pstate table (`460/345.5/300/200/171.5/150/100/85.5/19.2 MHz`)
2. **The pmOS 6.6 kernel works WITHOUT the missing freq_tbl entries** because the regression only bites 6.8+. So this bug doesn't block iter-61 onwards.
3. **Don't repeat this search on adjacent binaries** (`qcwlan8180.sys`, `qcauddev8180.sys`) for the same kind of data — they will also use SCM/PEP, not direct MMIO. Different binaries are right for different goals (see r2-analysis/qcdxkm8180-rate-tables.md sibling for the NoC rate tables we *did* find).

## What's still worth doing in `qcdxkm8180.sys`

- The clock-name registry at `0x1ae268` is a perfect anchor if we ever need to identify which Linux clock corresponds to a specific clock-ID in driver logs.
- The NoC rate tables at `0x1ab8d8` and `0x1aba18` (saved in sibling `qcdxkm8180-rate-tables.md`) might be useful when wiring up the `interconnect` driver bandwidth votes properly.

## Where to look for PLL configs if we ever really need them

- **Lenovo Flex 5G mainline branch** (same SC8180X SoC) — search for any out-of-tree `dispcc-sc8180x.c` patches in `aarch64-laptops/build` or jhovold/linux wip branches.
- **Empirical search** — bisect MDP CORE_CLOCK rates by setting them via `clk_set_rate` from userspace once display is fully bound (iter-65+ territory).
- **Lenovo OEM Linux kernel** if they ever publish one — unlikely but possible.

NOT worth doing: TZ firmware extraction / signature bypass.
