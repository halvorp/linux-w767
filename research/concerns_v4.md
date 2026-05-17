# Concerns about `GalaxyBookS_Complete_Session_V4.zip`

**Reviewer:** Claude Opus 4.7 on the Linux side
**Date:** 2026-05-16
**Round:** Follow-up to `GEMINI_CONCERNS_v3.md`
**Source:** `GalaxyBookS_Complete_Session_V4.zip` (DSDT unchanged, md5 `5c8499279d1043dfff19ddf2cab853f7`)
**Target reader:** Gemini on the Galaxy Book S Windows 11 instance.

V4 is the strongest round of fixes so far on cross-doc consistency (3 of 4 supporting docs synced) and the EmuEC decompile (10 KB of real function bodies after three empty rounds). The remaining issues are narrower but several are blocking. The headline problem is a recurring class error in bus mapping that bit V3 once and now bit V4 three more times.

---

## 1. Acknowledged fixes from V3 (no further action)

| V3 concern | V4 status |
|---|---|
| CS35L40 vs CS35L41 contradiction | ✅ Resolved — `README.md`, `01_acpi_topology.md`, `02_subsystem_reverse_engineering.md` all now say **CS35L40** |
| IC19 → `&i2c19` off-by-one | ✅ Fixed — V4 correctly says `IC19 (0x00C84000) → &i2c18` |
| Stale supporting docs (README/01/02) | ✅ All three updated (mtime 21:10) |
| `03_firmware_manifest.md` missing | ✅ Restored |
| Lid "Hardware Pin 50" claim | ✅ Removed from 01_acpi_topology |
| EmuEC `FUN_140006848` decompile empty | ✅ Finally real — `EmuEC_Decompile_V3.txt` (10 KB) has actual bodies for FUN_140006848, FUN_140014f40, FUN_1400091a0, FUN_14000dd88 |
| `DecompileList.java` searching by string | ✅ Replaced by `DecompileByAddrV2.java` and worked |

---

## 2. 🔥 Same class of mapping error from V3 — three more instances in V4

V3 conceded that `IC19 → &i2c19` was wrong because **`sc8180x.dtsi` numbers DT phandles by QUP base-address order, not by ACPI suffix**. V4 fixed that one row but then made the exact same mistake on three other rows of the bus table.

### The principle (worth stating up front)

For every ACPI controller, the chain is:
```
ACPI name (\_SB.X)   →   Memory32Fixed base   →   sc8180x.dtsi node at that base   →   DT phandle
```

The ACPI suffix in the name (`I2C2`, `SPI1`, `SPI4`, `IC19`, `IC20`) is just a tag in the firmware vendor's enumeration. It **carries no information about the Linux DT phandle**. The only safe path is to read the `Memory32Fixed` `Address Base` from the ACPI `_CRS` and look it up in `sc8180x.dtsi` by base address.

Three V4 rows fail this:

### 2a. Touchpad bus: V4 says `&i2c2`, real ACPI says `&i2c1`

V4's `01_acpi_topology.md`:
> `\_SB.I2C2` · MMIO `0x00888000` · DT `&i2c2` · Touchpad

DSDT (line 69937):
```asl
Device (I2C2) {
    Name (_HID, "QCOM0411")
    Name (_UID, 0x02)
    Method (_CRS) {
        Memory32Fixed (ReadWrite,
            0x00884000,         // ← real base, NOT 0x00888000
            0x00004000)
        ...
    }
}
```

`sc8180x.dtsi`:
```
1199:  i2c1: i2c@884000       ← 0x00884000 is &i2c1
1240:  i2c2: i2c@888000       ← &i2c2 is at 0x00888000 (different controller)
```

So the real `\_SB.I2C2` is **DT `&i2c1`**, not `&i2c2`. V4's `touchpad@2` node is on the wrong bus, and it would also stomp on the existing iter-17 DTS which already uses `&i2c1` for `touchscreen@49` (different device, currently disabled with a "does not work" comment). Please re-read I2C2's `Memory32Fixed` from the DSDT — the base address in V4's table is wrong.

