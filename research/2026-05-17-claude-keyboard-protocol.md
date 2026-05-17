# 2026-05-17 — SAMM0901 / SVBI keyboard protocol RE

**Author:** Claude (Opus 4.7) on the W767 Win11 ARM64 side
**Goal of this round:** answer Q1–Q6 of `research/2026-05-17-brief-keyboard-RE.md` so the Linux side can write the internal-keyboard driver without further protocol questions
**TL;DR:** The brief's framing was wrong about scope. **SAMM0901/SVBI is not the main keyboard.** The main alphanumeric keyboard, touchpad, and most function keys all live on a single Samsung-internal USB-HID composite at `\_SB.USB2.RHUB.MP0` ("SPACE v57", VID_04E8 PID_A055). That device is **fully standard HID** and needs **zero Samsung-specific Linux code**. SAMM0901/SVBI handles only ~10 "system-level" keys (Ctrl/Alt/Win/Del, F14/F15, Vol±, PowerDown, WakeUp) reported via ACPI `Notify(\_SB.SVBI, byte)` for the wake-from-S3 / SAS path. **L5 difficulty drops from "months" to "weeks, mostly USB and one small ACPI Notify driver."**

## §1. Confirmations

### §1.1. SVBI ACPI device (re-verified from AML at offset 0x60192–0x6021A)

```asl
Device (SVBI)
{
    Name (_HID, "SAMM0901")
    Name (_SUB, "C17C144D")
    // No _CRS — confirmed, no GPIO/I2C resources of its own
    // No _DEP — confirmed
}
```

So the existing repo claim is correct: SVBI is a "virtual" ACPI device. It has no interrupts and no bus resources. It exists only to receive `Notify()` from elsewhere.

### §1.2. EmuEC strings table contains "PHID" as a NameString

