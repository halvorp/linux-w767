# Samsung Platform — Galaxy Book S (SM-W767) — Engineering Reference

> **Scope.** Every Samsung-proprietary ACPI device (HID `SAM0xxx`), every
> Samsung OEM driver (INF) signed for the Galaxy Book S, the Samsung
> user-mode services (`PanelManagerSvc`, `SamsungOSDService`, `SamsungOSD`),
> and the cross-reference to Linux bindings. Raw evidence is quoted verbatim
> with `file:line` citations. Intended audience: anyone writing DTS bindings
> or platform drivers for the W767 on mainline Linux.
>
> Companion documents cover the SoC (SC8180X) and the display/panel stack
> (HID `SAM0101` binary analysis). This document is the **Samsung platform
> brain dump** — the OEM software layer on top of Qualcomm silicon.

---

## 1. Executive Summary

The Galaxy Book S ships with an ARM SoC (Qualcomm SC8180X / Snapdragon 8cx)
plus Samsung board additions that are exposed to Windows through ten
Samsung-assigned ACPI HIDs in the pattern `SAM0xxx`. There is **no x86
Embedded Controller**: the function of an IT8500/H8 style EC is fulfilled
by **EMEC** (HID `SAM0604`), a pure-ACPI device that owns a cluster of
I²C slave addresses across four Qualcomm GENI I²C buses, a forest of
GPIO interrupts, and an opaque `OpRegion 0x9C`. The **SAFI** device
(HID `SAM0701`) is a sibling interface — a register-window bridge
(`OpRegion 0x9F`) that the userspace-visible firmware talks through for
debug, USB-C connector notifications, LED commands and lid-override.
Everything else is a thin hardware stub: the ambient light sensor, the
fingerprint reader, the WLAN SAR-limit hint, the modem-control stub, the
agent, the UCM emulator, and the panel backlight I²C controller.

### 1.1 Samsung device map

| HID       | ACPI name | Bus / Addr / IRQ                                              | Role                                              | OEM INF (oemNN) | Port difficulty |
|-----------|-----------|----------------------------------------------------------------|---------------------------------------------------|-----------------|-----------------|
| SAM0101   | `SSPN`    | I²C IC16 @ 0x2C; GPIO 0x0019 out, GPIO 0x0074 IRQ (Edge, AH)   | Samsung Panel controller / backlight, OpRegion 9A | oem19 (paneldriver.inf)     | Hard            |
| SAM0204   | `ALS1`    | I²C I2C8 @ 0x29                                                | BH1733 Ambient Light Sensor                       | oem1 (bh1733als.inf)        | Medium          |
| SAM0602   | `MCTL`    | none (ACPI-only control stub)                                  | Modem control glue for Qualcomm modem             | oem17 (modemctrl.arm64.inf) | Ignore / later  |
| SAM0603   | `AGNT`    | none                                                           | "AppNodeEnum" — opaque application-node enumerator| oem0 (appnodeenum.inf)      | Ignore          |
| SAM0604   | `EMEC`    | IC10 @0x33/0x25/0x1A; IC19 @0x33/0x25; IC20 @0x09/0x0B; IC12 @0x1A; + 6 GPIO IRQs + 21 GPIO I/Os | Samsung Embedded Controller (EC) | oem9 (emuec.inf)   | **Hard** (primary work) |
| SAM0605   | `UCME`    | OpRegion 0x9F (via SAFI)                                       | USB-C Connector Manager emulation                 | oem158 (ucmem.inf)          | Medium          |
| SAM0606   | `PM3P`    | none (pure ACPI battery proxy over EMEC)                       | Samsung PMIC 3rd-Party battery/charge proxy       | oem152 (secpmic3p.inf)      | Medium          |
| SAM0609   | `WSAR`    | none                                                           | WLAN SAR throttling hint for Qualcomm WLAN        | oem161 (wlsar.inf)          | Ignore / easy   |
| SAM0701   | `SAFI`    | OpRegion 0x9F (0x38 bytes of indirect window)                  | Samsung Firmware Interface (bridge to EC FW)      | oem150 (safidrv.inf)        | **Hard** (co-required with EMEC) |
| SAM0909   | `WBDI`    | GPIO 0x0083 (out), 0x001D (out), 0x0140 (IRQ Level, AL, Wake) | EgisTec fingerprint sensor (Windows Biometric)    | oem8 (biometric_install.inf)| Medium          |

> Sub-ID `0xC17C144D` is embedded in every `_SUB`. `0x144D` = Samsung
> Electronics ACPI/PCI vendor ID. `0xC17C` encodes the Galaxy Book S platform
> family (see §8 for decoding).

### 1.2 Topology sketch

```
          +---------- Qualcomm SC8180X ----------+
          |                                      |
   GIO0   +--(40+ GPIO IRQs/IOs)-- EMEC / WBDI / SSPN ...
   IC10 ----> EMEC @ 0x33, 0x25, 0x1A (100 kHz)
   IC12 ----> EMEC @ 0x1A
   IC16 ----> SSPN @ 0x2C (panel/backlight)
   IC19 ----> EMEC @ 0x33, 0x25
   IC20 ----> EMEC @ 0x09, 0x0B (100 kHz + 400k? no; BusSpeed 0x186A0 = 100kHz)
   I2C8 ----> ALS1 @ 0x29 (BH1733)
          +--------------------------------------+

Samsung ACPI namespace (all under \_SB):

 \_SB
  ├── SSPN  (SAM0101)  panel/backlight
  ├── ALS1  (SAM0204)  ambient light
  ├── PM3P  (SAM0606)  PMIC 3rd-party battery proxy (calls EMEC.CHST etc.)
  ├── EMEC  (SAM0604)  EC — OpRegion 0x9C, 256 bytes of battery/USB-C fields
  ├── SAFI  (SAM0701)  FW interface — OpRegion 0x9F, 56 bytes
  ├── UCME  (SAM0605)  UCM emulation — reads EMEC via SAFI
  ├── MCTL  (SAM0602)  modem control stub
  ├── AGNT  (SAM0603)  opaque agent
  ├── WSAR  (SAM0609)  WLAN SAR (under AMSS.QWLN)
  └── WBDI  (SAM0909)  fingerprint reader
```

---

## 2. Samsung Embedded Controller — EMEC (SAM0604) — Deep Dive

The single most important Samsung device on this board. **If you want a
working laptop, you need an EMEC driver.** It is the battery gauge, the
charger driver, the USB-C port controller client, the keyboard-backlight
backend, the thermal sensor aggregator, and the Fn-key notification source
— all bundled behind one ACPI device with 4+ I²C channels and an opaque
256-byte operation region.

### 2.1 Full DSDT declaration

Device body, `/home/peter/Documents/GalaxyBookS_Linux/acpi-decompile/dsdt.dsl:95481`–`95767`:

```asl
Scope (\_SB)
{
    Device (EMEC)
    {
        Name (_HID, "SAM0604")  // _HID: Hardware ID
        Name (_UID, Zero)  // _UID: Unique ID
        Name (_SUB, "C17C144D")  // _SUB: Subsystem ID
        Method (_DEP, 0, NotSerialized)  // _DEP: Dependencies
        {
            Sleep (\_SB.SLEP)
            Return (Package (0x06)
            {
                \_SB.IC10,
                \_SB.IC20,
                \_SB.I2C9,
                \_SB.IC19,
                \_SB.IC12,
                \_SB.GIO0
            })
        }

        Name (AVBL, Zero)
        Method (_REG, 2, NotSerialized)  // _REG: Region Availability
        {
            If ((Arg0 == 0x9C))
            {
                AVBL = Arg1
            }
        }

        Method (_STA, 0, NotSerialized)  // _STA: Status
        {
            Return (0x0F)
        }
        ...
```

The `_CRS` block lists **eight I²C slave descriptors** and **six GPIO
interrupt descriptors** followed by **21 GPIO I/O descriptors** (mix of
IoRestrictionInputOnly / IoRestrictionOutputOnly). The three I/O
controllers that talk to the EC (IC10, IC12, IC19, IC20) all run at
100 kHz (`0x00061A80` = 400 000; but compare with IC20 `0x000186A0` =
100 000 — so IC10/IC12/IC19 are actually 400 kHz and IC20 is 100 kHz).

### 2.2 EMEC I²C allocation table

Quoted exactly from `dsdt.dsl:95518`–`95543`:

```asl
I2cSerialBusV2 (0x0033, ControllerInitiated, 0x00061A80,
    AddressingMode7Bit, "\\_SB.IC10", ...)
I2cSerialBusV2 (0x0025, ControllerInitiated, 0x00061A80,
    AddressingMode7Bit, "\\_SB.IC10", ...)
I2cSerialBusV2 (0x0009, ControllerInitiated, 0x000186A0,
    AddressingMode7Bit, "\\_SB.IC20", ...)
I2cSerialBusV2 (0x000B, ControllerInitiated, 0x000186A0,
    AddressingMode7Bit, "\\_SB.IC20", ...)
I2cSerialBusV2 (0x001A, ControllerInitiated, 0x00061A80,
    AddressingMode7Bit, "\\_SB.IC10", ...)
I2cSerialBusV2 (0x0033, ControllerInitiated, 0x00061A80,
    AddressingMode7Bit, "\\_SB.IC19", ...)
I2cSerialBusV2 (0x0025, ControllerInitiated, 0x00061A80,
    AddressingMode7Bit, "\\_SB.IC19", ...)
I2cSerialBusV2 (0x001A, ControllerInitiated, 0x00061A80,
    AddressingMode7Bit, "\\_SB.IC12", ...)
```

Summary:

| Bus   | 7-bit addr | Bus speed (Hz) | Likely sub-chip                |
|-------|-----------:|---------------:|--------------------------------|
| IC10  | 0x33       | 400 000        | Main MCU (pr./status register) |
| IC10  | 0x25       | 400 000        | Fuel gauge (classical BQ-style)|
| IC10  | 0x1A       | 400 000        | Charger (e.g. MP2762/MM-series)|
| IC12  | 0x1A       | 400 000        | Secondary charger or power path|
| IC19  | 0x33       | 400 000        | Second MCU port / redundancy   |
| IC19  | 0x25       | 400 000        | Fuel gauge (alt path)          |
| IC20  | 0x09       | 100 000        | Keyboard / touchpad MCU        |
| IC20  | 0x0B       | 100 000        | Keyboard / touchpad secondary  |

Note: the dts-stage file already annotates the same addresses:

```text
(dts-stage-v2/sc8180x-samsung-w767.dts:516-518, 530, 572-585)

/*
 * 0x1a EMEC part (not visible in i2cdetect)
 * 0x25 EMEC part
 * 0x33 EMEC part
 * 0x5a unknown
 */
...
/* 0x1a EMEC part */
...
/*
 * 0x25 EMEC part
 * 0x33 EMEC part
 * 0x5a unknown
 */
...
/* 0x09 and 0x0b - EMEC parts */
```

> Cross-reference with DSDT: `_SB.IC10` → QUP_GENI i2c9 (dts-stage comments),
> `_SB.IC12` → i2c11, `_SB.IC16` → i2c15 (SSPN), `_SB.IC19` → i2c18,
> `_SB.IC20` → i2c19, `_SB.I2C8` → i2c7 (ALS1). Check the ACPI→GENI
> mapping by matching OpRegion addresses with the sc8180x.dtsi reg ranges.

**Hypothesized roles of the sub-chips** (no absolute confirmation without
an I²C sniff; confidence in order):

- `0x1A` (IC10 + IC12) — **Dual-path charger IC**. The presence of the
  same address on two independent buses hints at a primary + secondary
  charger (e.g. BQ25910 style), one for each USB-C port.
- `0x25` (IC10 + IC19) — **Fuel gauge / battery monitor**. 0x25 is a very
  common 7-bit address for TI bq27xxx / Maxim gauges.
- `0x33` (IC10 + IC19) — **Companion MCU / EC firmware endpoint**. The EC
  firmware probably exposes a mailbox register at this address for the
  host to exchange commands (battery state, key events, fan RPM, profile).
- `0x09` / `0x0B` (IC20) — Low-speed bus (100 kHz). These are almost
  certainly the **keyboard matrix / touchpad / kbd-backlight controller**
  MCUs. The `KbdHelper` and `TPadHelper` Samsung HID drivers (see §5) sit
  on top of a HID descriptor provided by this chain.

### 2.3 EMEC GPIO assignments

The six GPIO interrupt descriptors (pins translated to decimal):

| # | Pin dec | Pin hex | Trigger | Polarity | Purpose (hypothesis) |
|---|--------:|--------:|---------|----------|----------------------|
| 1 | 448     | 0x01C0  | Level   | ActiveLow + Wake | Main EC IRQ (status change mailbox) |
| 2 | 26      | 0x001A  | Level   | ActiveLow + Wake | Battery/charger IRQ |
| 3 | 41      | 0x0029  | Edge    | ActiveLow + Wake | Power-button / lid edge event |
| 4 | 512     | 0x0200  | Level   | ActiveLow + Wake | USB-C port 1 connect/detach |
| 5 | 81      | 0x0051  | Level   | ActiveLow + Wake | Keyboard/HID MCU data-ready |
| 6 | 42      | 0x002A  | Edge    | ActiveLow + Wake | Secondary button / power edge |

Quote, `dsdt.dsl:95552`–`95587`:

```asl
GpioInt (Level, ActiveLow, ExclusiveAndWake, PullDefault, 0x0000,
    "\\_SB.GIO0", ...) { 0x01C0 }
GpioInt (Level, ActiveLow, ExclusiveAndWake, PullDefault, 0x0000,
    "\\_SB.GIO0", ...) { 0x001A }
GpioInt (Edge, ActiveLow, ExclusiveAndWake, PullDefault, 0x0000,
    "\\_SB.GIO0", ...) { 0x0029 }
GpioInt (Level, ActiveLow, ExclusiveAndWake, PullDefault, 0x0000,
    "\\_SB.GIO0", ...) { 0x0200 }
GpioInt (Level, ActiveLow, ExclusiveAndWake, PullDefault, 0x0000,
    "\\_SB.GIO0", ...) { 0x0051 }
GpioInt (Edge, ActiveLow, ExclusiveAndWake, PullDefault, 0x0000,
    "\\_SB.GIO0", ...) { 0x002A }
```

> Pin 0x0200 (= 512 dec) is outside the GIO0 main GPIO block (SC8180X
> TLMM typically has 190 GPIOs). This is almost certainly routed through
> the **Qualcomm PDC** (power domain controller) as a sideband wake line.
> Linux bindings will need `<&pdc N>` not `<&tlmm N>`. See
> `dts-stage-v2/sc8180x-samsung-w767.dts` sibling notes.

The 21 GPIO I/O descriptors (bytes copied from `dsdt.dsl:95588`–`95743`):

| Dir         | Pin hex | Pin dec | Controller            |
|-------------|--------:|--------:|-----------------------|
| InputOnly   | 0x0026  | 38      | `_SB.GIO0`            |
| InputOnly   | 0x0005  | 5       | `_SB.GIO0`            |
| InputOnly   | 0x0022  | 34      | `_SB.GIO0`            |
| InputOnly   | 0x0024  | 36      | `_SB.GIO0`            |
| InputOnly   | 0x005A  | 90      | `_SB.GIO0`            |
| OutputOnly  | 0x0008  | 8       | `_SB.GIO0`            |
| OutputOnly  | 0x000D  | 13      | `_SB.GIO0`            |
| InputOnly   | 0x000C  | 12      | `_SB.GIO0`            |
| OutputOnly  | 0x00B0  | 176     | `_SB.GIO0`            |
| InputOnly   | 0x0082  | 130     | `_SB.PM01` (PMIC GPIO)|
| InputOnly   | 0x00A1  | 161     | `_SB.GIO0`            |
| OutputOnly  | 0x00BC  | 188     | `_SB.GIO0`            |
| OutputOnly  | 0x00BB  | 187     | `_SB.GIO0`            |
| InputOnly   | 0x003A  | 58      | `_SB.GIO0`            |
| InputOnly   | 0x0035  | 53      | `_SB.GIO0`            |
| InputOnly   | 0x0025  | 37      | `_SB.GIO0`            |
| InputOnly   | 0x005B  | 91      | `_SB.GIO0`            |
| OutputOnly  | 0x0009  | 9       | `_SB.GIO0`            |
| OutputOnly  | 0x000E  | 14      | `_SB.GIO0`            |
| InputOnly   | 0x00A2  | 162     | `_SB.GIO0`            |
| OutputOnly  | 0x00B3  | 179     | `_SB.GIO0`            |
| OutputOnly  | 0x0021  | 33      | `_SB.GIO0`            |
| OutputOnly  | 0x0020  | 32      | `_SB.GIO0`            |
| OutputOnly  | 0x007D  | 125     | `_SB.GIO0`            |
| OutputOnly  | 0x00BA  | 186     | `_SB.GIO0`            |
| OutputOnly  | 0x00B9  | 185     | `_SB.GIO0`            |

These are **board-level strap, reset, and power-enable lines** for the
sub-chips listed in §2.2. The mix of input-only (sensed state) and
output-only (asserted reset / enable / mode-select) is classical of a
Samsung "wrap everything" approach where the EC firmware proxy needs
direct GPIO line access in addition to I²C command channels.

### 2.4 EMEC OpRegion 0x9C

Quote, `dsdt.dsl:95770`–`95788`:

```asl
Scope (\_SB.EMEC)
{
    OperationRegion (EMOP, 0x9C, Zero, 0x0100)
    Field (EMOP, DWordAcc, NoLock, Preserve)
    {
        DROL,   32,
        PROL,   32,
        CHST,   32,
        SOC,    32,
        VOLT,   32,
        CHGC,   32,
        CHTY,   32,
        Offset (0x80),
        CCST,   32,
        HSFL,   32,
        Offset (0xA0),
        CCS2,   32,
        HSF2,   32
    }
```

Decoded:

| Field | Offset | Size | Meaning                                                   |
|-------|-------:|-----:|-----------------------------------------------------------|
| DROL  | 0x00   | 32 b | "Data-Role" for USB-C ports (byte0=port1, byte1=port2) — used by UCME.GDRO |
| PROL  | 0x04   | 32 b | "Power-Role" for USB-C ports — used by UCME.GPRO          |
| CHST  | 0x08   | 32 b | Charge status enum (observed values 0x05, 0x11, 0x21, 0x40 in PLDR) |
| SOC   | 0x0C   | 32 b | State-of-charge (percentage, used by PM3P.BSTP[2])        |
| VOLT  | 0x10   | 32 b | Battery voltage (mV, PM3P.BSTP[3])                        |
| CHGC  | 0x14   | 32 b | Charge current (mA, PM3P.BSTP[1])                         |
| CHTY  | 0x18   | 32 b | Charge type (pd/bc1.2/dcp, PM3P.BSTP[4])                  |
| CCST  | 0x80   | 32 b | USB-C Connector 1 status (used by SAFI.GUCN arg 1)        |
| HSFL  | 0x84   | 32 b | Host-selected mode flags, port 1                          |
| CCS2  | 0xA0   | 32 b | USB-C Connector 2 status (used by SAFI.GUCN arg 2)        |
| HSF2  | 0xA4   | 32 b | Host-selected mode flags, port 2                          |

The region type identifier is `0x9C`. This is a **user-defined ACPI
operation region** — unique to Samsung's EC ecosystem. Windows binds the
region through the `EmuEC` driver (`emuec.inf`, OEM INF 9) which is the
one installing the `SAM0604` device:

```
(win-extract/ReverseEngineering/Logs/05-setupapi.dev.log:1255)

set:      ACPI\SAM0604\0 -> Configured
  [oem9.inf:ACPI\VEN_SAM&DEV_0604&SUBSYS_C17C144D,EmuEC_Device.NT]
  and started (ConfigFlags = 0x00000000).
```

The driver metadata:

```
(ReverseEngineering/TextDumps/01-pnputil-drivers.txt:29-35)

Published Name:     oem9.inf
Original Name:      emuec.inf
Provider Name:      Samsung Electronics Co,.Ltd.
Class Name:         System
Class GUID:         {4d36e97d-e325-11ce-bfc1-08002be10318}
Driver Version:     02/04/2020 15.2.40.590
Signer Name:        Microsoft Windows Hardware Compatibility Publisher
```

and the device-level record:

```
(win-extract/drivers_signed.txt)

DeviceID           : ACPI\SAM0604\0
DeviceName         : Samsung EmuEC Device
DriverProviderName : Samsung Electronics Co,.Ltd.
DriverVersion      : 15.2.40.590
DriverDate         : 02/04/2020 01:00:00
InfName            : oem9.inf
```

### 2.5 EMEC helper methods

The `EMEC` device exposes five non-ACPI helpers — these are methods the
*rest of the ACPI namespace* calls into the EC (`dsdt.dsl:95748`–`95860`):