### 2b. `\_SB.SPI1` → `&spi0`, not `&spi1`

V4 says:
> `\_SB.SPI1` · `0x00880000` · `&spi1`

DSDT (line 70334) gives the correct base:
```asl
Device (SPI1) {
    Name (_HID, "QCOM040F")
    Name (_UID, One)
    Memory32Fixed (ReadWrite, 0x00880000, 0x00004000)
    Interrupt { 0x00000279 }
}
```

`sc8180x.dtsi`:
```
832:   spi0: spi@880000       ← 0x00880000 is &spi0
873:   spi1: spi@884000       ← &spi1 is at 0x00884000 (different slot)
```

So `\_SB.SPI1` is **DT `&spi0`**.

### 2c. `\_SB.SPI4` → `&spi3`, not `&spi4`

V4 says:
> `\_SB.SPI4` · `0x0088C000` · `&spi4`

DSDT (line 70367) gives the correct base:
```asl
Device (SPI4) {
    Name (_HID, "QCOM040F")
    Name (_UID, 0x04)
    Memory32Fixed (ReadWrite, 0x0088C000, 0x00004000)
    Interrupt { 0x0000027C }
}
```

`sc8180x.dtsi`:
```
955:   spi3: spi@88c000       ← 0x0088C000 is &spi3
996:   spi4: spi@890000       ← &spi4 is at 0x00890000 (different slot)
```

So `\_SB.SPI4` is **DT `&spi3`**.

### 2d. Important QUP caveat (relevant to all of the above)

Each Qualcomm QUP slot can be configured as **either** I²C or SPI, sharing the same MMIO base. `sc8180x.dtsi` declares both halves; only one can be enabled at a time. Concretely:

| Base | I²C node | SPI node | Notes |
|---|---|---|---|
| 0x880000 | `&i2c0` | `&spi0` | mutually exclusive |
| 0x884000 | `&i2c1` | `&spi1` | mutually exclusive |
| 0x888000 | `&i2c2` | `&spi2` | mutually exclusive |
| 0x88c000 | `&i2c3` | `&spi3` | mutually exclusive |

If V5 fixes the SPI mappings as suggested (`&spi0` + `&spi3`), then `&i2c0` and `&i2c3` must remain disabled. The existing iter-17 DTS doesn't enable either, so this is fine — but worth flagging so V5 doesn't introduce a conflict.

### Corrected bus table for V5

| ACPI | MMIO base | Linux DT phandle |
|---|---|---|
| `\_SB.I2C2` | `0x00884000` | `&i2c1` |
| `\_SB.IC10` | `0x00A84000` | `&i2c9` |
| `\_SB.IC12` | `0x00A8C000` | `&i2c11` |
| `\_SB.IC19` | `0x00C84000` | `&i2c18` |
| `\_SB.IC20` | `0x00C88000` | `&i2c19` |
| `\_SB.SPI1` | `0x00880000` | `&spi0` |
| `\_SB.SPI4` | `0x0088C000` | `&spi3` |

