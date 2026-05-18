# Ghidra analysis: `PanelDriver.sys` — verdict and Linux implications

**For:** brother instance on Linux side
**Triggered by:** user direction "now do the Ghidra work on PanelDriver.sys"
**Date:** 2026-05-18 (evening, after iter-62 fix-it report `68002cb`)
**Inputs:**
- `recon/PanelDriver.sys` (staged copy, 58 056 B, SHA-256
  `cdb7b15c5dfa79c70df0de3902021ddf7637b25544659ca263b2de2fe03b3739`)
- `recon/PanelDriverDump.java` (Ghidra headless script — Java, not Jython,
  because Ghidra 12.1 dropped Python without PyGhidra)
- `recon/ghidra_paneldriver_analysis.txt` (raw 244 KB stdout, gitignored)
- `recon/ghidra_paneldriver_clean.txt` (cleaned 244 KB plain-text, also
  gitignored — regenerable on demand)

## TL;DR — don't RE this driver further

`PanelDriver.sys` is a **104-function, 58 KB Microsoft-pattern panel manager
shim**. It does:

- KMDF device-add → ACPI _DSM-like call (`GFTV`/`AeiB`/`AeoB`) to fetch a
  `displayId`
- Registers two `PoRegisterPowerSettingCallback`s for OS power-policy events
- Exposes an `ExCreateCallback` object that exports two named operations:
  **`AuxRead`** and **`AuxWrite`**, indexed by `displayId`, taking
  `numBytes` / `bufferSize` / `bufferData`
- Other drivers (almost certainly `qcdxkm8180.sys`) invoke that callback
  via `ExNotifyCallback` to do DPCD-channel ops on the panel

It does **NOT**:

- Touch any GPIO (no GPIO API imports — zero `GpioClx*` or `Gpio*Acpi*`
  references)
- Touch any I²C (no `WdfIoTarget*Open*` for I²C, no IRP_MJ_DEVICE_CONTROL
  IOCTLs for I²C bus)
- Issue panel power-on sequences (the three `KeDelayExecutionThread` refs
  are all in HPD-related code paths, not sequence-stepped delays)
- Have any panel-specific magic numbers (no Samsung TCON command bytes,
  no BOE-specific OPCODES)

So the dreaded "we'll have to RE PanelDriver.sys to get the panel
power-on GPIO sequence" worry — that's a false alarm. The GPIO sequencing
lives in **ACPI methods owned by `SAFI` (SAM0701) + EmuEC (SAM0604)**,
NOT in `PanelDriver.sys`. And the DPCD ops PanelDriver.sys does as a
callback target are already done inline by mainline Linux's
`panel-edp` / `drm_dp_aux` infrastructure.