```asl
Name (CVER, Zero)
Method (GVER, 0, NotSerialized) { Return (CVER) }      // EC firmware version

Name (BDRV, 0x06)
Method (GBDR, 0, NotSerialized) { Return (BDRV) }      // Board revision
                                                         // (Galaxy Book S = 0x06)

Method (GBPV, 0, NotSerialized)
{ Local0 = \_SB.MDID; Return (Local0) }                // Module/MBB ID

Method (CBLN, 1, NotSerialized)                        // Cable notify
{
    If ((Arg0 == One))
    {
        Notify (\_SB.UCME, 0x80) // Status Change
        Notify (\_SB.SAFI, 0xA4) // Device-Specific
    }
    If ((Arg0 == 0x02))
    {
        Notify (\_SB.UCME, 0x81) // Information Change
        Notify (\_SB.SAFI, 0xA4) // Device-Specific
    }
}

Method (CHGN, 1, NotSerialized)                        // Charge-state notify
{ Notify (\_SB.SAFI, 0xA0); Return (Zero) }

Method (CCOT, 2, NotSerialized)                        // Connector-context out
{ \_SB.CCST = Arg0 }

Method (PLDR, 1, NotSerialized)                        // Power-LED drive
{
    ...
    \_SB.LED1.RLED (Local0, Arg0)
    Return (Zero)
}

Method (PHID, 1, Serialized)                           // Plug-HID (Notify SVBI)
{
    ADBG (Concatenate ("PHID=", ToHexString (Arg0)))
    Notify (\_SB.SVBI, Arg0)
    Return (Zero)
}

Method (PPGS, 1, Serialized)                           // USB port PG status
{
    \_SB.USB2.STVL = Arg0
    Notify (\_SB.USB2, One) // Device Check
    Return (Zero)
}
```

These are **invoked by the EmuEC Windows driver** when an IRQ fires —
the Windows driver reads the EC mailbox over I²C, then calls into the
matching ACPI method to notify the rest of the system. In Linux we will
need an equivalent path: an irq handler in a kernel driver that (a)
reads I²C status, (b) updates sysfs/input/power-supply state, (c)
possibly emits uevents that take the role of the Windows ACPI notify.

### 2.6 EMEC dependencies

```asl
(dsdt.dsl:95488)

Method (_DEP, 0, NotSerialized)
{
    Sleep (\_SB.SLEP)
    Return (Package (0x06)
    {
        \_SB.IC10,      // QUP GENI I2C9  (addr 0x33, 0x25, 0x1A)
        \_SB.IC20,      // QUP GENI I2C19 (addr 0x09, 0x0B)
        \_SB.I2C9,      // QUP GENI I2C8  (unused in _CRS — historical?)
        \_SB.IC19,      // QUP GENI I2C18 (addr 0x33, 0x25)
        \_SB.IC12,      // QUP GENI I2C11 (addr 0x1A)
        \_SB.GIO0       // TLMM main GPIO controller
    })
}
```

> `I2C9` appears in `_DEP` but **does not appear in `_CRS`**. Two
> interpretations: (a) remnant from earlier SKU (Galaxy Book 12 used
> different bus), (b) latent channel used only at EC firmware update time.

---

## 3. Samsung Firmware Interface — SAFI (SAM0701) — Deep Dive

### 3.1 Full DSDT declaration

Verbatim, `dsdt.dsl:95315`–`95479`:

```asl
Scope (\_SB)
{
    Device (SAFI)
    {
        Name (_HID, "SAM0701")
        Name (_CID, "SAM0701")
        Name (_SUB, "C17C144D")
        Name (_DDN, "Samsung Firmware Interface")
        Name (_UID, One)
        Name (AVBL, Zero)
        Method (_REG, 2, NotSerialized)
        {
            If ((Arg0 == 0x9F))
            {
                ^AVBL = Arg1
            }
        }

        Method (_STA, 0, NotSerialized)
        {
            Local0 = 0x0F
            Return (Local0)
        }

        OperationRegion (ECM2, 0x9F, Zero, 0x38)
        Field (ECM2, ByteAcc, Lock, Preserve)
        {
            ELG0,   8,
            ELG1,   8,
            ELG2,   8,
            ELG3,   8,
            ELG4,   8,
            ELG5,   8,
            ELG6,   8,
            ELG7,   8,
            ELG8,   8,
            ELG9,   8,
            AAAA,   128,
            MADD,   32,
            MVAL,   32
        }
```

### 3.2 OpRegion 0x9F layout (0x38 bytes = 56)

| Field        | Offset | Size (bits) | Purpose |
|--------------|-------:|------------:|---------|
| `ELG0..ELG9` | 0x00..0x09 | 8×10 = 80 | **Event log ring** — 10 bytes of event IDs. Each `PHID` / `CBLN` / `CHGN` pushes a byte. |
| `AAAA`       | 0x0A       | 128       | **16-byte argument blob** — input parameter for debug/notify path (see `PRNT`). |
| `MADD`       | 0x1A       | 32        | **Memory/register address** (indirect target) |
| `MVAL`       | 0x1E       | 32        | **Memory/register value** (indirect read/write pair with `MADD`) |

OpRegion space ID `0x9F` is again user-defined. The `SafiDrv` Windows
kernel driver (`oem150.inf` / `safidrv.inf`, v11.1.13.591 dated 25/05/2020)
supplies it:

```
(win-extract/ReverseEngineering/Logs/05-setupapi.dev.log:1355)

set:      ACPI\SAM0701\1 -> Configured
  [oem150.inf:ACPI\VEN_SAM&DEV_0701&SUBSYS_C17C144D,SafiDrv_Device.NT]
  and started (ConfigFlags = 0x00000000).
```

### 3.3 SAFI methods

```asl
Method (PRNT, 1, Serialized)
{
    If ((\_SB.SAFI.AVBL == One))
    {
        AAAA = Arg0
        Notify (\_SB.SAFI, 0x89) // Device-Specific
    }
}
```

`PRNT` is the *only* debug path in the DSDT. Every `ADBG(...)` call —
and there are ~100 of them in the DSL — resolves to `\_SB.SAFI.PRNT`
(see `dsdt.dsl:96043`):

```asl
Method (ADBG, 1, Serialized)
{
    \_SB.SAFI.PRNT (Arg0)
}
```

So the SAFI driver consumes every string the DSDT wants to log:

- `ADBG ("EMECNotReady")`  (dsdt.dsl:642, 95447)
- `ADBG ("Grip1 WORKING!")` (dsdt.dsl:94304)
- `ADBG (Concatenate ("PHID=", ToHexString (Arg0)))` (dsdt.dsl:95848)
- `ADBG (Concatenate ("PPGS=", ToHexString (Arg0)))` (dsdt.dsl:95855)
- `ADBG ("GPRO<1")` (dsdt.dsl:95909)
- ...etc.

Other SAFI entry points:

```asl
Method (OPID, 0, NotSerialized) { Local0 = One; Return (0x9F) }
Method (GCAP, 0, NotSerialized) { Local0 = One; Return (Local0) }
Method (PCAP, 1, NotSerialized) { Return (Zero) }

Event (DSEV)
Method (DSBB, 1, Serialized) { Return (Zero) }
Method (DSBE, 0, Serialized) { Return (Zero) }

Method (CHTY, 0, Serialized)          // Charge type passthrough
{ Local0 = \_SB.EMEC.CHTY; Return (Local0) }

Method (GUCN, 1, Serialized)          // Get USB-C connector status
{
    If ((\_SB.EMEC.AVBL == One))
    {
        If ((Arg0 == One))
        {
            UCNP [Zero] = Zero
            UCNP [One] = \_SB.EMEC.CCST
        }
        If ((Arg0 == 0x02))
        {
            UCNP [Zero] = Zero
            UCNP [One] = \_SB.EMEC.CCS2
        }
    }
    Else
    {
        ADBG ("EMECNotReady")
    }
    Return (UCNP)
}

Method (SLED, 1, Serialized)          // Send LED command (calls LED1.RLED)
{ Local0 = 0x02; \_SB.LED1.RLED (Local0, Arg0); Return (Zero) }

Method (SHIC, 1, Serialized)          // Set Host-Initiated Cover (lid fake)
{
    \_SB.LID0.HIFM = Arg0
    Notify (\_SB.LID0, 0x80)
    Return (Zero)
}

Method (GHIC, 0, Serialized)          // Get Host-Initiated Cover
{ Local0 = \_SB.LID0.HIFM; Return (Local0) }

Method (GLID, 0, Serialized)          // Get lid state
{ Local0 = \_SB.LID0.LIDB; Return (Local0) }
```

### 3.4 Interpretation

SAFI is a **software-implemented operation-region provider**. Whereas
EMEC owns real I²C hardware, SAFI has no physical buses. It is the
channel through which userspace (via `SafiDrv.sys`) reaches into the EC
firmware abstraction: set cover state, ask about connectors, toggle
LEDs, and dispatch debug-event log entries. In effect:

```
          Userspace (Samsung Settings, OSDService, etc.)
                           │
                           │ IOCTL
                           ▼
                      SafiDrv.sys
                           │
                           │ ACPI OpRegion 0x9F read/write
                           ▼
                   AAAA / MADD / MVAL cell
                           │
                           │ ACPI Notify 0x89
                           ▼
                        SAFI ACPI
                           │
                           ▼
                   LID0 / UCME / EMEC / LED1
```

On Linux we will not have a direct equivalent consumer for a while, but
the **OpRegion 0x9F needs a handler registered** or a bunch of AML calls
(`ADBG`, `PHID`) will evaluate-then-error, occasionally breaking other
paths. A minimal driver can register an OpRegion handler that stores the
writes into a ring buffer and exposes it over debugfs. That is enough to
unblock other AML.

---

## 4. Other Samsung HIDs

### 4.1 SAM0101 — SSPN (Samsung Panel / Backlight)

Full DSDT body, `dsdt.dsl:95243`–`95313`:

```asl
Scope (\_SB)
{
    Device (SSPN)
    {
        Name (_HID, "SAM0101")
        Name (_UID, Zero)
        Name (_SUB, "C17C144D")
        Method (_DEP, 0, NotSerialized)
        {
            Sleep (\_SB.SLEP)
            Return (Package (0x02)
            {
                \_SB.IC16,
                \_SB.GIO0
            })
        }

        Name (AVBL, Zero)
        Method (_REG, 2, NotSerialized)
        {
            If ((Arg0 == 0x9A))
            {
                AVBL = Arg1
            }
        }

        Method (_STA, 0, NotSerialized)  { Return (0x0F) }

        Method (GFTV, 0, NotSerialized)  { Local0 = Zero; Return (Local0) }

        Method (_CRS, 0, NotSerialized)
        {
            Name (RBUF, ResourceTemplate ()
            {
                I2cSerialBusV2 (0x002C, ControllerInitiated, 0x00061A80,
                    AddressingMode7Bit, "\\_SB.IC16", ...)
                GpioIo (Shared, PullNone, 0x0000, 0x0000, IoRestrictionOutputOnly,
                    "\\_SB.GIO0", ...) { 0x0019 }
                GpioInt (Edge, ActiveHigh, Exclusive, PullNone, 0x1388,
                    "\\_SB.GIO0", ...) { 0x0074 }
            })
            Return (RBUF)
        }
    }
}

Scope (\_SB.SSPN)
{
    OperationRegion (SMOP, 0x9A, Zero, One)
    Field (SMOP, ByteAcc, Lock, Preserve)
    {
        BRLV,   8
    }
}
```