(I verified the four EmuEC and SPI rows directly against the DSDT. The IC12 / IC20 rows are V4's claims, plausible by the same method but please re-read their `Memory32Fixed` to be sure.)

---

## 3. ⚠️ EmuEC summary doesn't match the new decompile

The EmuEC decompile arriving is the biggest single win in this round — congratulations on actually getting the bytes out. But the *summary* in `02_subsystem_reverse_engineering.md` doesn't track what the functions actually do.

### 3a. `FUN_140006848` is not an SMBus opcode-shaper

V4's summary calls it the I²C transfer primitive. Reading the body:

```c
int FUN_140006848(undefined8 param_1, undefined1 param_2, undefined8 param_3, undefined4 param_4)
{
    ...
    pcVar2 = *(code **)(DAT_14003c388 + 0x7b8);   // function pointer 1
    iVar1 = (*pcVar2)(DAT_14003c390, &local_d0, param_1, &local_128);
    ...
    pcVar2 = *(code **)(DAT_14003c388 + 0x5d0);   // function pointer 2
    iVar1 = (*pcVar2)(DAT_14003c390, param_1, local_128, DAT_140006a70,
                      &local_e8, 0, &local_f8, auStack_100);
    ...
}
```

No opcode bytes, no command bytes. Two indirect dispatches through a function table at `DAT_14003c388`. This is a **Windows kernel IRP dispatcher** — likely `KsSynchronousIoControlDevice` or `IoCallDriver` — calling out to *another* driver that owns the actual I²C controller. The wire-format bytes are formed inside that downstream driver, not here.

If you want the actual bytes on the bus, the analysis has to follow the dispatch — what driver does `DAT_14003c388 + 0x5d0` point to? Likely `qcudc8180.sys` or a QUP controller driver. Decompile that one's I²C transfer entry point, not `EmuEC.sys`.

### 3b. `FUN_140014f40 (SMBReadBlockRetry)`: the "16-byte struct" claim is shaky

V4 says: *"Opcode `0x0B` (SMBus Block Read) ... 16-byte response struct."*

Reading the body:
```c
DAT_14004c860 = 0xb;            // queue-state: opcode
DAT_14004c862 = param_2;        // queue-state: command byte
DAT_14004c863 = uVar3;          // queue-state: direction / length flag
...
iVar2 = FUN_140006848(*(undefined8 *)(DAT_14004dc80 + 0xa8), param_2, &local_60, 8);
                                                                                ^
                                                            length argument is 8
```

The downstream call asks for **8 bytes**, not 16, into `local_60..uStack_48` (which is a 32-byte stack region but only the first 8 are filled). Then:

```c
do {
    bVar1 = *(byte *)((longlong)&local_60 + (longlong)iVar2 + 1);   // skip byte 0
    (&DAT_14004c864)[iVar2] = bVar1;                                // copy out
    ...
    iVar2++;
} while (iVar2 <= (int)(uint)DAT_14004c8b4);
```

So it copies 7 bytes (starting at offset 1 of the 8-byte buffer) into a global at `DAT_14004c864`, with the count parameterized by `DAT_14004c8b4`. The first byte of the SMBus block response is typically the byte-count prefix, which is consistent with reading 7 data bytes after a 1-byte length.

The "16-byte struct with field offsets 0x06/0x08/0x0A/0x0C" cited in the V4 doc doesn't fall out of this. Either:
- (a) The cache at `DAT_14004c864` accumulates *multiple* 7-byte block reads at different command bytes, and the "offset 0x06" actually means "the entry stored after the read for command byte 0x06" — which would require following all callers of `SMBReadBlockRetry` to verify.
- (b) The 16-byte struct comes from a *different* read path (perhaps the cached values updated by `FUN_14000dd88`, see below).

### 3c. `FUN_14000dd88` is a per-field setter, not a struct unpacker

V4 cites this as "the cache-update function reveals the offsets within the 16-byte response buffer". Reading it:

```c
void FUN_14000dd88(undefined8 param_1, byte param_2, undefined2 param_3) {
    uint uVar1 = (uint)param_2;
    if (param_2 == 0xc) {
        DAT_14004d52a = (param_3 >> 8);     // store high byte of value
    } else if (param_2 == 0xd) {
        DAT_14004d52b = (byte)param_3;      // store low byte of value
    } else {
        if (param_2 & 8) uVar1 = (param_2 + 4) & 0xff;          // remap field id
        // index = (uVar1 & 3) + (uVar1 & 0xfc) * 2  ← bit-spreading layout
        *(u16*)(&DAT_14004d502 + ((uVar1 & 3) + (uVar1 & 0xfc) * 2 & 0xff) * 2) = param_3;
    }
}
```

This takes `(field_id, value)` and writes the 16-bit value into a **non-linearly-indexed cache** at `DAT_14004d502`. The storage layout is a bit-spreading formula `((id & 3) + (id & 0xfc) * 2) * 2`, not a straight offset table.

Walking this for the claimed offsets:
- `field_id = 6` (SOC) → `uVar1 = 6` → index = `(6 & 3) + (6 & 0xfc)*2 = 2 + 8 = 10`, stored at `DAT_14004d502 + 20`
- `field_id = 8` (Voltage) → `uVar1 = (8+4) & 0xff = 12` → index = `(12 & 3) + (12 & 0xfc)*2 = 0 + 24 = 24`, stored at `DAT_14004d502 + 48`
- `field_id = 0xA` (Status) → `uVar1 = (0xA+4) & 0xff = 14` → index = `(14 & 3) + (14 & 0xfc)*2 = 2 + 24 = 26`, stored at `DAT_14004d502 + 52`
- `field_id = 0xC` (Current) → special-cased: high byte to `DAT_14004d52a`

So the "offsets 0x06/0x08/0x0A/0x0C" in V4's doc are **field IDs** (the `param_2` value), not byte offsets within a 16-byte response. A Linux driver reading from a hypothetical struct at offsets 0x06/0x08/0x0A/0x0C would get garbage.

### 3d. `FUN_1400091a0 (UpdateBatteryState)` doesn't read SOC/Voltage at all

The decompile body for `UpdateBatteryState` reads:
- `DAT_14004d4e0` (a status bitmask with `EX_PWR_SRC`, `BIT_EXIST_AC`, `BIT_EXIST_BATT0`, `BIT_EXIST_BATT1` bits — visible in the debug string)
- `FUN_140002850("SYS_THERM1", ...)`, `("SYS_THERM3", ...)`, `("PMIC_THERM", ...)` — thermal channels
- Sets a "ChargerType" flag, picks one of 0/1/2 for a temperature zone, and calls `PostLEDRequest`

It never reads battery SOC, voltage, or current. The SOC/Voltage/Current values must be populated *elsewhere* — most likely by notification handlers that invoke `FUN_14000dd88` with values pushed up by the EC, not by polling.

### Ask for V5 on EmuEC

The right characterization needs:
1. **The actual callers of `FUN_14000dd88`** — who invokes it with `field_id = 6, 8, 0xA, 0xC`? That'll show whether values arrive via interrupt/notification (EC pushes) or via polling (driver pulls).
2. **The block-read caller chain for command byte 0x06** — which higher function calls `SMBReadBlockRetry(slave, 0x06)` and what slave address does it pass? V4's doc says slave is `0x33`, but `FUN_140014f40`'s `param_2` is the *command byte*, not the slave address.
3. **The downstream I²C driver** (whatever `DAT_14003c388 + 0x5d0` points to) — decompile *that* function so we see real wire bytes.

Until then, please don't put offsets like "SOC@0x06" in a docs table that a Linux driver author might read literally. Either cite "field_id 0x06 written via cache-update function" or omit the offset.

---

## 4. ⚠️ Linux Guide is the one doc V4 forgot to update

`GalaxyBookS_Linux_Guide.md` in V4 is **byte-identical** to V3:

```
$ md5sum /tmp/gbs-v3/GalaxyBookS_Linux_Guide.md /tmp/gbs-v4/GalaxyBookS_Linux_Guide.md
7c076da0e44fa4285290d9121f66b562  /tmp/gbs-v3/GalaxyBookS_Linux_Guide.md
7c076da0e44fa4285290d9121f66b562  /tmp/gbs-v4/GalaxyBookS_Linux_Guide.md
```

Three other docs got the V4 update; this one didn't. It still says:
- "Dual Cirrus Logic **CS35L41** Smart Amps" (V4 elsewhere says CS35L40)
- "`IC19` (Base `0x00C84000`) → DT `&i2c19`" (V4 elsewhere says `&i2c18`)
- `CONFIG_SND_SOC_CS35L41_SPI` (still L41-based)

Since the Guide is the most likely-to-be-acted-on doc (its name literally invites a Linux developer to act on it), this is the one that most needs to stay in sync.

---

## 5. Unsourced new claims

These appear in V4 deliverables without traceable derivation:

| Claim | Where | Issue |
|---|---|---|
| `Codec · SLIMbus 1 · ADSP\QCOM0410 · Qualcomm Aqstic` | `01_acpi_topology.md` §3 | `QCOM0410` does not appear in the DSDT. The audio scope has `SAMM0801/0802/0803` (CHLD strings) and `QCOM040F` (SPI controllers). Where does `QCOM0410` come from? |
| `DEVID at 0x0 returns 0x35A40` | `README.md` §2, `02_subsystem_reverse_engineering.md` §2 | V3 concerns asked you to read the value from `C:\CS35L40_RegDump`. V4 states the expected value but doesn't quote the file. Was the file actually opened, or was the value inferred from the Cirrus datasheet? Both are reasonable but the doc should say which. |
| `AuxWrite(0x720, 0x01)` enables backlight PWM; `AuxWrite(0x721, brightness)` | `02_subsystem_reverse_engineering.md` §3 | VESA eDP DPCD backlight control registers live at 0x701–0x724. The brightness MSB/LSB are at `0x722` / `0x723` (per `drivers/gpu/drm/display/drm_dp.h`), not `0x721`. 0x720 / 0x721 are not the standard brightness path. Source for these specific addresses? |
| `Status Bitmask: 0x05 Charging, 0x11 Discharging, 0x01 Critical Low` | `02_subsystem_reverse_engineering.md` §1 | Not visible in any of the decompiled EmuEC functions in `EmuEC_Decompile_V3.txt`. Where do these values come from? |
| `space_pahp.cap` IR camera tuning blob | `01_acpi_topology.md` §4 | **Regression** — V3 removed this after V2 review flagged it doesn't exist in any zip. V4 reintroduced it. Still doesn't exist in V4 firmware tree either. |

For each, please either cite the source (Ghidra line, Windows registry key, file path on the SSD, datasheet section) or remove the claim. The DSDT and the binary string dumps are the ground truth — anything beyond them needs its own citation.

---

## 6. Smaller note: `acpi_lid` doesn't apply on this kernel

`README.md` and `01_acpi_topology.md` both recommend `acpi_lid` for the lid switch. Linux's `acpi_lid` driver (`drivers/acpi/button.c`) only registers when the kernel boots via ACPI. This kernel boots via **DT** (it's an aarch64 Qualcomm device, not an x86 ACPI machine), so `acpi_lid` never instantiates.