**Recommendation: STOP Ghidra work on PanelDriver.sys.** The iter-62
fix-it report's recommendations (EDID entry + gpio-hog on 23/25/35 +
regulator wiring + MMCX power-domain) are the correct Linux substitute
for everything PanelDriver.sys does on Windows. Save the Ghidra time for
qcdxkm8180.sys (huge, dispcc PLL tables potentially recoverable) or
qci2c8180.sys (relevant if we ever want the I²C controller to honour
Samsung's PEP votes).

## Evidence

### Binary fingerprint

```
Format:      PE32+ ARM64 (machine 0xAA64)
Compiler:    visualstudio:unknown
Build:       2020-03-03 06:11:59 UTC
PDB path:    E:\depot\space\src\Drivers\Display\ARM64\Release\PanelDriver.pdb
Size:        58 056 bytes
Sections:    .text (32 KB), .rdata (3.5 KB), .data (2 KB), PAGE, PAGED_CO,
             INIT, .pdata, .rsrc, .reloc
Functions:   104 (including DriverEntry, EvtDeviceAdd, AuxRead/AuxWrite
             handlers, KMDF/WDF boilerplate, security-cookie helpers)
```

### External imports — none of them are GPIO/I²C

```
NTOSKRNL.EXE imports actually used (with ref counts):
  ExAllocatePoolWithTag       (6)   memory
  ExFreePoolWithTag           (7)   memory
  ExCreateCallback            (3)   ← the export mechanism for AuxRead/AuxWrite
  ExRegisterCallback          (2)
  ExUnregisterCallback        (2)
  ExNotifyCallback            (5)   ← consumers fire the callback
  ExEventObjectType           (1)
  ObReferenceObjectByHandle   (2)
  ObfReferenceObject          (3)
  ObfDereferenceObject        (4)
  IoCreateNotificationEvent   (3)
  IoGetDeviceInterfaces       (2)   ← finds dxgkrnl panel device on first probe
  IoWMIRegistrationControl    (1)
  PoRegisterPowerSettingCallback   (2)   ← screen-on/off + lid-state
  PoUnregisterPowerSettingCallback (2)
  KeSetEvent                  (4)
  KeStallExecutionProcessor   (5)   ← spin-wait, not seq delay
  KeDelayExecutionThread      (3)   ← see "delays" section below
  MmGetSystemRoutineAddress   (1)
  RtlInitUnicodeString        (11)
  RtlCopyUnicodeString        (2)
  string helpers (strcmp, strcat_s, strchr, sprintf_s, ...) — boilerplate

HAL.dll imports:
  (only used indirectly through WPP tracing — nothing hardware-touching)

WDFLDR.SYS imports:
  WdfVersionBind / Unbind            (3 + 3)
  WdfVersionBindClass / UnbindClass  (3 + 3)   ← KMDF version negotiation
```

**No `WdfIoTargetCreate`, no `IoCreateDevice` for the panel I²C bus, no
GPIO IOCTL handles, no `WdfRequestSend` to a child interconnect.** This is
the strongest possible signal that PanelDriver.sys is a pure software
intermediary, not a hardware driver.

### The mystery `GFTV` / `AeiB` / `AeoB` strings — decoded

In `FUN_1400018c0` (the `EvtDeviceAdd` callback, 800 B body), I found:

```c
/* @ 0x140001a78 */
local_84 = 0;
local_8c = 0x56544647;   /* LE bytes 47 46 54 56 = "GFTV" */
local_7c = 0;
local_88 = 0x426f6541;   /* LE bytes 41 65 6f 42 = "AeoB" */
local_90 = 0x42696541;   /* LE bytes 41 65 69 42 = "AeiB" */
iVar1 = FUN_140001718(param_1, &local_90, puVar3, &local_88);
/* iVar1 = result of an ACPI _DSM-style IOCTL through dxgkrnl */
*(int *)(lVar2 + 200) = iVar6;   /* store displayId in WDF context */
```

`FUN_140001718` is a thin wrapper that takes the 4-tag buffer and issues
IOCTL **`0x32c004`** (looks like a custom dxgkrnl/qcdxkm IOCTL — not a
standard `IOCTL_VIDEO_*` code) through a function-table call.

Reading those 4-byte tags as little-endian ASCII strings:

| local | hex value     | LE-ASCII |
|-------|---------------|----------|
| local_90 | 0x42696541 | `AeiB`  |
| local_8c | 0x56544647 | `GFTV`  |
| local_88 | 0x426f6541 | `AeoB`  |

These three tags are **lookup keys** in a JSON-ish argument-passing
convention. The IOCTL gets a buffer; the GPU driver pattern-matches on
the keys; the returned integer is the `displayId` for the panel.

> **Cross-reference to my iter-56 work**: `AeoB` is also the magic 4 bytes
> at the head of `SPMD.bin` (the SCSS PM config blob, see iter-56 README).
> So `AeoB` is a Samsung-internal magic / structure-tag, NOT a GUID.
> "Aeon Boot" is my best guess for the expansion (Aeon = Samsung-internal
> codename across multiple W767 components).
>
> **`GFTV`** matches the Method (GFTV) stub I decoded inside
> `Device(\_SB.SSPN)` in iter-56 (the method that just `Return (Local0=0)`).
> So this is a Samsung-specific Aeon-namespaced API: PanelDriver.sys
> probes for the `GFTV` slot via the IOCTL bus, but on the W767 that slot
> returns 0 — meaning Samsung defined the API but didn't ship the
> implementation for this product. Effectively dead code.

### AuxRead / AuxWrite — what these actually do

`FUN_1400079b0` (440 B, called `AuxRead`):

```c
int FUN_1400079b0(longlong device_ctx,
                  uint param2_displayId, uint param3_flags,
                  ulonglong param4_address,
                  uint param5_bufferSize,
                  void *param6_outBuffer,
                  uint *param7_numBytes) {

  /* lazy-alloc 0x1000 scratch buffer */
  if (device_ctx[0x38] == 0 || device_ctx[0x40] < 0x1000) {
    void *buf = ExAllocatePoolWithTag(0x200, 0x1000, 'LDCQ');
    ...
  }
  if (param7_numBytes == NULL || param5_bufferSize < *param7_numBytes) return 0;

  /* build a key-value packet in the scratch buffer */
  local_98 = "AuxRead";
  pcStack_90 = "displayId";
  local_88 = &LAB_140008bb0;     /* GUID-prefix sentinel */
  pcStack_80 = "bufferSize";
  local_78 = "numBytes";
  FUN_140007fb8(scratch_buffer, &keys_struct, &kv_template,
                NULL, 5, param4_address,
                param2_displayId, param3_flags);

  /* fire the ExCreateCallback object — wakes any driver that subscribed */
  if (device_ctx[0x30] != 0) {
    ExNotifyCallback(device_ctx[0x30], scratch_buffer, &local_a0);
    /* decode response */
    return FUN_140008228(scratch_buffer, param6_outBuffer,
                         local_a0[0], param7_numBytes);
  }
  return 1;
}
```

`FUN_140007b70` (`AuxWrite`) is the same pattern but adds a `bufferData`
key and copies *in* instead of *out*.

So the runtime flow is:

```
Other driver (qcdxkm8180?)
  ExNotifyCallback("PanelDriver", { op: "AuxRead",  displayId, address, numBytes })
      → PanelDriver.sys formats packet, returns bytes
  ExNotifyCallback("PanelDriver", { op: "AuxWrite", displayId, address, bufferData })
      → PanelDriver.sys formats packet, writes bytes
```

These ARE DPCD reads/writes — the standard eDP register space (0x000000..)
that defines link rate, lane count, training pattern, panel self-refresh,
backlight DPCD, etc. Doing them through PanelDriver.sys's callback bus
lets Microsoft's display pipeline route them centrally instead of letting
qcdxkm8180 do them directly.

On Linux, the equivalent operations happen inline inside `drm_dp_aux_*`
calls from `drivers/gpu/drm/msm/dp/dp_aux.c` (no callback bus). Same
hardware operations, different software architecture.

### Delays — not panel power sequences

The three `KeDelayExecutionThread` call sites are all inside
`FUN_140003888` (1012 B) which appears to be a notification-listener
poll loop (registered via `IoCreateNotificationEvent`). Looking at the
neighbouring decompile, these delays are on the order of
`-1000 * 10000` (100-ms intervals) for IRP-completion waits — NOT
millisecond-precision panel power sequencing. No GPIO bit-banging exists
here.

### Entry / DriverEntry

```c
void entry(DriverObject, RegistryPath) {
    FUN_14000f160();                         /* __security_init_cookie  */
    FUN_140008560(DriverObject, RegistryPath); /* DriverEntry          */
}

ulonglong DriverEntry(DriverObject, RegistryPath) {
    WdfVersionBind(...);                     /* KMDF init             */
    FUN_1400087a8(...);                      /* WPP trace init        */
    FUN_140008708();                         /* device-interface lookup */
    FUN_14000f000(...);                      /* WdfDriverCreate proper */
    /* install IRP handler patches via DAT_14000a640 */
    return 0;
}
```

Boring. No magic constants. No hardcoded panel DPCD addresses. No
hardcoded GPIO pin numbers.

## What this confirms about the iter-62 fix-it plan

| iter-62 §  | Item                                | Did PanelDriver.sys help us? |
|------------|-------------------------------------|------------------------------|
| §1 GPU regulators        | `LDO3_C` / `LDO9_E`            | Already known from iter-56 DSDT — PanelDriver.sys doesn't touch regulators |
| §2 eDP PHY regulators    | `LDO3_C` / `LDO5_E`            | Already known — PanelDriver.sys doesn't touch them either |
| §5 MMCX power-domain     | `<&rpmhpd SC8180X_MMCX>`       | Already known from iter-56 DSDT |
| §6 SSPN GPIO toggling    | gpio-hog 23/25/35              | **CONFIRMED**: PanelDriver.sys doesn't issue them; ACPI methods do. The gpio-hog is the right Linux substitute |
| EDID                     | BOE TE133FHE-TS0 entry          | Already done in `ca22992` |
| Panel `.delay` tightening | placeholder values still fine | PanelDriver.sys has no panel-specific delays we could borrow |

In other words: **none of the iter-62 patches needed PanelDriver.sys; we
were already correct without it.** The Ghidra dive negated a follow-up
worry rather than producing new patches.

## What's actually worth Ghidra-ing next (if anything)

Ranked, with concrete Linux outcomes:

1. **`qcdxkm8180.sys` for the disp_cc PLL frequency tables** — only
   matters if we move the kernel past 6.6 (pre-6.8-rc1 DRM regression
   workaround). Linux's `clk-disp-cc-sc8180x.c` is missing rate entries
   for the W767's eDP path; the values are hardcoded in this huge
   (~25 MB) Microsoft binary. **Worth doing later, big effort.**
2. **`qci2c8180.sys` IOCTL surface** — only if we hit a bug where Samsung
   panel companion (i2c slave 0x2C on i2c15) needs custom timing/clock
   stretching beyond what the GENI I²C controller exposes. **Probably
   never needed.**
3. **`SafiDrv.sys` (Samsung Firmware Interface)** — does the actual
   GPIO sequencing for panel-enable. If our gpio-hog approach fails to
   bring up the cold-boot panel, we'd need to RE this to learn the
   millisecond delays between assertions. **Standby in case iter-63
   needs it.** Note: this binary is NOT yet imported in the GalaxyBookS
   Ghidra project — it's in the `safidrv.inf_arm64_*` DriverStore dir
   which I included in the bundle archive (see manifest).
4. **`EmuEC.sys` (SAM0604)** — the embedded controller that owns the
   GPIO pin block. Its IOCTL surface is what SafiDrv.sys calls into.
   **Already imported in the Ghidra project — could pair-RE with
   SafiDrv if needed.**

## Files staged this commit

- `research/2026-05-18-claude-iter62-ghidra-paneldriver.md` (this file)
- `recon/PanelDriverDump.java` (the Ghidra script — committed so future
  re-runs are reproducible)
- `recon/w767-w11-drivers.manifest.md` (manifest for the Google Drive
  archive)

Not committed (too big or regenerable):
- `recon/w767-w11-drivers.zip` — 50.8 MB, gitignored; upload to Drive
  via separate channel. See `recon/w767-w11-drivers.manifest.md` for what
  it contains + per-binary SHA-256s.
- `recon/PanelDriver.sys`, `recon/PanelManagerSvc.exe` — staged copies
  of the Windows binaries (58 KB and 37 KB respectively). Inside the
  archive zip; not separately committed.
- `recon/ghidra_paneldriver_*.txt` — analysis output; regenerable from
  the script.

## Bottom line for brother

iter-62 already had all the panel-power facts it needed (from the iter-56
DSDT walk). The Ghidra dive added one new datum:

> **PanelDriver.sys is not a hardware driver; it's an inter-driver
> DPCD-callback bus + an ACPI displayId-fetcher. Linux's mainline
> `panel-edp` + `drm_dp_aux` covers everything PanelDriver.sys does on
> Windows. No Linux replication needed.**

Proceed with the iter-63 patches from §1-8 of
`research/2026-05-18-claude-iter62-fixit-from-logs.md` as-is.