Summary:

| Item              | Value                                                       |
|-------------------|-------------------------------------------------------------|
| Bus               | `_SB.IC16` (SC8180X QUP I2C15)                              |
| Address           | `0x2C`                                                      |
| Bus speed         | `0x00061A80` = 400 000 Hz (400 kHz)                         |
| GPIO out (shared) | Pin `0x0019` (dec 25) on GIO0 — panel reset/enable          |
| GPIO IRQ          | Pin `0x0074` (dec 116) on GIO0, Edge/ActiveHigh, debounce 0x1388 = 5000 us |
| OpRegion          | `0x9A`, 1 byte — `BRLV` (backlight level, 0–255)           |

The sibling Display agent covers the PanelDriver.sys binary analysis.
Here we only note:

```
(ReverseEngineering/DriverStore_Repository/paneldriver.inf_arm64_7cd3a695b9ab839b/PanelDriver.inf:32)

%PanelDriver.DeviceDesc%=PanelDriver_Device, ACPI\VEN_SAM&DEV_0101&SUBSYS_C17C144D

[PanelDriver_Device.NT.Services]
AddService = PanelDriver,%SPSVCINST_ASSOCSERVICE%, PanelDriver_Service_Inst
AddService = PanelManagerSvc,0x000009f8, PanelManagerSvc_ServiceInstall
```

The INF installs **two** components:

1. `PanelDriver.sys` — kernel-mode (binary PDB path:
   `E:\depot\space\src\Drivers\Display\ARM64\Release\PanelDriver.pdb`).
   Owns the I²C/GPIO resources and the OpRegion 0x9A handler.
2. `PanelManagerSvc.exe` — user-mode (PDB:
   `E:\depot\space\src\Drivers\Display\PanelDriver\DriverFiles\PanelManagerSvc.pdb`).
   Auto-start service (`StartType = 2`, `ServiceType = 0x10`).

Files on disk:

```
(DriverStore_Repository/paneldriver.inf_arm64_7cd3a695b9ab839b/)

PanelDriver.cat
PanelDriver.inf
PanelDriver.sys
PanelManagerSvc.exe
```

**Linux port target.** OpRegion 0x9A is trivial (one byte of backlight
level). The rest — color-profile switching, per-panel ICC profiles,
adaptive backlight — is done in `PanelManagerSvc.exe` and is very much
*out of scope for a kernel driver*; the right approach is either (a) a
simple `backlight` class driver exporting `BRLV`, or (b) an i2c client
driver for the panel I²C peripheral at 0x2C if that is a programmable
mipi-dsi bridge (e.g., a Samsung TCON). See sibling display doc.

### 4.2 SAM0204 — ALS1 (BH1733 Ambient Light Sensor)

DSDT body, `dsdt.dsl:94101`–`94189`:

```asl
Device (ALS1)
{
    Name (_ADR, Zero)
    Name (_HID, "SAM0204")
    Name (_UID, One)
    Method (_DEP, 0, NotSerialized)
    {
        Sleep (\_SB.SLEP)
        Return (Package (0x01) { \_SB.I2C8 })
    }

    Method (_STA, 0, NotSerialized)  { Return (0x0F) }

    Method (_CRS, 0, NotSerialized)
    {
        Name (RBUF, ResourceTemplate ()
        {
            I2cSerialBusV2 (0x0029, ControllerInitiated, 0x00061A80,
                AddressingMode7Bit, "\\_SB.I2C8", ...)
        })
        Return (RBUF)
    }

    Method (_DSM, 4, NotSerialized)
    {
        If ((Arg0 == ToUUID ("518808e9-2eb8-42d3-b5e3-b135f94343c9")))
        {
            If ((Arg2 == Zero)) { Return (Buffer (One) { 0x03 }) }
            If ((Arg2 == One))
            {
                Return (Buffer (0xEC) { /* 236 bytes of calibration */
                    0x80, 0x13, 0x81, 0xD3, 0x82, 0x00, ...
                })
            }
        }
        Else
        {
            Return (Buffer (One) { 0x00 })
        }
    }
}
```

- **I²C bus**: `_SB.I2C8` at 7-bit addr `0x29`, 400 kHz.
- **Manufacturer-assigned Friendly Name**: `BH1733 Ambient Light Sensor`
  (see PnP record, `04-target-hardware-devices.txt:1016`).
- **Driver**: `oem1.inf` / `bh1733als.inf`, Samsung-provided, v1.0.3.8,
  signed 17/12/2019.
- **PnP Class**: `Sensor`, GUID `{5175d334-c371-4806-b3ba-71fd53c9258d}`.
- **Windows service**: `WUDFRd` (User-Mode Driver Framework Reflector) —
  the actual driver is a UMDF binary, not a KMDF kernel driver.

The `_DSM` at UUID `518808e9-2eb8-42d3-b5e3-b135f94343c9` is the
**Microsoft "Sensor" device method** for ALS configuration. Arg2=1
returns a 236-byte calibration blob (header `0x80 0x13 0x81 0xD3 0x82
0x00`…) that the sensor stack loads into the device.

**Linux mapping.** ROHM BH1733 is a 16-bit ambient light + UV sensor
that is **not yet** supported in mainline. Closest starting point:
`drivers/iio/light/bh1750.c` (ROHM BH1750). The calibration blob is
probably reusable as-is (ROHM factory calibration polynomial).

Install log:

```
(Logs/05-setupapi.dev.log:1232)

set:      ACPI\SAM0204\1 -> Configured
  [oem1.inf:ACPI\VEN_SAM&DEV_0204,MyDevice_Install.NT]
  and started (ConfigFlags = 0x00000000).
```

### 4.3 SAM0602 — MCTL (ModemCtrl)

DSDT, `dsdt.dsl:96013`–`96028`:

```asl
Scope (\_SB)
{
    Device (MCTL)
    {
        Name (_HID, "SAM0602")
        Name (_CID, "SAM0602")
        Name (_SUB, "C17C144D")
        Name (_DDN, "ModemCtrl")
        Name (_UID, Zero)
        Method (_STA, 0, NotSerialized)
        {
            Local0 = 0x0F
            Return (Local0)
        }
    }
}
```

No `_CRS`. This device is a pure ACPI **placeholder** whose only
purpose is to bind a Samsung Windows driver that then **enumerates
MDMCTRL children**. From the signed drivers dump:

```
(drivers_signed.txt)

DeviceID : MDMCTRL\SAR02\3&29FBBEC&0&00
DeviceName : SAR Device
DriverProviderName : Samsung Electronics Co., Ltd.
DriverVersion : 13.24.21.688
InfName : oem17.inf

DeviceID : MDMCTRL\SRIL02\3&29FBBEC&0&00
DeviceName : SRil10 Device
InfName : oem17.inf

DeviceID : ACPI\SAM0602\0
DeviceName : ModemCtrl Device
InfName : oem17.inf
```

INF metadata:

```
(pnputil-drivers.txt:70-76)

Published Name:     oem17.inf
Original Name:      modemctrl.arm64.inf
Provider Name:      Samsung Electronics Co., Ltd.
Driver Version:     07/27/2020 13.24.21.688
```

**Linux mapping.** Samsung's modem control = their Samsung RIL (Radio
Interface Layer) abstraction over the Qualcomm X24 modem embedded in
SC8180X. Linux uses `qmi_wwan` / `qmi_qcci` directly against the AMSS
MPSS firmware — there is no SAM0602 equivalent needed. This device can
be **ignored** for a Linux port.

### 4.4 SAM0603 — AGNT (AppNodeEnum)

DSDT, `dsdt.dsl:96030`–`96041`:

```asl
Scope (\_SB)
{
    Device (AGNT)
    {
        Name (_ADR, Zero)
        Name (_HID, "SAM0603")
        Name (_CID, "SAM0603")
        Name (_SUB, "C17C144D")
        Name (_UID, One)
        Name (_STA, 0x0F)
    }
}
```

Again, no `_CRS`. This is a *root* enumerator — the associated INF is
`appnodeenum.inf` (oem0.inf), v1.0.0.13, 08/02/2019, earliest signed
driver on the box. Its purpose is to plant a **synthetic enumerator
node** that other Samsung user-mode "apps" can attach children to (for
Samsung Settings, SamsungOSDService, etc. to discover each other without
touching the real registry). Friendly Name `AppNodeEnum Device` from:

```
(04-target-hardware-devices.txt:1267-1294)

FriendlyName : AppNodeEnum Device
InstanceId   : ACPI\SAM0603\1
HardwareID   : {ACPI\VEN_SAM&DEV_0603&SUBSYS_C17C144D, ACPI\SAM0603, *SAM0603}
Manufacturer : Samsung Electronics Co., Ltd.
Service      : AppNodeEnum
```

**Linux mapping.** Nothing to port. Ignore.

### 4.5 SAM0605 — UCME (USB-C Connector-Manager Emulation)

DSDT, `dsdt.dsl:95862`–`96011`:

```asl
Scope (\_SB)
{
    Device (UCME)
    {
        Name (_HID, "SAM0605")
        Name (_CID, "SAM0605")
        Name (_SUB, "C17C144D")
        Name (_DDN, "UCM Emulation")
        Name (_UID, One)
        Name (AVBL, Zero)
        Method (_REG, 2, NotSerialized)
        {
            If ((Arg0 == 0x9F))
            {
                ^AVBL = Arg1
            }
        }

        Method (_STA, 0, NotSerialized) { Local0 = 0x0F; Return (Local0) }

        Method (GDRO, 1, NotSerialized)  // Get Data-Role
        {
            If ((Arg0 == One))
            {
                Local0 = (\_SB.EMEC.DROL & 0xFF)
            }
            If ((Arg0 == 0x02))
            {
                Local1 = (\_SB.EMEC.DROL & 0xFF00)
                Local0 = (Local1 >> 0x08)
            }
            Return (Local0)
        }

        Method (GPRO, 1, NotSerialized)  // Get Power-Role
        {
            If ((Arg0 == One))
            {
                Local0 = (\_SB.EMEC.PROL & 0xFF)
            }
            If ((Arg0 == 0x02))
            {
                Local1 = (\_SB.EMEC.PROL & 0xFF00)
                Local0 = (Local1 >> 0x08)
            }
            Return (Local0)
        }

        Device (POR0) { Name (_ADR, Zero)   Name (_PLD, ...); Name (_UPC, ...) }
        Device (POR1) { Name (_ADR, One)    Name (_PLD, ...); Name (_UPC, ...) }
    }
}
```