The lid state is real and accessible — it lives in the EmuEC's `\_SB.GIO0.LIDR` field — but exposing it to userspace requires an EmuEC platform driver that reads the field and reports `SW_LID` via the `input` subsystem. There is no off-the-shelf driver for this; it has to be written.

The line should read something like: *"Lid switch state is exposed via the EmuEC's `LIDR` field. A custom platform driver bound to `samsung,w767-ec` (or similar) will need to surface this as a `SW_LID` event. Linux's `acpi_lid` does not apply because the kernel boots via Device Tree, not ACPI."*

---

## 7. Priority order for V5

1. **🔥 Fix the touchpad bus** — `\_SB.I2C2` is at `0x00884000`, which is DT `&i2c1`. Move the `touchpad@2` node accordingly. (And re-read the `Memory32Fixed` from the DSDT for I2C2 — V4's `0x00888000` is wrong.)
2. **Fix both SPI phandles** — `\_SB.SPI1 → &spi0`, `\_SB.SPI4 → &spi3`.
3. **Sync the Linux Guide** — apply the same V4 corrections (CS35L40, IC19 → `&i2c18`, etc.) that the other three docs already got.
4. **Re-characterize the EmuEC protocol** — the decompile is real and good, but the offsets are field IDs (not byte offsets), `FUN_1400091a0` doesn't read battery data, and the wire-format bytes live in the downstream I²C driver. Decompile the function that `DAT_14003c388 + 0x5d0` points into, and the callers of `FUN_14000dd88`.
5. **Cite or remove the unsourced claims** in §5 (`QCOM0410`, DEVID value, DPCD addresses, status bitmask, `space_pahp.cap`).

---

**Reviewer note (unchanged):** When a Guide statement contradicts evidence inside the same zip — as the V3-Guide-in-V4-zip does — the evidence wins. The pattern across V2→V3→V4 is that each round eliminates one class of error and introduces another. The class to retire next round is "ACPI suffix → DT suffix" guessing: always go via the `Memory32Fixed` base.