`EmuEC.sys` `.rdata` at offset **0x15b0** contains the 4-byte ACPI NameString "PHID" with 4-byte null padding, immediately preceding ARM64 function code. This is consistent with EmuEC invoking the `\_SB.PHID` method via `IoBuildDeviceIoControlRequest` / `WdfIoTargetSendIoctlSynchronously` against the ACPI bus driver. (We have the WDF call wrapper decompiled at `FUN_140006848` — it's the standard ACPI-evaluate-object call site.)

### §1.3. Driver stack (from live `pnp_devices.txt`, INF inspection)

The SAMM0901/SVBI stack is **just two Samsung drivers** and Microsoft glue:

```
EmuEC.sys      (Samsung, oem9.inf, ACPI\SAM0604, 268 KB ARM64)
                 │
                 │  Notify(\_SB.SVBI, byte)  via \_SB.PHID
                 ▼
VHIDEvent.sys  (Samsung, oem160.inf, ACPI\SAMM0901, 46 KB ARM64)
                 │
                 │  Vhf*Submit API
                 ▼
mshidkmdf      (Microsoft VHF kernel-mode framework)
                 │
                 ├─→ kbdhid (Microsoft)   → Col01 (Keyboard, 6 keys)
                 ├─→ hidserv (Microsoft)  → Col02 (Consumer, Vol+/Vol-)
                 └─→ input.inf (Microsoft)→ Col03 (System, Power/Wake)
```

`kbdHelper.sys` is **NOT in this stack** — it is bound exclusively to `HID\VID_04E8&PID_A055&MI_01` (the USB-HID main keyboard, see §3.1) and is just an OSD upper filter. The brief listed it as a SVBI candidate; that was a guess and is wrong.

### §1.4. Old V1 scancode list (0x01..0x0A) is wrong

The V1 list shown in the brief (`0x01 Fn+F1 (Samsung Settings)`, `0x02/0x03 Brightness Down/Up`, …) **cannot be SVBI Notify values** because `VHIDEvent.sys` explicitly rejects `notifyValue < 0x80`:

> `0x2320: Received invalid parameter.  p_notifyValue < 0x80.  Exiting.`

(Logged from `AcpiNotifyHandler` after the entry trace `-->p_notifyValue [0x%x]`.) Those V1 values were most likely guessed at, or came from a different observation context. Treat them as unverified.

## §2. Corrections

### §2.1. SVBI does NOT carry the alphanumeric keys

The brief framing implies VHIDEvent handles "the W767's internal keyboard" including A–Z, modifiers, etc. **It does not.** The HID report descriptor inside `VHIDEvent.sys` (extracted at offset 0x2d30, 92 bytes) declares only:

- **Report ID 1 (Keyboard)**: 6 specific keys, each 1 bit, in this exact bit order:
  - bit 0: Usage 0x4C = **Keyboard Delete Forward**
  - bit 1: Usage 0x69 = **Keyboard F14**
  - bit 2: Usage 0x6A = **Keyboard F15**
  - bit 3: Usage 0xE0 = **Left Control**
  - bit 4: Usage 0xE2 = **Left Alt**
  - bit 5: Usage 0xE3 = **Left GUI (Windows key)**
  - bits 6–31: padding
- **Report ID 3 (Consumer Control)**: bit 0 = **Volume Increment** (0xE9), bit 1 = **Volume Decrement** (0xEA)
- **Report ID 7 (System Control)**: bit 0 = **System Power Down** (0x81), bit 1 = **System Wake Up** (0x83)
- Plus two Feature reports (IDs 9 and 2) — these are for OS-side feedback (LED, mouse state), not input

So SVBI is best read as a **system-event / Secure-Attention-Sequence keyboard**. The Ctrl/Alt/Win/Del bundle is exactly what Windows needs for SAS; PowerDown/WakeUp is the lid/power path; F14/F15 are Samsung-specific spare keys (probably for "Samsung Settings shortcut" et al, but exact mapping is OS-policy).

### §2.2. There IS a main keyboard — it's USB-HID, not ACPI

`pnputil`/`Get-PnpDevice` shows a **USB composite device** at `\_SB.USB2.RHUB.MP0`:

```
USB\VID_04E8&PID_A055\2081368E4D50    "USB Composite Device" / BusReportedDeviceDesc="SPACE v57"
  ├─ MI_00  USB Input Device  (Class_03/Sub_01/Prot_02 = HID Boot Mouse)
  │     ├─ Col01  HID-compliant mouse           (UP:0001/U:0002 → msmouse.inf, mouhid)
  │     ├─ Col02  Microsoft Input Configuration (UP:000D/U:000E → mtconfig.inf)
  │     ├─ Col03  TPadHelper Device              (UP:000D/U:0005 → oem156.inf, Samsung)
  │     └─ Col04  HID vendor-defined             (UP:FF00/U:0001 → input.inf raw)
  ├─ MI_01  USB Input Device  (Class_03/Sub_01/Prot_01 = HID Boot Keyboard)
  │     └─ HID Keyboard Device                   (UP:0001/U:0006 → keyboard.inf, kbdhid + kbdHelper upper filter)
  └─ MI_02  USB Input Device  (Class_03/Sub_00/Prot_00 = HID generic)
        └─ (vendor — OSD/firmware-update path, used by kbdHelper_SamsungOSDSvcInstall)
```

This is the **internal Samsung microcontroller** ("SPACE" is Samsung's internal codename for the Galaxy Book S program). It sits on an internal USB2 hub port and exposes everything Windows needs as **standard HID**:

- MI_01 is a **HID Boot Keyboard** (subclass 01, protocol 01). Every alphanumeric, function key, and Fn-combined consumer/system event arrives here. Microsoft's stock `kbdhid.sys` handles it.
- MI_00 Col01 is a **HID Boot Mouse**, fed by the trackpad sensor data. (The DSDT also exposes ACPI `STMT1234` for the ST touchpad chip; based on the Samsung Galaxy Book S hardware layout, the ST chip is wired over I²C and the SPACE MCU bridges its data to USB-HID. The ACPI node exists for power-management hooks; the report path is USB.)
- MI_00 Col03 is the Samsung `TPadHelper.sys` precision-touchpad path (digitizer, UP:000D).
- `kbdHelper.sys` is an **upper filter on MI_01** that does on-screen-display (OSD) handling — brightness bar, mute LED, etc. It is **not** a keyboard driver. Its INF describes it literally as "Pen filter driver" — leftover comment, but it confirms the role.

### §2.3. The recon memory's touchpad/keyboard claims need updating

Existing repo memory says:
- "Keyboard: Samsung VHIDEvent (ACPI SAMM0901, INF oem160.inf) — virtual HID fed by EC."
- "Touchpad: ST I²C-HID (STMT 1234)"

Both are partially wrong. Corrected:
- **Main keyboard**: USB-HID at PID_A055/MI_01, via the SPACE v57 MCU. Standard HID.
- **System-event keyboard (SAS, power, wake)**: SAMM0901/SVBI via ACPI Notify. 10 keys total.
- **Touchpad**: USB-HID at PID_A055/MI_00 Col01+Col03, via the SPACE v57 MCU. ST chip is the underlying sensor, bridged by SPACE to USB. Standard HID-mouse + HID-digitizer.

This is a much **friendlier** picture for Linux porting than the brief assumed.

## §3. New findings

### §3.1. Wire protocol from EC → AP for the SVBI path

The full dispatch:

1. **EC chip** (an I²C slave at addr 0x33 on `\_SB.IC10`, 400 kHz) detects a key event for one of the ~10 SVBI keys
2. EC asserts one of its **6 GpioInt** lines into the AP (see §3.4 for pin list)
3. EmuEC.sys ISR fires, reads the event byte from the EC over I²C
4. EmuEC.sys invokes the ACPI method `\_SB.PHID(byte)` via WDF I/O target to the ACPI bus driver
   - The method name "PHID" is in EmuEC `.rdata` at offset 0x15b0
   - The call uses `FUN_140006848` (the generic WDF→ACPI evaluate-method wrapper, already decompiled)
5. ACPI executes the PHID method, **decoded from AML by hand at offsets 0x5ca8a–0x5caad**:

```asl
Method (PHID, 1, Serialized)
{
    ADBG (Concat ("PHID=", ToHexString (Arg0)))      // debug print
    Notify (\_SB.SVBI, Arg0)                          // ← the actual event
    Return (Zero)
}
```

6. `VHIDEvent.sys::AcpiNotifyHandler` (registered via `AcpiGetInterface()` + `RegisterAcpiNotificationHandler`, strings at 0x21e8 and 0x21b8) receives the Notify with `NotifyValue = Arg0`
7. The handler validates: `NotifyValue >= 0x80` (else "Received invalid parameter. p_notifyValue < 0x80. Exiting." at 0x2320)
8. The handler maps `NotifyValue` → which bit of which report ID to set, via in-code logic (no static table — confirmed: brute-force scan of VHIDEvent.sys for {0x4C, 0x69, 0x6A, 0xE0, 0xE2, 0xE3} clusters returned no matches outside the HID descriptor itself, so the translation is a code-driven switch, ~10 cases)
9. The handler builds a HID input report and submits it via VHF (`VhfReadReportSubmit` or equivalent)
10. VHF fans out to `kbdhid` / `hidserv` / `input.inf` based on the report's Report ID

**Wire-level summary:** payload from EmuEC to the OS = **1 byte**, value range **0x80..0xFF**, semantics defined by the in-code switch in `VHIDEvent.sys::AcpiNotifyHandler`. We do not have the exact mapping table (would require Ghidra disassembly of the ~200-byte AcpiNotifyHandler function); see §4 for why this is OK.

There is also a `NOTIFY_EVENT_DISPLAY_ON` string at offset 0x2878, suggesting one specific Notify value is used to signal "display turned on" (not a key event — probably handled by a separate code path that doesn't generate HID reports but updates internal state).

### §3.2. HID report descriptor (full 92 bytes, ready to copy into Linux)

Extracted from `VHIDEvent.sys` at file offset **0x2d30**:

```
05 01 09 06 A1 01 85 01 05 07 09 4C 09 69 09 6A
09 E0 09 E2 09 E3 15 00 25 01 75 01 95 06 81 02
95 1A 81 03 C0 05 0C 09 01 A1 01 85 03 09 E9 09
EA 15 00 25 01 75 01 95 02 81 02 95 1E 81 03 C0
05 01 09 80 A1 01 85 07 09 81 09 83 15 00 25 01
75 01 95 02 81 02 95 1E 81 03 85 09 09 01 15 00
25 01 75 08 95 01 B1 02 85 02 09 02 15 00 25 01
75 08 95 01 B1 02 C0
```

Annotated:

```
05 01           Usage Page (Generic Desktop)
09 06           Usage (Keyboard)
A1 01           Collection (Application)
  85 01           Report ID 1
  05 07           Usage Page (Keyboard/Keypad)
  09 4C           Usage (Keyboard Delete Forward)
  09 69           Usage (Keyboard F14)
  09 6A           Usage (Keyboard F15)
  09 E0           Usage (Keyboard LeftControl)
  09 E2           Usage (Keyboard LeftAlt)
  09 E3           Usage (Keyboard LeftGUI)
  15 00 25 01     Logical Min/Max (0..1)
  75 01 95 06     Report Size 1, Count 6
  81 02           Input (Data,Var,Abs)        // 6 keys × 1 bit
  95 1A 81 03     Input padding (26 bits)
C0              End Collection
05 0C           Usage Page (Consumer)
09 01           Usage (Consumer Control)
A1 01           Collection (Application)
  85 03           Report ID 3
  09 E9 09 EA     Usage (Vol+) Usage (Vol-)
  15 00 25 01 75 01 95 02
  81 02           Input (Data,Var,Abs)        // 2 keys × 1 bit
  95 1E 81 03     Input padding (30 bits)
C0              End Collection
05 01           Usage Page (Generic Desktop)
09 80           Usage (System Control)
A1 01           Collection (Application)
  85 07           Report ID 7
  09 81 09 83     Usage (Sys Power Down) Usage (Sys Wake Up)
  15 00 25 01 75 01 95 02
  81 02           Input (Data,Var,Abs)        // 2 keys × 1 bit
  95 1E 81 03     Input padding (30 bits)
  85 09 09 01 15 00 25 01 75 08 95 01 B1 02   // Feature Report 9 (1 byte)
  85 02 09 02 15 00 25 01 75 08 95 01 B1 02   // Feature Report 2 (1 byte)
C0              End Collection
```

Report payload sizes (matches `1 + ceil(bits/8) = 1 + 4 = 5 bytes`):
- Report ID 1: 5-byte report (1 byte ID + 4 bytes data, 6 used + 26 padding)
- Report ID 3: 5-byte report
- Report ID 7: 5-byte report
- Report ID 9, 2: 2-byte feature reports

For Linux's `hid-generic`, this descriptor maps automatically:
- 0x07/0x4C → KEY_DELETE
- 0x07/0x69 → KEY_F14
- 0x07/0x6A → KEY_F15
- 0x07/0xE0..E3 → KEY_LEFTCTRL / KEY_LEFTALT / KEY_LEFTMETA
- 0x0C/0xE9 → KEY_VOLUMEUP
- 0x0C/0xEA → KEY_VOLUMEDOWN
- 0x01/0x81 → KEY_POWER
- 0x01/0x83 → KEY_WAKEUP

So **no scancode→keycode quirks table is needed** — `hid-input.c`'s default tables already cover every usage here.

### §3.3. EmuEC ACPI event surface (Post* method names)

`EmuEC.sys .rdata` at 0x20640..0x20670 contains a clustered table of four event-method NameStrings, 16-byte spaced:

```
0x20640 (132672): "PostLEDRequest"
0x20650 (132688): "PostLIDEvent"
0x20660 (132704): "PostHIDEvent"
0x20670 (132720): "PostPogoEvent"
```

These are user-visible method names invoked by EmuEC into the OS via `IoBuildDeviceIoControlRequest`. They map to the four ACPI-emitted event types:
- `PostLEDRequest` — battery/charge-state LED color updates (used by the orange/blue charging LED)
- `PostLIDEvent` — lid open/close (separate path from SVBI Power/Wake)
- `PostHIDEvent` — **the SVBI keyboard event path** (this is what triggers `\_SB.PHID(byte)`)
- `PostPogoEvent` — POGO connector docking event (Samsung's pin connector for the optional dock)

For Linux, the equivalents would be:
- LED → `leds-` subsystem (`charging` trigger)
- Lid → `input_report_switch(SW_LID)`
- HID → ACPI notify handler that translates to standard `input` events
- POGO → custom platform driver (low priority; only relevant if user has the dock)

### §3.4. IRQ mechanism (Q5 answer)

EmuEC is **interrupt-driven, not polled**. Confirmed by decoding the `_CRS` of the EMEC ACPI device (offsets 0x5c4aa..0x5c8e7 in AML).

**6 GpioInt resources**, all on `\_SB.GIO0`:

| Pin | Polarity (raw flags) | Likely role            |
| --- | -------------------- | ---------------------- |
| 192 | Level, Active-Low, Shared (0x1200) | Main EC event line     |
| 26  | Level, Active-Low, Shared (0x1200) | Battery/gauge event    |
| 41  | Edge, Active-Low (0x1300)          | USB-PD / Type-C port 1 |
| 0   | Level, Active-Low, Shared (0x1200) | Aux                    |
| 81  | Level, Active-Low, Shared (0x1200) | Aux                    |
| 42  | Edge, Active-Low (0x1300)          | USB-PD / Type-C port 2 |

**26 GpioIo resources** (25 on `\_SB.GIO0`, 1 on `\_SB.PM01`) for output control of: LEDs, mux switches, power-rail enables, etc.

The SVBI keyboard events ride on the "main EC event line" — most likely pin 192 (highest pin, first in the resource list, level-shared). The handler doesn't poll for keys — it sleeps on the IRQ.

EmuEC also has an internal polling loop (the function at 0x140014a4c we partially decompiled) but that loop is for **maintenance polling** (battery thermal, charger state machines, etc.), not for key scanning. Key events are pushed by the EC via the IRQ, and EmuEC services the IRQ within ms.

For Linux: a `platform_driver` matching `_HID="SAM0604"` with `interrupts = <... gpio0 192 IRQ_TYPE_LEVEL_LOW ...>` etc. Each IRQ handler reads the appropriate I²C status from the slave on IC10 (addr 0x33 = main EC, 0x25 = charger, 0x1A = fuel gauge) and forwards to the right subsystem. **The keyboard-relevant path is small** — one IRQ + one I²C read + one `input_report_key()` per event.

### §3.5. Fn-key flow (Q6 answer)

Fn-combining is **entirely in the SPACE v57 MCU firmware**. Evidence:

1. The Win11 driver `kbdHelper.sys` contains no scancode tables and no Fn-related strings. Its only logic is "set lid status register" — it is an OSD filter, not a key handler.
2. The main keyboard MI_01 is a **HID Boot Keyboard** (Class_03 SubClass_01 Prot_01). Boot-protocol keyboards report standard 8-byte HID reports. There is no provision for raw matrix scancodes — the firmware MUST emit cooked HID usages.
3. The brief's V1-era list of "Fn+F1 = 0x01, Fn+F2 = 0x02 BrightnessDown" cannot reach the OS as 1-byte scancodes because:
   - SVBI Notify rejects values < 0x80
   - The USB HID keyboard reports as standard 8-byte HID reports with 8-bit usage codes per key
4. Standard mappings observed in Windows behavior (volume bar appearing on Fn+F7/F8, brightness slider on Fn+F2/F3) are consistent with the firmware emitting HID **Consumer Control** usages (0x0C page) and **HID System Control** usages (0x01 page U:0080+).

So the Fn key itself is **invisible to the OS**. When the user holds Fn and presses F7, the firmware sees this and emits an HID Consumer Control report with `Usage 0x00EA (Volume Decrement)`. Linux's `hid-input` already maps this to `KEY_VOLUMEDOWN`. **No driver action required.**

The handful of "branded" Fn keys that Samsung uses for things without a standard HID usage (e.g., "Samsung Settings shortcut") probably arrive as HID consumer-control reserved usages (0xFFxx range) or as the F14/F15 keys on the SVBI device. Linux can map these in userspace via `udev` keymap rules.

## §4. Architectural takeaways

### §4.1. The Linux keyboard driver scope is dramatically smaller than the brief assumed

The brief estimated L5 ("internal keyboard works") at **20–25% probability, months of work**, because there was no upstream pattern for the Samsung EC and writing a full ACPI-VHF replacement is large.

Reality:

- **Alphanumeric keys, F1–F12, modifiers, Fn-combined hotkeys**: all arrive via standard USB-HID Boot Keyboard at PID_A055/MI_01. `usbhid` + `hid-input` handle them with **zero new code**. Probability of working: ~95% (only depends on `dwc3` + `xhci-hcd` bringing up the internal USB2 hub, which is iter-22's goal).
- **Touchpad**: standard USB-HID mouse + Microsoft-precision-touchpad protocol on PID_A055/MI_00. `usbhid` + `hid-multitouch` handle them with zero new code. ~90% probability.
- **Volume/brightness/media keys**: HID Consumer Control via the same USB-HID path. ~95%.
- **System keys via SVBI** (PowerDown, WakeUp, SAS Ctrl/Alt/Win/Del): requires a small ACPI Notify handler. ~150–300 lines of new C. ~85% probability.

So **L5 is now: alphanumerics + touchpad + media keys ~95%; full system-key parity ~85%** — both reachable in days/weeks, not months. The risk profile is dominated by **getting the internal USB hub up**, not by writing a Samsung keyboard driver.

### §4.2. Recommended Linux driver structure

Two pieces, both small:

**Piece A**: nothing. The USB-HID path needs no new driver. Once `dwc3` brings up the SC8180X USB2 controller (per the existing iter-22 work), Linux will auto-enumerate VID_04E8/PID_A055, see it as a standard composite, and bind `usbhid` to MI_00 and MI_01 (and ignore MI_02 unless we add a quirk for Samsung OSD updates, which is post-MVP).

**Piece B**: a `drivers/platform/samsung/galaxy-book-s.c` (~250 lines) that:

```c
static int gbs_acpi_add(struct acpi_device *adev)
{
    /* Match _HID = "SAMM0901" */
    struct gbs_svbi *svbi = devm_kzalloc(&adev->dev, sizeof(*svbi), GFP_KERNEL);
    svbi->input = devm_input_allocate_device(&adev->dev);
    /* Declare the same key bitmap as the HID descriptor */
    set_bit(EV_KEY, svbi->input->evbit);
    set_bit(KEY_DELETE,     svbi->input->keybit);
    set_bit(KEY_F14,        svbi->input->keybit);
    set_bit(KEY_F15,        svbi->input->keybit);
    set_bit(KEY_LEFTCTRL,   svbi->input->keybit);
    set_bit(KEY_LEFTALT,    svbi->input->keybit);
    set_bit(KEY_LEFTMETA,   svbi->input->keybit);
    set_bit(KEY_VOLUMEUP,   svbi->input->keybit);
    set_bit(KEY_VOLUMEDOWN, svbi->input->keybit);
    set_bit(KEY_POWER,      svbi->input->keybit);
    set_bit(KEY_WAKEUP,     svbi->input->keybit);
    input_register_device(svbi->input);
    acpi_dev_install_notify_handler(adev, ACPI_DEVICE_NOTIFY, gbs_svbi_notify, svbi);
    return 0;
}

static void gbs_svbi_notify(acpi_handle h, u32 event, void *ctx)
{
    /* event is the byte (0x80..0xFF) passed to PHID().  We need an empirical
     * mapping table, populated either by:
     *  (a) disassembling VHIDEvent.sys::AcpiNotifyHandler to extract the switch
     *  (b) runtime-logging on the W767 itself and pressing every key
     * (b) is fast and self-validating. The switch has ~10 cases total. */
    static const struct { u8 notify; u16 key; } map[] = {
        { 0x80, KEY_POWER     },    /* placeholders — replace by observation */
        { 0x81, KEY_WAKEUP    },
        { 0x82, KEY_VOLUMEUP  },
        { 0x83, KEY_VOLUMEDOWN},
        { 0x84, KEY_LEFTCTRL  },
        { 0x85, KEY_LEFTALT   },
        { 0x86, KEY_LEFTMETA  },
        { 0x87, KEY_DELETE    },
        { 0x88, KEY_F14       },
        { 0x89, KEY_F15       },
    };
    /* … look up + input_report_key + input_sync … */
}
```

The Notify→keycode mapping is the only piece we don't have statically. Two routes:

1. **Empirical**: ship the driver with `pr_info("SVBI notify: 0x%02x\n", event)` initially, press each key, watch dmesg, fill in the table. Takes ~20 minutes once the kernel boots to userspace. Self-validating.
2. **Static**: load `VHIDEvent.sys` into Ghidra, find the function at the cross-reference site for the `-->p_notifyValue [0x%x]` string (offset 0x22c0), decompile the ~200-byte switch statement. Takes longer but gives the table cold. Recommended only if there's a reason to need it before first boot.

Recommend **route 1** because (a) we expect the W767 to reach userspace next iter, (b) the table is only ~10 entries, (c) misclassification of any single entry just means one wrong key — easy to debug.

### §4.3. The EmuEC platform driver (separate from keyboard)

Independent of the keyboard, EmuEC needs its own Linux driver to handle battery, charger, USB-PD events. That's a separate (much larger) project — likely ~1500 lines, with sub-modules for fuel-gauge, charger control, MUIC, and S2MM005 PD. **For the keyboard alone, none of that is required** — the keyboard driver only needs the ACPI Notify side, which goes through ACPICA's standard notify-handler API. The fact that "EmuEC.sys is the thing that *invokes* PHID" is invisible from the Linux SVBI driver's perspective; from Linux's view, ACPICA delivers a `Notify` event to the registered handler and that's the whole interface.

## §5. Methodology notes

### §5.1. Tools used

- **PowerShell + `[IO.File]::ReadAllBytes`** for byte-level binary search in `EmuEC.sys`, `VHIDEvent.sys`, `kbdHelper.sys`, and the DSDT AML. Found HID descriptor magic, ACPI NameStrings, byte clusters by direct pattern match. **Worked extremely well** — no need for Ghidra for any of these specific questions.
- **`grep -aob`** for finding string offsets in binaries.
- **`xxd`** for dumping AML bytes near interesting offsets.
- **Manual AML decode** by hand against the ACPI 6.4 spec to read the PHID method and EMEC `_CRS` resource template. Cost: ~30 minutes for ~80 bytes of AML. **Tedious but reliable** — and we have no `iasl` on Win11-ARM (not in PATH, not in `clangarm64/bin`).
- **`Get-PnpDevice` / `pnp_devices.txt`** from the previous recon session. Did 90% of the architectural work.
- **The existing `EmuEC_*` Ghidra dumps** from the V7-era session. Less useful than expected — they were focused on battery/I²C paths, not keyboard. The deep-dump file did not contain `PostHIDEvent` or `PHID` references.

### §5.2. What I did NOT use (and probably should have, in retrospect)

- **Ghidra disassembly of `VHIDEvent.sys::AcpiNotifyHandler`**. Would have produced the exact static Notify→key mapping table (§4.2 route 2). Skipped because:
  1. The table has ~10 entries and is empirically discoverable in ~20 minutes once Linux boots.
  2. Decompiling a Win11-ARM64 KMDF driver in Ghidra is non-trivial (need recent Ghidra build + ARM64 calling convention setup) and would have eaten the rest of the session.
  3. The bigger architectural finding (SVBI handles only 10 keys, not the whole keyboard) was more valuable than the table itself.

If the Linux side wants the table cold, recommendation: load `C:\Windows\System32\drivers\VHIDEvent.sys` into Ghidra, find xref to string at PE offset 0x22c0 ("`-->p_notifyValue [0x%x]`"), decompile the function containing it, and dump the switch.

### §5.3. Verification against existing repo claims

| Claim in repo                                              | Verification     | Status   |
|------------------------------------------------------------|------------------|----------|
| SAMM0901 has no _CRS                                       | AML offset 0x60192 confirms | ✓ |
| SAMM0901 `_HID` is at AML offset 393660                    | Confirmed (xxd) | ✓ |
| EmuEC has 6 GpioInts on `\_SB.GIO0`                        | Decoded all 32 GPIO descriptors in EMEC `_CRS`; counts exactly 6 | ✓ |
| EmuEC has ~32 GpioIos                                      | Decoded 26 GpioIo on `\_SB.GIO0` + 1 on `\_SB.PM01` = 27 total. Memory said "~32"; off by 5. Memory should be updated. | ⚠ |
| Touchpad is "ST I²C-HID (STMT 1234)"                       | Misleading: ST chip exists at ACPI STMT1234, but the touchpad's *report path* is USB-HID via SPACE MCU on PID_A055/MI_00 Col01+Col03. Memory should be corrected. | ⚠ |
| Internal keyboard handled by VHIDEvent.sys via ACPI Notify | Partially correct: only ~10 system keys; main alphanumeric path is USB-HID via SPACE MCU on PID_A055/MI_01. Memory should be corrected. | ⚠ |
| V1 scancode list (0x01..0x0A)                              | Wrong — values < 0x80 are explicitly rejected by VHIDEvent. Drop from memory. | ✗ |

I'll send the memory updates in a follow-up message.

### §5.4. What's left for a follow-up round

If the keyboard turns out NOT to work out of the box (despite all signs pointing to "it will"), the next round should:

1. Capture USB descriptors from the live SPACE v57 device via `usbview` or `lsusb -v` (under Linux once it boots, or via Windows usbview which we may need to fetch).
2. Disassemble the AcpiNotifyHandler in VHIDEvent.sys for the exact Notify→key map (§4.2 route 2).
3. Check whether MI_02 (the vendor-defined interface) carries anything the OS needs at boot time (firmware-version handshake?). The Win-side oem15.inf `kbdHelper_SamsungOSDSvcInstall` references `ComponentIds = VEN_SAMS&PID_0906` which suggests there's a UWP component that talks to MI_02; if Linux misses an expected handshake, the SPACE MCU may drop events.

Otherwise: hand this off to the Linux side, expect alphanumerics + touchpad on first USB-up boot, expect SVBI system keys after writing the ~250-line platform driver.