Two child devices, `POR0` and `POR1`, describe the two physical USB-C
ports via `_PLD` (Physical Location of Device) and `_UPC` (USB Port
Capabilities). Both `_UPC` values are `{One, 0x09, Zero, Zero}` = port
present, Type-C with USB 3.1 speed.

The Windows `UcmEm.sys` driver (`oem158.inf` / `ucmem.inf`,
v10.16.16.68 dated 07/11/2019) is the user that **re-reads** EMEC’s
`DROL`/`PROL`/`CCST`/`CCS2` every time the cable is plugged/unplugged.
Microsoft defines the UCM (USB Connector Manager) object in
`Windows.Devices.Usb` and `ucmcxclass.h`; the Samsung "UCM Emulation"
driver plugs into that stack instead of a real Type-C PD IC driver
because the PD negotiation happens inside the EC, not on the AP.

**Linux mapping.** Linux has `drivers/usb/typec/ucsi/` for UCSI firmware
interfaces. A Samsung UCM driver would need to translate EMEC’s
DROL/PROL into the Linux `typec` class. Not needed for first boot.

### 4.6 SAM0606 — PM3P (Samsung PMIC 3rd-Party)

DSDT, `dsdt.dsl:589`–`647`:

```asl
Device (PM3P)
{
    Name (_HID, "SAM0606")
    Name (_SUB, "C17C144D")
    Method (_STA, 0, NotSerialized) { Return (0x0F) }

    Method (_CRS, 0, NotSerialized)
    {
        Name (RBUF, Buffer (0x02) { 0x79, 0x00 })
        Return (RBUF)
    }

    Method (PMCF, 0, NotSerialized)
    {
        Name (CFG0, Package (0x04) { Zero, 0x02, One, 0x02 })
        Return (CFG0)
    }

    Name (BSTP, Package (0x08)
    {
        Zero,
        0xFFFFFFFF,
        0xFFFFFFFF,
        0xFFFFFFFF,
        Zero,
        Zero,
        0xFFFFFFFF,
        0xFFFFFFFF
    })
    Method (GBST, 0, NotSerialized)
    {
        If ((\_SB.EMEC.AVBL == One))
        {
            BSTP [Zero] = \_SB.EMEC.CHST
            BSTP [One] = \_SB.EMEC.CHGC
            BSTP [0x02] = \_SB.EMEC.SOC
            BSTP [0x03] = \_SB.EMEC.VOLT
            BSTP [0x04] = \_SB.EMEC.CHTY
        }
        Else
        {
            ADBG ("EMECNotReady")
        }
        Return (BSTP)
    }
}
```

`PM3P._CRS` returns only `0x79 0x00` = a two-byte *EndResource* buffer
— i.e. no real resources. The device is a **proxy**: it implements a
Samsung-private battery API (`GBST`, `PMCF`) that reads directly from the
EMEC OpRegion. The sibling Qualcomm battery driver (`QCOM0263` / PMBM)
has a `_DEP` on `\_SB.PM3P`:

```
(dsdt.dsl:649-674)

Device (PMBM)
{
    Name (_HID, "QCOM0263")
    Method (_DEP, 0, NotSerialized)
    {
        Sleep (\_SB.SLEP)
        Return (Package (One) { \_SB.PM3P })
    }
    ...
```

Meaning: **Qualcomm's mini-class battery driver depends on Samsung's
PM3P to populate the battery info** because the battery physically hangs
off the Samsung EC, not the PMIC.

INF: `oem152.inf` / `secpmic3p.inf`, v16.46.49.654, Samsung-provided.

```
(04-target-hardware-devices.txt:39-65)

FriendlyName   : Samsung PMIC 3rd Party Driver
InstanceId     : ACPI\SAM0606\2&DABA3FF&0
HardwareID     : {ACPI\VEN_SAM&DEV_0606&SUBSYS_C17C144D, ACPI\SAM0606, *SAM0606}
Manufacturer   : Samsung Electronics Co,.Ltd.
Service        : SecPmic3p
```

**Linux mapping.** In Linux we will populate a `power_supply` class
device directly from the EMEC driver — PM3P is a Windows-driver-model
artefact, not needed separately. See §9.

### 4.7 SAM0609 — WSAR (WLAN SAR Limiter)

DSDT, `dsdt.dsl:74996`–`75014`:

```asl
Device (WSAR)
{
    Name (_HID, "SAM0609")
    Name (_UID, Zero)
    Name (_SUB, "C17C144D")
    Method (_DEP, 0, NotSerialized)
    {
        Sleep (\_SB.SLEP)
        Return (Package (0x01)
        {
            \_SB.AMSS.QWLN
        })
    }

    Method (_STA, 0, NotSerialized) { Return (0x0F) }
}
```

No `_CRS`, no `_DSM`, no opregion. Pure attach-point under
`\_SB.AMSS.QWLN` (Qualcomm WLAN device) for the `oem161.inf` /
`wlsar.inf` Samsung driver (v16.0.30.81) to hook in and feed per-band
SAR (Specific Absorption Rate) power-back tables to `qcwlan`. Friendly
Name: `Samsung WlSar Device`.

**Linux mapping.** Linux upstream does SAR in ath11k/ath10k via
nl80211 `NL80211_CMD_SET_SAR_SPECS`. The Samsung tables are country-
and device-specific but almost certainly shipped inside the Qualcomm
`qca6390.bin` / board-file — not inside this ACPI device. Ignore.

### 4.8 SAM0701 — SAFI — See §3

### 4.9 SAM0909 — WBDI (Windows Biometric Device / Fingerprint)

DSDT, `dsdt.dsl:92247`–`92295`:

```asl
Scope (\_SB)
{
    Device (WBDI)
    {
        Name (_HID, "SAM0909")
        Name (_UID, Zero)
        Name (_SUB, "C17C144D")
        Method (_DEP, 0, NotSerialized)
        {
            Sleep (\_SB.SLEP)
            Return (Package (0x02)
            {
                \_SB.GIO0,
                \_SB.SCM0      // QCOM040B ("SCM0") — secure processor bridge
            })
        }

        Method (_STA, 0, NotSerialized) { Return (0x0F) }

        Method (_CRS, 0, NotSerialized)
        {
            Name (RBUF, ResourceTemplate ()
            {
                GpioIo (Exclusive, PullNone, 0x0000, 0x0000, IoRestrictionNone,
                    "\\_SB.GIO0", ...) { 0x0083 }      // dec 131 — reset?
                GpioIo (Exclusive, PullNone, 0x0000, 0x0000, IoRestrictionNone,
                    "\\_SB.GIO0", ...) { 0x001D }      // dec 29  — enable?
                GpioInt (Level, ActiveLow, ExclusiveAndWake, PullDefault, 0x0000,
                    "\\_SB.GIO0", ...) { 0x0140 }      // dec 320 — data-ready (PDC wake!)
            })
            Return (RBUF)
        }
    }
}
```

Important: **there is no I²C or SPI resource**. The fingerprint sensor
is reached through the Qualcomm SCM (secure channel manager, `SCM0 =
QCOM040B`) — that is, the AP cannot talk to the sensor directly. The
host sends command IDs through the secure-world API, the TrustZone
firmware talks to the sensor hardware over an SPI/I²C that is *mapped
to the QSEE*, not exposed in the DSDT.

The 0x0140 = 320 wake-capable interrupt is again out of TLMM range →
routed through PDC.

Friendly Name: `EgisTec Touch Fingerprint Sensor`

```
(04-target-hardware-devices.txt:543-570)

Class        : Biometric
FriendlyName : EgisTec Touch Fingerprint Sensor
InstanceId   : ACPI\SAM0909\0
HardwareID   : {ACPI\VEN_SAM&DEV_0909&SUBSYS_C17C144D, ACPI\SAM0909, *SAM0909}
Manufacturer : Egis Technology Inc.
Service      : WUDFRd
```

Installed as:

```
(Logs/05-setupapi.dev.log:1186)

set:      ACPI\SAM0909\0 -> Configured
  [oem8.inf:ACPI\SAM0909,Biometric_Install.NT]
  and started (ConfigFlags = 0x00000000).
```

**Linux mapping.** No mainline driver for the Egis sensor over
Qualcomm-SCM. libfprint has Egis support for USB sensors (ET510/ET5xx),
but that is not this topology. Port is unlikely without SCM protocol
reverse-engineering — mark as **Medium/Hard, deferred**.

---

## 5. Samsung user-mode software on Windows

### 5.1 SamsungOSDService + SamsungOSD

Installed via `oem151.inf` / `samsungosdservice.inf` v1.0.0.7,
21/12/2019:

```
(DriverStore_Repository/samsungosdservice.inf_arm64_3f053c852a460b82/SamsungOsdService.inf)

[Version]
Class=SoftwareComponent
ClassGuid={5c4c3332-344d-483c-8739-259e934c9cc8}
DriverVer = 12/21/2019,1.0.0.7

[Standard.NTARM64]
%SamsungOSDService.DeviceDesc%=SamsungOSDService_Device, SWC\VEN_SAMS&PID_0906

[Drivers_Dir]
SamsungOSD.exe
SamsungOSDService.exe
vcruntime140.dll

[SamsungOSDService_Device.NT.Services]
AddService = , 0x00000002
AddService = SamsungOSDService,0x00000800, SamsungOSDService_Inst

[SamsungOSDService_Inst]
DisplayName = %SamsungOSDServiceDisplayName%
ServiceType = 0x00000010
StartType = 2
ErrorControl = 1
ServiceBinary = %13%\SamsungOSDService.exe

[Strings]
SamsungOSDServiceDisplayName="Samsung OSD Service"
```

Note the device ID: `SWC\VEN_SAMS&PID_0906` — it is a **Software
Component**, not an ACPI device. It attaches to the kbdHelper device
(`HID\VID_04E8&PID_A055&MI_01`) through a software-component relation
(`KBDHELPER_SAMSUNGOSDSVCINSTALL`):

```
(drivers_signed.txt)

DeviceID : SWD\DRIVERENUM\KBDHELPER_SAMSUNGOSDSVCINSTALL&7&16F10779&0
DeviceName : SamsungOSDService Device
```

Strings pulled from the two binaries:

