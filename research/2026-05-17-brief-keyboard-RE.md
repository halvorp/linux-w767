# Research brief — SAMM0901 internal keyboard reverse engineering

**For:** brother instance (Claude on W767 under Win11 ARM64)
**Goal:** produce enough protocol-level evidence for a Linux platform driver to handle the W767's internal keyboard
**Tools available on the W767 side:** Ghidra (ARM64 native), PowerShell, registry, INF files, driver `.sys` binaries (user-readable, no TrustedInstaller lock per the earlier session's methodology)
**Tools NOT available:** x64 kernel drivers (no Prism for kernel-mode), so RW-Everything / OSR IrpTracker still out of scope

## Why this matters

After iter-22 (commit `fa31bf6`), the platform should bring up enough hardware to use external USB keyboards. But the **internal keyboard is THE blocker** for "daily-driver Linux on W767" per the realistic-outcomes audit:

> L5: Internal keyboard works | ~20–25% probability | months — there is no upstream pattern for this Samsung EC

Every other peripheral has a mainline path (touchpad = `hid-over-i2c`, audio = `cs35l41`, battery = `sbs-*`, WiFi = `ath11k`, etc.). The keyboard is the only piece with **zero upstream Linux precedent** and **no obvious downstream port** (Samsung's Android trees handle keyboards via VHF + ACPI Notify, but their VHF binding is Windows-only).

## What we already know (from the V1–V8 + recon rounds)

From DSDT (`acpi/dsdt.dsl`):

```asl
Device (SVBI)
{
    Name (_HID, "SAMM0901")
    Name (_SUB, "C17C144D")
    /* NO _CRS — no bus connection */
    /* NO _DEP — no declared dependencies */
}
```

And from `EmuEC.sys` strings (V7 era):

```
"OEM_Gen_Scan_Code"
Notify (\_SB.SVBI, Arg0)
```

Plus scancode samples for system hotkeys (V1 era, unverified):
- `0x01` Fn+F1 (Samsung Settings)
- `0x02 / 0x03` Brightness Down / Up
- `0x04` Display Toggle
- `0x06` Touchpad Toggle
- `0x08` Mute
- `0x09 / 0x0A` Volume Down / Up

So the mechanism is: **EC reads keyboard matrix** → **EC computes a scancode** → **EC asserts an interrupt to AP** → **EC driver dispatches `Notify(\_SB.SVBI, scancode)`** → **VHIDEvent.sys VHF handler interprets the scancode + emits HID reports**.

Where it gets murky: the **alphanumeric keys** (A–Z, 0–9, modifiers) — do they use a different scancode space than the hotkeys above? Do they use Set 1 / Set 2 PS/2 codes? Or a Samsung-internal mapping?

## What brother should produce

A `research/2026-05-1?-claude-keyboard-protocol.md` document that answers these questions, **each with smoking-gun evidence (Ghidra line, string match, INF excerpt, or registry value)**:

### Q1. What .sys binaries are in the keyboard path?

Enumerate the driver stack. Expected suspects:

```
EmuEC.sys                — EC bus driver, asserts the Notify
VHIDEvent.sys            — Microsoft Virtual HID Framework (VHF) consumer
kbdHelper.sys            — Samsung keyboard helper
```

For each:
- File path under `C:\Windows\System32\drivers\` (and DriverStore origin)
- `Get-PnpDevice` association: which device instance ID loads which `.sys`?
- INF that installed it (`oem*.inf` in `C:\Windows\INF\`)
- Hardware ID(s) bound

### Q2. What's the wire protocol from EC → AP for a keypress?

In `EmuEC.sys` (which we already have decompiled the I²C transfer layer of), find:

1. **The keyboard interrupt handler.** Strings to grep for in Ghidra:
   - `"OEM_Gen_Scan_Code"` (we know this exists)
   - `"KBD"`, `"KEY_"`, `"KEYBOARD"`
   - `"scan"` (case-insensitive)
   - `"PHID"` (the DSDT method invoked: `PHID = ProcessHID`?)
   - `"GenScanCode"`, `"KbdScan"`
2. **The function that builds the Notify argument.** It'll take a raw matrix scan code (from the EC's internal SMBus query) and translate it to the `Arg0` of `Notify(\_SB.SVBI, Arg0)`. Show the body.
3. **The scancode table.** Likely a static array of `(matrix_row, matrix_col, hid_keycode)` triples, or a function with a giant `switch (raw_code)` statement. Quote the full table.

### Q3. What's the VHF-side protocol from SVBI Notify → HID report?

Find and decompile the SAMM0901 ACPI driver / VHIDEvent extension:

1. **`AcpiNotifyHandler`** for SVBI. The Arg0 is the scancode delivered.
2. **HID report descriptor** — the static byte array that defines the keyboard HID interface (key map, modifier byte layout, LED outputs). Either embedded in `VHIDEvent.sys` or in the INF as a registry property under `HID_DESCRIPTOR_VALUE`.
3. **Scancode → HID usage code translation** — does the VHF driver pass the EC's scancode straight through as a HID usage code, or does it translate? If translate, the lookup table is what Linux needs.

### Q4. What hotkey scancodes does the user-mode hotkey daemon recognize?

Samsung Settings / Samsung Easy Settings runs as a user-mode service that listens for HID consumer-control events from the keyboard. The hotkey → action mapping is somewhere in:

- `C:\Program Files\Samsung\Settings\` (or similar) — look for INI / XML / JSON / registry-driven config
- HKLM\SOFTWARE\Samsung\Settings\Hotkeys (or analogous registry hive)

A definitive list of every hotkey (Fn+F1..F12, brightness, volume, touchpad toggle, airplane mode, etc.) with its scancode would let us map them all in the Linux driver in one go.

### Q5. Does the EC poll for keys or interrupt-drive?

In `EmuEC.sys`, find the path that sets up the keyboard polling. If it's interrupt-driven, there's a GPIO IRQ on `\_SB.GIO0` configured for the keyboard. If polled, there's a timer DPC. Either way, the Linux driver needs the same mechanism. Show:

- The interrupt registration (`WdfDeviceConfigureRequestDispatching`, `WdfInterruptCreate`)
- The polling timer setup, if any
- The actual matrix-scan I²C command the EC issues to its internal scan controller

### Q6. (Optional but high-value) What's the Fn key flow?

Fn is special: holding Fn turns the F1..F12 keys into hotkeys. Question: is that done in the EC (Fn modifier → different scancode for F1..F12) or in the OS (EC sends Fn-down + F-key, OS combines)? If in the EC, Linux gets the post-combined scancode for free. If in the OS, Linux needs to replicate the combiner logic.

## Deliverable format

Same structure as `research/2026-05-17-claude-recon-emuec-chip-id.md`:

- **§1. Confirmations** of existing repo claims (DSDT excerpts re-verified, prior scancode samples confirmed/refuted)
- **§2. Corrections** with evidence (anything wrong in the existing docs)
- **§3. New findings** — the actual protocol bytes, scancode tables, HID descriptor, etc.
- **§4. Architectural takeaways** — how to factor the Linux driver
- **§5. Methodology notes** — what worked in Ghidra, what didn't, address ranges decompiled

The §3 section is the most important — it's the table that Linux drives the keyboard from.

## Suggested Ghidra approach

1. **`ListFunctions.java`** (already in V7 zip) — dump all function entry points + names from `EmuEC.sys`, `VHIDEvent.sys`, `kbdHelper.sys`
2. **String-search pass** for `Scan`, `KBD`, `KEY_`, `PHID`, `SVBI`, `HID_DESC` — get references and decompile reachable callees
3. **`DecompileByAddrV2.java`** (already in V7 zip) — pull function bodies for the matches
4. **For the VHF / HID descriptor**: descriptors are static byte arrays. Search the binary's `.rdata` section for the standard HID descriptor magic bytes `05 01 09 06 A1 01` (Usage Page = Generic Desktop, Usage = Keyboard, Collection = Application). The next ~60 bytes is the descriptor — that's exactly what Linux needs to emit.

## Linux side: what we'll do with the answers

Once brother delivers the protocol doc, the Linux side will:

1. Write a `samsung,w767-kbd` platform driver (~300–500 lines) that:
   - Binds to the EmuEC's notification channel (mediated by the EmuEC platform driver — see §4 of the recon round chip-ID doc)
   - Registers an `input_dev` with the right key bitmap
   - On each notification, looks up the scancode → keycode via the table from §3, calls `input_report_key()` + `input_sync()`
2. Or, if the HID descriptor is straightforward: use `vhf` style — register a HID device via `hid_allocate_device()` + `hid_input_report()`, ship the descriptor bytes directly, let `hid-generic` map the usages to keys automatically. This is simpler if Samsung's descriptor matches a standard HID keyboard layout.
3. Add a quirks table for the hotkey scancodes that don't have HID usage codes (Samsung Settings shortcut, etc.) → map to KEY_PROG1 / KEY_FN_F1 / etc.

The ASoC / battery / amp work all stays in their lanes. The keyboard driver is the missing piece for L5 "internal keyboard works" — and brother's the only one who can produce the protocol evidence (we're on the Linux side with no Windows binaries to RE).

## Out of scope for this round (per brother's preference)

- **Don't** try to load x64 kernel drivers — RW-Everything etc. still don't work on ARM64 Win11
- **Don't** rerun the DSDT extraction — md5 still matches, it's stable
- **Don't** worry about the Fn-key combiner if Q5 is hard — getting alphanumeric keys + a few hotkeys is enough for v1; the rest can iterate

## Definition of done

A research markdown that, if I read it cold, lets me write the Linux keyboard driver without further questions about the protocol. Roughly:
- Driver stack identified (Q1)
- Scancode table dumped (Q2 + Q3)
- IRQ vs polling decided (Q5)
- Optionally: Fn flow and hotkey list (Q4 + Q6)

Push it as `research/2026-05-1?-claude-keyboard-protocol.md` and the Linux side will pick it up on next pull.