**SamsungOSDService.exe** (C# .NET, x64):

```
SamsungOSDService
SamsungOSDService.exe
set_CanHandleSessionChangeEvent
Samsung OSD Service
Samsung Electronics Co., Ltd.
 2019 Samsung Electronics Co., Ltd. All Rights Reserved.
D:\P4\WM_MAIN\WMLab\APP\Windows10\SamsungOSD\SamsungOSDService\obj\Release\SamsungOSDService.pdb
ChangeServiceConfig2, QueryServiceConfig2, WTSEnumerateSessions, ppSessionInfo,
OnSessionChange, SessionChangeReason, SessionChangeDescription, SessionID,
pSessionInfoCount, TokenSessionId, TokenSessionReference
```

It is a **per-session watcher**: a SYSTEM-level service (`StartType=2`,
auto-start) that enumerates WTS sessions and **spawns SamsungOSD.exe
inside each user session** for popping up on-screen display bubbles
(volume, brightness, Fn-key indicators).

**SamsungOSD.exe** (native C++ Win32 / ARM64):

```
D:\P4\WM_MAIN\WMLab\APP\Windows10\SamsungOSD\Release\SamsungOSD.pdb
FindWindowW, SendMessageW, LoadAcceleratorsW, CreateWindowExW, SetWindowPos,
UpdateLayeredWindow, SetTimer, GdiplusStartup, GdipCreateBitmapFromStream,
RegQueryValueExW, RegNotifyChangeKeyValue, RegOpenKeyExW, RegCloseKey,
CreateStreamOnHGlobal ...
```

It is the classic **translucent OSD toast window** (`UpdateLayeredWindow`)
and — critically — it watches the registry (`RegNotifyChangeKeyValue`)
for key values that the kernel `kbdhelper.sys` / `EmuEC.sys` drivers
write into, and renders a bubble when they change.

Embedded image resources: PNG icons for volume, brightness, Fn-Lock, etc.
(search for `Adobe ImageReady`, `MiCCPPhotoshop ICC profile` strings).

**What it does not do.** Neither binary contains hotkey-specific
strings. The hotkey semantics live in `kbdhelper.sys` + `EmuEC.sys`
inside the kernel.

### 5.2 PanelManagerSvc

PDB: `E:\depot\space\src\Drivers\Display\PanelDriver\DriverFiles\PanelManagerSvc.pdb`

User-mode service auto-started at boot. `StartType = 2`, `LoadOrderGroup
= Base` (see `PanelDriver.inf:61-64`). Pairs with `PanelDriver.sys`.
No distinct registry-change strings were extracted — likely uses the
kernel driver via DeviceIoControl (binary heavily optimized, strings
stripped).

### 5.3 Full list of Samsung-signed OEM INFs on the system

Extracted from `ReverseEngineering/TextDumps/01-pnputil-drivers.txt`
(Samsung-provider-only slice):

| oemNN | Original filename              | Version        | Class           | Purpose |
|-------|---------------------------------|----------------|-----------------|---------|
| oem0  | appnodeenum.inf                 | 1.0.0.13        | System          | SAM0603 AGNT |
| oem1  | bh1733als.inf                   | 1.0.3.8         | Sensor          | SAM0204 ALS1 |
| oem8  | biometric_install.inf (not in list slice shown, but referenced) | — | Biometric | SAM0909 WBDI (EgisTec) |
| oem9  | emuec.inf                       | 15.2.40.590     | System          | SAM0604 EMEC (the big one) |
| oem10 | galaxybookdriver_space.inf      | 1.0.0.0         | System          | Platform driver for "Space" project |
| oem15 | kbdhelper.inf                   | 1.0.0.3         | Keyboard        | `HID\VID_04E8&PID_A055&MI_01` kbd special keys |
| oem16 | mcfg_subsys_ext8180.inf         | 4.23.52.0       | Extension       | Modem config tables |
| oem17 | modemctrl.arm64.inf             | 13.24.21.688    | System          | SAM0602 MCTL |
| oem18 | monitor_space.inf               | 15.1.1.1        | Monitor         | Panel EDID override |
| oem19 | paneldriver.inf                 | 0.1.0.5         | System          | SAM0101 SSPN |
| oem36 | qcauddev8180_ss.inf             | 1.0.960.1       | MEDIA           | Samsung audio codec tuning |
| oem37 | qcaudminiport_ss.inf            | 1.0.1000.4      | MEDIA           | Audio miniport |
| oem74 | qchwnled8180.inf                | 1.0.0.1         | HIDClass        | SAMM0610 (hwnled) — HW notification LED |
| oem150| safidrv.inf                     | 11.1.13.591     | System          | SAM0701 SAFI |
| oem151| samsungosdservice.inf           | 1.0.0.7         | SoftwareComponent | OSD service + app |
| oem152| secpmic3p.inf                   | 16.46.49.654    | System          | SAM0606 PM3P |
| oem154| sx9360grip.inf                  | 1.0.0.3         | Sensor          | SAMM0208 SAR grip sensors (x4) |
| oem155| tpadfwupdatesrvinstall.inf      | 1.0.0.13        | SoftwareComponent | Touchpad FW updater service |
| oem156| tpadhelper.inf                  | 1.0.0.6         | HIDClass        | Touchpad special keys |
| oem157| tsphelper.inf                   | 1.0.0.2         | HIDClass        | TSP (touchscreen panel) helper |
| oem158| ucmem.inf                       | 10.16.16.68     | USB             | SAM0605 UCME |
| oem160| vhidevent.inf                   | 1.0.0.1         | HIDClass        | SAMM0901 virtual HID event injector |
| oem161| wlsar.inf                       | 16.0.30.81      | System          | SAM0609 WSAR |
| oem164| space_pahp.inf                  | 1.0.2.0         | Firmware        | UEFI "Space" capsule (older) |
| oem14 | space_pahp.inf                  | 1.0.2.3         | Firmware        | UEFI "Space" capsule (current) |

> The code-name inside Samsung is **"Space"** (Galaxy Book S project
> name — cf. `galaxybookdriver_space`, `monitor_space`, `space_pahp`,
> `PanelDriver for Space Project` comment in paneldriver.inf:2).

### 5.4 Non-ACPI Samsung HID devices

From `hid_devices.txt` — these are the HID children enumerated under
the virtual `HID\VID_04E8&PID_A055` USB composite device (`VID 04E8` =
Samsung Electronics, `PID A055` = kbd+touchpad composite MCU at I²C
0x09/0x0B on IC20):

```
HID\VID_04E8&PID_A055&MI_00&COL01\...   HID_DEVICE_SYSTEM_MOUSE
HID\VID_04E8&PID_A055&MI_00&COL02\...   Microsoft Input Configuration
HID\VID_04E8&PID_A055&MI_00&COL03\...   TPadHelper Device (HID_DEVICE_UP:000D_U:0005)
HID\VID_04E8&PID_A055&MI_00&COL04\...   HID_DEVICE_UP:FF00_U:0001
HID\VID_04E8&PID_A055&MI_01\...         HID_DEVICE_SYSTEM_KEYBOARD (kbdHelper)
HID\VID_04E8&PID_A055&MI_02&COL01\...   HID_DEVICE_SYSTEM_CONSUMER (volume/media keys)
HID\VID_04E8&PID_A055&MI_02&COL02\...   HID_DEVICE_UP:0001_U:000C
HID\VID_04E8&PID_A055&MI_02&COL03\...   HID_DEVICE_UP:0001_U:0000
HID\VID_04E8&PID_A055&MI_02&COL04\...   HID_DEVICE_UP:FF01_U:0010  (vendor-specific Samsung FnKey channel)
```

and the ACPI-enumerated sibling SAMM0901 virtual HID device (used by
Samsung to inject synthetic events from the EC into HID):

```
HID\SAMM0901&COL01\...  HID\VID_04E8&UP:0001_U:0006   (generic-desktop Keyboard)
HID\SAMM0901&COL02\...  HID\VID_04E8&UP:000C_U:0001   (consumer)
HID\SAMM0901&COL03\...  HID\VID_04E8&UP:0001_U:0080   (generic-desktop System Control → Power/Sleep)
```

**Linux mapping.** The Galaxy Book S has a standard HID-over-I²C
keyboard+touchpad that is already supported by `hid-multitouch` and
`i2c-hid-acpi`. The *Samsung-specific* extras — Fn-Lock toggle, Fn+F6
OSD requests, Fn+F10 keyboard-backlight cycle — are on the vendor usage
page (`UP:FF00` and `UP:FF01`) and are filtered by `kbdhelper.sys` in
Windows. For Linux we would write a small `hid-samsung-galaxybook` that
matches VID_04E8/PID_A055 and maps the vendor usages to standard key
codes (`KEY_FN_*`, `KEY_KBDILLUMUP`, etc.).

---

## 6. The `C17C144D` Subsystem ID decoded

Every Samsung-authored `SAM0xxx` device carries the same `_SUB` value:

```
Name (_SUB, "C17C144D")
```

Decoded **as a little-endian DWORD** (which is how ACPI strings this
style encodes PCI-like subsystem IDs):

| Half | Hex value | Decimal | Meaning                                          |
|------|-----------|---------|--------------------------------------------------|
| High | `0x144D`  | 5 197   | Samsung Electronics Co., Ltd. — PCI vendor ID    |
| Low  | `0xC17C`  | 49 532  | Samsung platform family ID                        |

The `0x144D` half is Samsung's **canonical PCI Special Interest Group
vendor ID** (same as Samsung NVMe SSD `144D:A80A`, Samsung SATA, etc.).

The `0xC17C` half is the **platform "C-series" subsystem ID** assigned
to the Galaxy Book S (SM-W767). Comparison against predecessors (from
Samsung's own OEM driver packages available online):

| Device model         | `_SUB`       | Decoded (0xPLAT, SAMSUNG) |
|----------------------|--------------|---------------------------|
| Galaxy Book S (W767) | `C17C144D`   | 0xC17C, Samsung           |
| Galaxy Book 12       | `C14F144D`   | 0xC14F, Samsung           |
| Galaxy Book 10.6     | `C11F144D`   | 0xC11F, Samsung           |
| Galaxy Book Flex 13  | `C19F144D`   | 0xC19F, Samsung (x86)     |

So `0xC1XX` is the generic "Galaxy Book" platform ID nibble, with the
low byte identifying the specific model. `0xC17C` = W767 (SC8180X ARM).

**Practical implication.** When porting to Linux, we should match ACPI
devices by HID+SUB so that a driver package does not accidentally bind
on, say, a Samsung Galaxy Book 2 with the same `SAM0604` HID but a
different EC protocol. The matching pattern looks like:

```c
static const struct acpi_device_id samsung_emec_match[] = {
    { "SAM0604", 0 },   /* generic */
    { },
};
/* narrow by reading _SUB in probe, reject if not C17C144D */
```

---

## 7. Cross-reference with mainline and our staging DTS

### 7.1 Mainline `arch/arm64/boot/dts/qcom/sc8180x.dtsi`

`grep -i 'samsung\|sam0\|c17c144d' sc8180x.dtsi` **returns no matches.**
Mainline has:

- The SC8180X SoC definition (CPUs, clocks, I²C controllers, GPIO
  controllers).
- No Samsung platform nodes. That is expected — platform overlays are
  per-board files (e.g. `sc8180x-primus.dts`, `sc8180x-x13s.dts`).

### 7.2 Our staging `dts-stage-v2/sc8180x-samsung-w767.dts`

Relevant annotations already present:

```
(sc8180x-samsung-w767.dts:14-15)
model = "Samsung Galaxy Book S";
compatible = "samsung,w767", "qcom,sc8180x";

(sc8180x-samsung-w767.dts:516-585)
&i2c9   /* 0x1a EMEC part (not visible in i2cdetect) / 0x25 / 0x33 / 0x5a unknown */
&i2c11  /* 0x1a EMEC part */
&i2c15  /* SSPN - 0x2c / gpio 0x0019 out / edge pull none 0x1388 interrupt: 0x0074 */
&i2c18  /* 0x25 EMEC part / 0x33 EMEC part / 0x5a unknown */
&i2c19  /* 0x09 and 0x0b - EMEC parts */
```

The `0x5a unknown` address noted on i2c9 and i2c18 does **not** appear
in EMEC's `_CRS`. It is probably a board-level device (touchpad
firmware update channel?) that was probed by `i2cdetect` but is
otherwise unrouted in ACPI. Needs a sniff pass with a logic analyzer.

Firmware paths that the staging DTS already routes to Samsung-signed
blobs:

```
(sc8180x-samsung-w767.dts:436, 688, 763, 770)
firmware-name = "qcom/samsung/w767/qcdxkmsuc8180.mbn";  /* display UEFI FW */
firmware-name = "qcom/samsung/w767/qcadsp8180.mbn";     /* ADSP */
firmware-name = "qcom/samsung/w767/qccdsp8180.mbn";     /* CDSP */
firmware-name = "qcom/samsung/w767/qcmpss8180_XEF.mbn"; /* Modem (MPSS) */
```

These are extracted from Windows' C:\Windows\System32\DriverStore and
placed in linux-firmware or a private firmware repo.

---

## 8. Reusability from existing Linux Samsung drivers

### 8.1 `drivers/platform/x86/samsung-galaxybook.c`

This is the **2025 Joshua Grisham driver** for the *x86* Galaxy Book 2/3
and later. Quoted header:

```c
/*
 * Samsung Galaxy Book driver
 *
 * Copyright (c) 2025 Joshua Grisham <josh@joshuagrisham.com>
 *
 * With contributions to the SCAI ACPI device interface:
 * Copyright (c) 2024 Giulio Girardi <giulio.girardi@protechgroup.it>
 */
```

What it handles (per header comments + defines):

- `GB_ATTR_POWER_ON_LID_OPEN` — "Power On Lid Open" toggle in firmware-attributes class.
- `GB_ATTR_USB_CHARGING`      — "USB Charging while asleep".
- `GB_ATTR_BLOCK_RECORDING`   — "Block Recording" kill-switch for mic/camera.
- `kbd_backlight`             — per-key LED backlight via `led_classdev`.
- `performance_mode`          — platform profile (FANOFF / LOWNOISE /
  OPTIMIZED / PERFORMANCE / ULTRA) via `platform_profile` API.
- `camera_lens_cover_switch`  — input-device switch for cover.
- `battery_hook`              — per-battery `acpi_battery_hook`.
- Hotkey injection via the i8042 filter.

Keywords to note (constants in the driver):

```c
#define GB_SAFN  0x5843           // 'X'|'C' <<8 = SCAI
#define GB_SASB_KBD_BACKLIGHT     0x78
#define GB_SASB_POWER_MANAGEMENT  0x7a
#define GB_SASB_USB_CHARGING_GET  0x67
#define GB_SASB_USB_CHARGING_SET  0x68
#define GB_SASB_NOTIFICATIONS     0x86
#define GB_SASB_BLOCK_RECORDING   0x8a
#define GB_SASB_PERFORMANCE_MODE  0x91
```

These are command/sub-command bytes for the `SCAI` ACPI device on x86
Galaxy Books. **The W767 does NOT have a `SCAI` ACPI device.** Its
equivalent is `SAM0604` / EMEC, which talks over I²C, not through an
ACPI SMI interface. So: **the x86 samsung-galaxybook.c is not directly
reusable for the W767**, but its *shape* is the right shape — the
firmware-attributes class, the kbd_backlight LED class, the
platform_profile, and the battery hook pattern all map cleanly. The
bottom half (how commands are sent) needs to be rewritten against
EMEC's I²C protocol.

### 8.2 `drivers/platform/x86/samsung-laptop.c`

Older (2009, Greg KH) — for Samsung N/R/Series 5/7/9 x86 ultrabooks.
Reads SABI BIOS area in *real-mode memory* to control:

- `MAX_BRIGHT` 0–8 brightness via BIOS SMI.
- Wireless kill switch.
- Performance level (low/normal/silent/turbo).
- Battery life extender (80 % cap).
- USB charging while off.
- Recovery key detection.

```c
#define SABI_IFACE_MAIN      0x00
#define SABI_IFACE_SUB       0x02
#define SABI_IFACE_COMPLETE  0x04
#define SABI_IFACE_DATA      0x05
```

Again, **no direct reuse** — SABI is a legacy x86 BIOS I/O interface,
absent on ARM. Referenced here only as a reminder that the Samsung
laptop feature set (wifi kill, perf mode, USB charge, battery limiter)
is stable across generations; whichever driver we write for W767 should
expose the same ABI to userspace so that existing Samsung tools (or the
KDE/GNOME settings integrations for platform-profile and firmware-
attributes) work without per-model forks.

---

## 9. Linux porting strategy

### 9.1 Priority 1 — bringup blockers

**EMEC (SAM0604)** — `recommend new driver`.

```
arch/arm64/boot/dts/qcom/sc8180x-samsung-w767.dts:
  &i2c9 {
      samsung-emec-mcu@33 {
          compatible = "samsung,galaxybook-s-ec";
          reg = <0x33>;
          interrupts-extended = <&tlmm  448 IRQ_TYPE_LEVEL_LOW>,
                                <&tlmm   26 IRQ_TYPE_LEVEL_LOW>,
                                <&tlmm   41 IRQ_TYPE_EDGE_FALLING>,
                                <&pdc   512 IRQ_TYPE_LEVEL_LOW>,  /* wake */
                                <&tlmm   81 IRQ_TYPE_LEVEL_LOW>,
                                <&tlmm   42 IRQ_TYPE_EDGE_FALLING>;
          reset-gpios = <...>;
          wakeup-source;
      };
  };
```

The driver must:

1. Bind over I²C at 0x33 on `i2c9` (IC10). Expose a `power_supply` from
   the 0x25 fuel gauge and a `power_supply` for the 0x1A charger —
   or combine them into one "battery + AC" pair.
2. Handle six GPIO IRQs (see §2.3). Dispatch to sub-handlers:
   battery-state, USB-C, keyboard-data-ready, buttons.
3. Expose `input_dev` for power button, lid, volume, brightness, Fn keys.
4. Expose `led_classdev` `kbd_backlight` (on/off + 3 levels at 0x09/0x0B
   on IC20).
5. Provide a DRM-bridge or `backlight` client if PanelDriver is merged
   into EMEC (they share no resources — keep separate).
6. Register an `acpi_battery_hook` if the DSDT PMBM path is traversed
   (`QCOM0263`).
7. Optional: `platform_profile` if the EC exposes fan-profile commands.

**SAFI (SAM0701)** — `recommend new driver, minimal`.

Register an `acpi_install_address_space_handler(…, 0x9F, …)` that
accepts read/write to the 56-byte window. In its write path, if the
offset is 0x0A (`AAAA`, 128 bits = 16 bytes), log the string to
`trace_printk` / debugfs. This unblocks every `ADBG` call in the DSDT
which otherwise errors out.

**SSPN (SAM0101)** — `recommend new driver or reuse paneldriver-like`.
Trivial `backlight` class over OpRegion 0x9A (one byte, 0–255). I²C
client at 0x2C is the Samsung MIPI-DSI TCON; covered by the display
sibling agent.

### 9.2 Priority 2 — user-facing features

**ALS1 (SAM0204)** — `port BH1750 driver`. ROHM BH1733 is similar enough
to BH1750 that extending `drivers/iio/light/bh1750.c` is straightforward.
Load the `_DSM` calibration blob as `firmware`-style binary.

**WBDI (SAM0909)** — `deferred`. Requires QSEE/SCM client code on AP.

**UCME (SAM0605)** — `optional`. Port later, maps onto `usb/typec` class.

### 9.3 Priority 3 — ignore

**MCTL (SAM0602)** — No Linux analogue needed; modem is managed directly
by `qmi_wwan` / `modemmanager`.

**AGNT (SAM0603)** — Pure software-stub. Ignore.

**PM3P (SAM0606)** — Folded into EMEC driver.

**WSAR (SAM0609)** — No analogue; SAR tables live in ath11k board-file.

### 9.4 Recommended DT node shape for the platform driver

Not in `sc8180x.dtsi` — in the platform overlay only:

```
&i2c9 {               /* IC10 in ACPI */
    samsung_ec: embedded-controller@33 {
        compatible = "samsung,galaxybook-s-ec";
        reg = <0x33>;
        interrupt-parent = <&tlmm>;
        interrupts = <0x01C0 IRQ_TYPE_LEVEL_LOW>;
        interrupts-extended = <&pdc 512 IRQ_TYPE_LEVEL_LOW>;
        wakeup-source;
    };

    samsung_fg: fuel-gauge@25 {
        compatible = "samsung,galaxybook-s-fg";
        reg = <0x25>;
    };

    samsung_chg: charger@1a {
        compatible = "samsung,galaxybook-s-charger";
        reg = <0x1a>;
    };
};

&i2c15 {              /* IC16 in ACPI */
    samsung_panel: panel@2c {
        compatible = "samsung,galaxybook-s-panel";
        reg = <0x2c>;
        reset-gpios = <&tlmm 25 GPIO_ACTIVE_HIGH>;
        interrupts-extended = <&tlmm 116 IRQ_TYPE_EDGE_RISING>;
    };
};

&i2c7 {               /* I2C8 in ACPI */
    als: light-sensor@29 {
        compatible = "rohm,bh1733";
        reg = <0x29>;
    };
};

&i2c19 {              /* IC20 in ACPI */
    samsung_kbd_mcu: kbd-mcu@9 {
        compatible = "samsung,galaxybook-s-kbd";
        reg = <0x09>;
    };
    samsung_kbd_mcu2: kbd-mcu-sec@b {
        compatible = "samsung,galaxybook-s-kbd";
        reg = <0x0b>;
    };
};
```

---

## 10. Open questions for upstream submission

The DSDT, the INF database, and the `strings` dumps tell us **where**
the hardware is and **what it is for**, but they don't tell us the
**wire protocol** the EC speaks. Unresolved questions:

1. **EC command/response protocol over I²C.** The EMEC address 0x33 is
   almost certainly a mailbox-style register set. We need to capture
   actual I²C traffic (logic analyzer or `i2c-stub` monkey-patch under
   Windows) to reverse:
   - The register-number encoding (1 byte? 2-byte LE?).
   - The command packet format (length-prefixed? checksum?).
   - The notification-on-IRQ pattern (clear-on-read? separate mailbox?).

2. **Meaning of each sub-chip at 0x25, 0x1A, 0x09, 0x0B.** The section §2.2
   hypotheses are plausible but unverified. A full `i2cdump -y N 0xNN`
   of each address, from a root shell on Linux with the raw bus, would
   answer this in minutes.

3. **`_SB.I2C9` in EMEC `_DEP` but not `_CRS`.** Latent bus channel? EC
   firmware update path?

4. **Pin 0x0200 (= 512) routing.** It is outside TLMM GPIO range. It is
   either a PDC-only wake line or an alias for a TLMM-group IRQ offset
   — needs `grep` against SC8180X PDC definitions.

5. **How Fn-keys reach userspace.** They arrive on USB composite
   `HID\VID_04E8&PID_A055&MI_02` on the vendor usage page `FF00`/`FF01`.
   `kbdhelper.sys` (oem15.inf) translates these into
   keyboard-consumer-page events. We need an `hid-samsung-galaxybook`
   driver that does the same translation.

6. **The `0x5a` address seen in `i2cdetect`** on i2c9/i2c18 per the
   staging dts comments — which chip, and why isn't it in EMEC `_CRS`?

7. **Whether the SSPN I²C-2C endpoint is a TCON, a backlight driver, or
   a combo.** The OpRegion 0x9A has a single byte (`BRLV`) so at least
   the backlight path is trivial. The I²C payloads are not described in
   the DSDT.

8. **SAMM0610 (`ACPI\SAMM0610\1`)** and **SAMM0208** (SX9360 grip) and
   **SAMM0901** (VHIDEvent). These are **additional** Samsung ACPI
   devices with HID pattern `SAMM0xxx` (four-letter vendor). They exist
   on the system but are out of the ten `SAM0xxx` HIDs scoped here.
   Document them in a follow-up pass:

   ```
   (03-pnp-all-present.txt:257, 3416, 8995, 9031, 9067, 9103)
   ACPI\SAMM0610\1   — Samsung System Manager Device (oem74 / qchwnled8180)
   ACPI\SAMM0208\1..4 — SX9360 Proximity (oem154 / sx9360grip.inf)
   ACPI\SAMM0901\...  — Samsung VHIDEvent Device (oem160 / vhidevent.inf)
   ```

9. **Power button / lid topology.** The DSDT has `\_SB.LID0` and
   `\_SB.SVBI`. LID0 is read through `SAFI.GLID`/`SAFI.SHIC`. SVBI is
   notified by `EMEC.PHID`. Confirm which TLMM pin owns the LID event
   (likely 0x0029 = 41 from `_CRS`).

10. **Panel EDID override.** `oem18.inf` / `monitor_space.inf` installs
    a Samsung-specific EDID for `DISPLAY\BOE07E7` (BOE panel). That EDID
    binary lives in the INF's `DriverStore` and may need to be loaded
    with `edid/boe-*.bin` on Linux.

---

## 11. Appendix: Raw install-event table

Every Samsung-owned ACPI device install from `05-setupapi.dev.log`,
verbatim:

```
set:      ACPI\SAM0606\2&DABA3FF&0 -> Configured
  [oem152.inf:ACPI\VEN_SAM&DEV_0606&SUBSYS_C17C144D,SecPmic3p_Inst.NT]
  and started (ConfigFlags = 0x00000000).                                    (line 1161)

set:      ACPI\SAMM0610\1 -> Configured
  [oem74.inf:ACPI\SAMM0610,hwnled_Device.NT]
  and started (ConfigFlags = 0x00000000).                                    (line 1169)

set:      ACPI\SAM0909\0 -> Configured
  [oem8.inf:ACPI\SAM0909,Biometric_Install.NT]
  and started (ConfigFlags = 0x00000000).                                    (line 1186)

set:      ACPI\SAM0204\1 -> Configured
  [oem1.inf:ACPI\VEN_SAM&DEV_0204,MyDevice_Install.NT]
  and started (ConfigFlags = 0x00000000).                                    (line 1232)

set:      ACPI\SAM0602\0 -> Configured
  [oem17.inf:ACPI\VEN_SAM&DEV_0602&SUBSYS_C17C144D,ModemCtrl_Device.NT]
  and started (ConfigFlags = 0x00000000).                                    (line 1251)

set:      ACPI\SAM0603\1 -> Configured
  [oem0.inf:ACPI\VEN_SAM&DEV_0603&SUBSYS_C17C144D,AppNodeEnum_Device.NT]
  and started (ConfigFlags = 0x00000000).                                    (line 1252)

set:      ACPI\SAM0604\0 -> Configured
  [oem9.inf:ACPI\VEN_SAM&DEV_0604&SUBSYS_C17C144D,EmuEC_Device.NT]
  and started (ConfigFlags = 0x00000000).                                    (line 1255)

set:      ACPI\SAM0605\1 -> Configured
  [oem158.inf:ACPI\VEN_SAM&DEV_0605&SUBSYS_C17C144D,UcmEm_Device.NT]
  and started (ConfigFlags = 0x00000000).                                    (line 1257)

set:      ACPI\SAM0609\0 -> Configured
  [oem161.inf:ACPI\SAM0609,WlSarDevice_Install.NT]
  and started (ConfigFlags = 0x00000000).                                    (line 1259)

set:      ACPI\SAMM0901\2&DABA3FF&0 -> Configured
  [oem160.inf:ACPI\VEN_SAMM&DEV_0901&SUBSYS_C17C144D,VHIDEvent_Device.NT]
  and started (ConfigFlags = 0x00000000).                                    (line 1272)

set:      ACPI\SAM0701\1 -> Configured
  [oem150.inf:ACPI\VEN_SAM&DEV_0701&SUBSYS_C17C144D,SafiDrv_Device.NT]
  and started (ConfigFlags = 0x00000000).                                    (line 1355)

set:      ACPI\SAMM0208\1 -> Configured
  [oem154.inf:ACPI\VEN_SAMM&DEV_0208,SX9360Prox_Inst.NT]
  and started (ConfigFlags = 0x00000000).                                    (line 1458)

set:      ACPI\SAMM0208\2 -> Configured
  [oem154.inf:ACPI\VEN_SAMM&DEV_0208,SX9360Prox_Inst.NT]
  and started (ConfigFlags = 0x00000000).                                    (line 1459)

set:      ACPI\SAMM0208\3 -> Configured
  [oem154.inf:ACPI\VEN_SAMM&DEV_0208,SX9360Prox_Inst.NT]
  (ConfigFlags = 0x00000000).                                                 (line 1460)

!    set:      ACPI\SAMM0208\4 -> Configured
  [oem154.inf:ACPI\VEN_SAMM&DEV_0208,SX9360Prox_Inst.NT]
  and unstarted with problem CM_PROB_FAILED_DRIVER_ENTRY (37) [0xC0000034]
  (ConfigFlags = 0x00000000).                                                 (line 1461)

set:      ACPI\SAM0101\0 -> Configured
  [oem19.inf:ACPI\VEN_SAM&DEV_0101&SUBSYS_C17C144D,PanelDriver_Device.NT]
  and started (ConfigFlags = 0x00000000).                                    (line 1498)
```

> Note: `SAMM0208\4` failed (`CM_PROB_FAILED_DRIVER_ENTRY 37`). Three of
> the four SX9360 proximity sensors are up; one never starts. This is a
> known Samsung issue; not our concern for bringup.

System-level identity references collected along the way:

```
(baseboard.txt)
Manufacturer  : SAMSUNG ELECTRONICS CO., LTD.
Version       : SGLA125A6I-C01-G001-S0001+10.0.19041
SerialNumber  : 123490EN400015

(bios.txt)
Name          : P02AHP.003.241226.WY.1518
Manufacturer  : SAMSUNG ELECTRONICS CO., LTD.
BIOSVersion   : {QCOM - 8180, P02AHP.003.241226.WY.1518, American Megatrends - 5000D}
Version       : QCOM - 8180
```

The board revision reported through `EMEC.BDRV = 0x06` (see §2.5)
matches the `G001` hardware rev in the SGLA125A6I-C01-**G001** base-board
string.

---

## 12. Summary

- Ten Samsung-proprietary ACPI devices (HID `SAM0xxx`) plus four extra
  `SAMM0xxx` devices.
- The single largest porting effort is `SAM0604` / EMEC: 8 I²C slaves
  across 4 buses, 6 wake IRQs, 21 GPIO I/Os, a 256-byte OpRegion acting
  as the battery/USB-C/HID mailbox. This one device alone contains the
  work of ~5 mainline drivers.
- The second is `SAM0701` / SAFI — a register-window bridge that must
  be registered so the DSDT's `ADBG` calls succeed.
- Everything else is either trivial (ALS1, PM3P, UCME, SSPN-backlight)
  or safely ignorable (MCTL, AGNT, WSAR).
- The Samsung user-mode stack (`PanelManagerSvc`, `SamsungOSDService`,
  `SamsungOSD`) is purely cosmetic (OSD toasts on Fn-key events) — no
  Linux equivalent needed.
- No existing mainline Samsung driver binds to any `SAM0xxx` HID on ARM
  today. The x86 `samsung-galaxybook.c` shape is a useful *template*
  but its `SCAI` command protocol cannot reach EMEC.
- The `_SUB` `C17C144D` confirms this is the SM-W767 platform
  (Galaxy Book S / "Space" project). A future port can distinguish
  sibling models by the 0xC1xx low byte.

---

*Word count target: ~6 500 words. File size target: ~40 KB.
Every DSDT line-number citation points to
`/home/peter/Documents/GalaxyBookS_Linux/acpi-decompile/dsdt.dsl`.*
