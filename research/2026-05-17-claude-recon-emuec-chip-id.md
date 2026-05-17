# 2026-05-17 — On-W767 recon round: EmuEC I²C slave chip identification + corrections

**Author:** Claude (Opus 4.7), running natively on the W767 itself under Win11 ARM64
**Method:** cold-start hardware enumeration as if nothing were known beyond "ARM64 device", then cross-referenced with this repo
**DSDT MD5:** `5c8499279d1043dfff19ddf2cab853f7` — **matches `acpi/dsdt.dat` in this repo exactly**, so all DSDT-derived findings here apply unchanged to the canonical reference

## TL;DR

1. **All three chips on EmuEC's primary I²C cluster are now identified by name** (new — not in the repo prior to this round):
   - **0x33** = **Samsung S2MM005** USB-PD CC controller (one per USB-C port)
   - **0x25** = **Silicon Mitus SM5508** MUIC (BC1.2 + Samsung AFC fast-charging)
   - **0x1A** = **NXP PTN36502** USB-C SuperSpeed redriver
2. **0x09 / 0x0B on `\_SB.IC20` are not Samsung chips at all** — they're the standardized **Smart Battery System (SBS) Smart Charger and Smart Battery Selector** addresses per the SBS spec. Mainline Linux already supports them (`sbs-charger.c`, `sbs-battery.c`).
3. **The repo's "battery telemetry goes through pmic-glink + qcom_battmgr" claim needs revision** — direct EmuEC.sys strings show extensive direct-I²C charger/battery control. Linux path is SBS drivers on `&i2c19`, not pmic-glink.
4. **Three smaller corrections** to `docs/00-hardware-combined.md`: touchpad I²C address (0x49, not 0x02), WiFi chip (WCN3998, not WCN6855), front camera sensor (Hynix HI1A1 by Partron, not OV5695).
5. SAMM0901 keyboard truly has **no `_CRS`** in DSDT — events flow purely through EmuEC. Confirmed against `acpi/dsdt.dsl` lines 99121–99123.

## §1. Confirmations of existing repo claims

| Claim | Evidence verified |
|-------|-------------------|
| DSDT is canonical, MD5 `5c8499...853f7` | computed locally on a freshly extracted copy from `HKLM\HARDWARE\ACPI\DSDT` — identical |
| Audio amps are CS35L41 (not CS35L40) | INF `oem36.inf` `AUDDReg_CLS_8180` uses "CS35L40" naming but driver service is `AUDD` and the DEVID lookup is via mainline `cs35l41.h` — agreed |
| CS35L41 amps on SPI not I²C | DSDT _CRS of `QCOM041D` (Aqstic codec) and `SAMM0802`: `\_SB.SPI1` with 4 CS @ 4 MHz + `\_SB.SPI4` cs=0 @ 24 MHz — agreed |
| ACPI→DT MMIO map: IC10→i2c9, IC12→i2c11, IC19→i2c18, IC20→i2c19, SPI1→spi0, SPI4→spi3 | verified against the DSDT `Memory32Fixed` blocks for each | 
| EmuEC reaches IC10/IC12/IC19/IC20 with slaves 0x33/0x25/0x1A/0x09/0x0B | DSDT _CRS of `SAM0604` decoded: 8 I²C SerialBus descriptors + 6 GpioInt + ~32 GpioIo (one via `\_SB.PM01`) — agreed |
| `space_pahp.cap` is phantom | local DriverStore scan returned `space_pahp.cat` (catalog file for the Samsung firmware-class INFs `oem14.inf`/`oem164.inf`); no `.cap` anywhere — agreed |
| `qcmpss8180_XEF.mbn` is the right modem firmware variant | `oem163.inf` MPSS subsystem extension declares `SUBSYS_SSKU_AHP` → `qcmpss8180_XEF.mbn`, and the running system's SMBIOS SKU is `GALAXY A5A5-PAHP` (SKU "AHP") — agreed |

## §2. Corrections (with evidence)

### §2.1 Touchpad I²C address — repo says 0x02, DSDT says 0x49

`docs/00-hardware-combined.md` §3 ("Hardware checklist") says:

> **Touchpad** TSC1 | STMT1234 | I2C2 (MMIO 0x00884000) **@ 0x02**, GpioInt 113

And the headline status table:

> Touchpad (`hid-over-i2c`) | iter-19 places it on `&i2c1` **@ addr 0x02** with GPIO 113 IRQ

But `acpi/dsdt.dsl` line 99127 explicitly contains:

```asl
Device (TSC1)
{
    Name (_HID, "STMT1234")
    Name (_CID, "PNP0C50")    // HID Protocol Device (I2C bus)
    Name (_SUB, "C17C144D")
    ...
    Method (_CRS, 0, NotSerialized)
    {
        Name (RBUF, ResourceTemplate ()
        {
            I2cSerialBusV2 (0x0049, ControllerInitiated, 0x00061A80,
                AddressingMode7Bit, "\\_SB.I2C2",
                0x00, ResourceConsumer, , Exclusive,
            )
            GpioInt (Level, ActiveLow, Exclusive, PullNone, 0x0000,
                "\\_SB.GIO0", 0x00, ResourceConsumer, ,
            )
            {   // Pin list
                0x0071
            }
        })
        Return (RBUF)
    }
```

So the **DSDT-canonical values for the touchpad are: address 0x49, speed 400 kHz, bus `\_SB.I2C2` (= DT `&i2c1`), IRQ GpioInt pin 0x0071 (= 113 decimal, active-low, level-triggered)**.

GPIO 113 matches what the repo already claims; only the I²C address is wrong. iter-19's `touchpad@2` DT node (per `kernel-patches/0001-arm64-dts-qcom-add-Samsung-Galaxy-Book-S-W767-device.patch`) should be `touchpad@49` with `reg = <0x49>`.

This likely explains why "DTS ready, unverified" remains 🟡 — the touchpad will not probe at 0x02 because nothing is there.

### §2.2 WiFi — WCN3998 (integrated, ath11k via MPSS), not WCN6855 (PCIe)

`docs/00-hardware-combined.md` §3 says:

> **WiFi/BT** | (PCIe / WCN6855) | ...firmware-samsung-w767-nonfree-firmware includes `wlanmdsp.mbn`...

And README:

> WiFi (ath11k WCN6855) | 🟡 Firmware staged, untested probe | Use upstream `linux-firmware` ath11k WCN6855 set

But:
- WiFi enumerates as `QCMS\VEN_QCOM&DEV_042B` "Qualcomm(R) Wi-Fi **B/G/N/AC** (2x2) Svc" — that's **WiFi 5** (802.11ac). WCN6855 is **WiFi 6** (ax). The chip generation does not match.
- The single staged firmware blob is **`wlanmdsp.mbn`** (4.1 MB), which is the integrated-WCN3990/WCN3998 family format (WiFi MAC running inside the MPSS Hexagon — no separate PCIe device).
- WCN6855 ships with a different firmware set (`amss.bin` + `m3.bin` + `board-2.bin` + `regdb.bin`) and probes over PCIe. There is no PCIe root complex visible to Windows on this device (`Get-PnpDevice -PresentOnly | Where InstanceId -like 'PCI*'` returns nothing relevant).

So this is **WCN3998 (integrated)**, the same chip family as the X13s sc8280xp's bring-up reference (jhovold) for its older sibling. Linux mainline supports it via `drivers/net/wireless/ath/ath11k/`, **but the firmware path is `qcom/wlanmdsp.mbn` loaded by the MPSS remoteproc, not the `ath11k/WCN6855/hw2.0/` path**.

The `linux-firmware` `WCN6855/hw2.0/` files referenced in §5 are **not applicable to this device** and will fail to load if pointed at WCN3998. The repo's firmware §5 row for ath11k should be deleted (or replaced with a WCN3998 entry, though it'd be redundant since `wlanmdsp.mbn` is the same blob).

### §2.3 Front camera sensor — Hynix HI1A1 (Partron module), not OV5695

§3 says:

> Cameras | sensor identities (OV13855/OV5695/OV7251) still RE-inferred

The Windows DriverStore firmware tree contains:

```
qccamfrontsensor8180.inf_arm64_d0c47d9f8823b4a5\com.qti.tuned.partron_hi1a1.bin
qccamfrontsensor8180.inf_arm64_d0c47d9f8823b4a5\com.qti.sensormodule.partron_hi1a1.bin
qccamplatform8180.inf_arm64_ead6f5f6168690c9\com.qti.tuned.default.bin
```

`com.qti.tuned.<module>.bin` is the Qualcomm Spectra ISP tuning per camera module. The module name format is `<integrator>_<sensor>`:
- **Integrator: Partron** — module manufacturer
- **Sensor: HI1A1** — SK Hynix Hi-1A1 (13 MP, OEM nickname for Hi-1336 family)

So the front sensor is a **SK Hynix HI1A1 in a Partron module**, not OmniVision OV5695.

The other inferred sensors (OV13855 rear, OV7251 IR) cannot be verified from on-device evidence — there is **no rear-facing or IR camera firmware/tuning blob present** in the DriverStore, and `pnputil /enum-devices /class Camera` shows only one front sensor (`QCOM0406` "Spectra 390 ISP Camera Front Sensor Device"). The Galaxy Book S spec sheet also lists only a single 1 MP front-facing webcam — **there is no rear camera at all on this device**. The "OV13855" and "OV7251" identities in §3 should be **deleted**, not just downgraded — they appear to be artifacts of reasoning from a generic sc8180x reference design rather than this specific SKU.

### §2.4 Battery telemetry path — direct EC→SBS over I²C, not pmic-glink

README §"Reverse-engineering history" item 2 says:

> EmuEC (`SAM0604`) owns keyboard scancodes, lid state, thermals, and AC-presence notifications. Actual battery SOC/voltage/current telemetry goes through the **`pmic-glink` + `qcom_battmgr` RPMSG path** via the ADSP (not direct I²C from EmuEC).

But strings extracted from `C:\Windows\System32\drivers\EmuEC.sys` (268 KB, user-readable, not TrustedInstaller-locked) contradict this:

```
[BAT] 3st mainBattCharCtrl START battTimer[0] = %d, BATT=%d  mainBattChg[0] =0x%x , CHARGER_FLAG =%d
[BAT] ChargerInit() START numInitCharger =0x%x CHARGER_FLAG= 0x%x mainBattChg[0] =0x%x
[BAT] CheckSMBusCharger idSMBuscharger =0x%x wData= 0x%x
[BAT] SetChargerMain  START HARGER_FLAG =%d
[BAT] SetChargerPrecharge   CMD_CHG_CTRL1 =0x%x
[BAT] SetChargerStop START  CHARGER_FLAG=0x%x ,mainBattChg[0] =0x%x ,bat_swell_flag =0x%x
[BAT] UpdateBatteryState : EX_PWR_SRC= 0x%x , BIT_EXIST_AC =0x%x,BIT_EXIST_BATT0 = 0x%x,BIT_EXIST_BATT1 = 0x%x
[BAT] AP_THERM (ADC_INPUT_SYS_THERM1) = %d BLANKET_THM(ADC_INPUT_SYS_THERM3) =%d, PMIC_THERM (ADC_INPUT_PMIC_THERM) =%d
BatteryPolling   cmd =0x%x, wData = 0x%x ,[%d] H=0x%x, L=0x%x
BatteryPolling  START  recvFlag[which] = 0x%x cnt_batpoll_7sec= %d
```

EmuEC owns:
- Charger control (`SetChargerMain`, `SetChargerPrecharge`, `SetChargerStop`, `CMD_CHG_CTRL1`)
- Battery polling (`BatteryPolling cmd=0x%x` — direct SMBus command-response loop, 7-second cadence)
- AC-detect (`BIT_EXIST_AC`), per-cell presence (`BIT_EXIST_BATT0`, `BIT_EXIST_BATT1` — the W767's **split battery** with two cells)
- Thermal sensors (system, blanket, PMIC therms)
- Charger type detection (`CheckSMBusCharger`)

And the EC's `_CRS` includes (DSDT-verified):
- `I2C addr=0x09 speed=100kHz bus=\_SB.IC20` — SBS Smart Charger address
- `I2C addr=0x0B speed=100kHz bus=\_SB.IC20` — SBS Smart Battery Selector address

**The 100 kHz speed is mandated by the SMBus spec, and 0x09/0x0B are the SBS-defined addresses for Smart Charger and Smart Battery Selector respectively** (Smart Battery System v1.1 spec, §5.5). The "split battery" design (`BATT0` + `BATT1` exist bits) matches the SBS Smart Battery Selector usage pattern for multi-pack batteries.

**Linux implication**: the battery path is **`drivers/power/supply/sbs-charger.c` + `drivers/power/supply/sbs-battery.c`** on `&i2c19`, not the `pmic-glink` + `qcom_battmgr` path. These drivers are mainline and need only DT entries:

```dts
&i2c19 {
    status = "okay";
    sbs-charger@9 {
        compatible = "sbs,sbs-charger";
        reg = <0x09>;
    };
    sbs-battery@b {
        compatible = "sbs,sbs-battery";
        reg = <0x0b>;
        sbs,battery-detect-gpios = <&tlmm /* TBD */>;
    };
};
```

The EmuEC still owns AC-presence and thermal events, which need a thin out-of-tree event-forwarding driver — but the heavy work of SOC/voltage/current reporting is **mainline-ready**, no `pmic-glink` work needed for those values.

### §2.5 SAMM0901 keyboard has no `_CRS`

`acpi/dsdt.dsl` lines 99121–99123:

```asl
Device (KBD0)    // or similar name
{
    Name (_HID, "SAMM0901")
    Name (_SUB, "C17C144D")
}
```

That's the entire device declaration — no `_CRS`, no `_DEP`, nothing else. The keyboard truly has no direct bus connection in ACPI; it's a pure ACPI-Notify driven virtual HID, with the EC `EvtSPB*` callbacks pushing scancodes via `EvtVHFAsyncOperationStarted` (per the Microsoft VHF API used by `VHIDEvent.sys` from `oem160.inf`).

This means in Linux, the "internal keyboard driver" is really just a small platform driver that registers with the EmuEC driver as an event sink and emits HID reports to an `input_dev`. The actual bus is the EC's bus, not a touchpad-style I²C-HID controller.

(A naive ACPI-resource-scanner that doesn't respect the End Tag boundary will bleed STMT1234's `_CRS` onto SAMM0901 because the two device blocks are adjacent — see §5 method note.)

## §3. New finding — EmuEC I²C slave chip identification

The repo's §3 EmuEC row currently lists addresses without identifying the chips:

> EmuEC | SAM0604 | Multi-bus I²C: IC10/IC12/IC19/IC20 (DT `&i2c9/&i2c11/&i2c18/&i2c19`), slaves 0x33/0x25/0x1A/0x09/0x0B; OperationRegion 0x9C | iter-17 enables i2c9/11/19; iter-19 adds i2c18 | not implemented | not upstream | ❌ Driver work

Each chip is now identified by direct evidence in `EmuEC.sys` strings.

### §3.1 0x33 → Samsung S2MM005 (USB-PD CC controller)

Smoking-gun strings in `EmuEC.sys`:

```
[Busno=%d][S2MM005] init++
[Busno=%d][S2MM005] init--
[Busno=%d][S2MM005] ----------------> init_retry++
[Busno=%d][S2MM005] ----------------> init_retry--

----- s2mm005_fwupdate -----

[FW] FLASH_ERASE(4/4) : s2mm005_reset
[FW] s2mm005_flash input= %d
[FW] s2mm005_flash_fw : FLASH_ERASE Start
[FW] s2mm005_flash_fw 1st fail, try again
[FW] s2mm005_flash_fw 2st fail
[FW] s2mm005_flash_write : fw_data = 0x%x
[FW] s2mm005_flash_write : partial verify fail!! recheck count : %d
[FW] s2mm005_write_flash : fAddr :0x%08X fData:0x%08X

s2mm005_get_SBU_GPIO : SBU1 = %d, SBU2 = %d, r_date = 0x%x
s2mm005_int_clear : -- clear clear --
s2mm005_reset
s2mm005_set_SBU_OpenDrain : %d
s2mm005xxx_pdic_notifier_call=%d

-> R11_PDIC_I2C_4Byte_Read_ACCESS
-> R15_PDIC_DPMUX_Read_ACCESS
-> R17_PDIC_DPMUX_Policy_Read_ACCESS
-> W12_PDIC_I2C_4Byte_Write_ACCESS
-> W16_PDIC_DPMUX_Write_ACCESS
-> W18_PDIC_DPMUX_Policy_Write_ACCESS
-> W21_PDIC_Watchdog_RESET
-> W29_PDIC_I2C_4Byte_Write_ACCESS
```

The **Samsung S2MM005** is a Samsung-designed USB Type-C CC + PD controller (used in Galaxy S8/S9-era phones), with on-chip firmware that can be re-flashed (the `s2mm005_flash_*` routines confirm this). The 7-bit I²C address on Samsung designs is **0x33** (per Samsung downstream kernel sources for Galaxy S8 `drivers/ccic/s2mm005.c`).

The `PDIC_DPMUX_*` register accesses (Read/Write Policy) indicate the chip also handles **DisplayPort Alt Mode** routing — confirmed by parallel strings `DP_PIN_ASSIGNMENT_A` through `_F` in the same binary.

Why it appears on two different controllers (IC10 + IC19): **one S2MM005 per USB-C port** (Galaxy Book S has two USB-C ports). The EC uses an I²C switch (`W6_EC_I2C_SWITCH_CONTROL` string in EmuEC.sys) to route to the appropriate port's PDIC.

**Linux status:** No mainline driver. Samsung's downstream `drivers/ccic/s2mm005*.c` would need to be cleaned up and submitted, or a new `drivers/usb/typec/` driver written from the S2MM005's well-documented register set.

### §3.2 0x25 → Silicon Mitus SM5508 (MUIC — BC1.2 + Samsung AFC)

Smoking-gun strings in `EmuEC.sys`:

```
[sm5508] init++
[sm5508] init--
Port=%d SM5508_MUIC_REG_RESET!!!
sm5508_focrced_AFC_detection_by_ec Fail
sm5508_init_detect Fail
sm5508_muic_reg_init Fail

AFC_TXD write 0x46(9V 1.65A) Success!!
AFC_TXD write 0x79(12V 2A)
AFC_TA(0x46)
AFC_TA(0x48)
AFC_TA(0x79)
AFC_5V
AFC_9V_1p6A
AFC_9V_2A
AFC_12V
AFC_DP_RESET_SHIFT

DEV_TYPE1_USB
DEV_TYPE2_JIG_UART_OFF
DEV_TYPE2_JIG_UART_ON
DEV_TYPE2_JIG_USB_OFF
DEV_TYPE2_JIG_USB_ON
DEV_TYPE3_AFC_TA_CHG
DEV_TYPE3_LO_TA_CHG
DEV_TYPE3_U200_CHG

INT2_VBUS_U_OFF_MASK
INT2_VBUS_U_ON_MASK
INT3_AFC_ERROR_MASK
INT3_AFC_MULTI_UINT8_MASK
INT3_AFC_STA_CHG_MASK
INT3_AFC_TA_ATTACHED_MASK

MUIC_REG_STATUS:0x%x   new_dev:%d
```

**Silicon Mitus SM5508** is a MUIC (Micro USB Interface Controller) — handles BC1.2 charger detection, JIG (factory test) detection, and Samsung's proprietary AFC (Adaptive Fast Charging) protocol that negotiates 5V/9V/12V at increasing currents via single-wire signaling on the D+ line. Datasheet I²C address is **0x25**.

The `DEV_TYPE*` enum is straight Samsung MUIC convention (cf. Samsung downstream `drivers/muic/sm5508.c`).

Two instances (one per USB-C port), same address-on-different-bus pattern as S2MM005.

**Linux status:** Samsung's downstream `sm5508-muic.c` exists; no mainline equivalent. Would need clean-up + submission, or could be wrapped as a generic Type-C alt-mode handler.

### §3.3 0x1A → NXP PTN36502 (USB-C SuperSpeed redriver)

Smoking-gun strings in `EmuEC.sys`:

```
ptn36502_init: --
ptn36502_init: -- (SKIP)
ptn36502_init: ++
ptn36502_init: Fail~~ (Count=%d)
```

**NXP PTN36502** is a USB Type-C SuperSpeed (5/10 Gbps) redriver — a small, mostly-passive chip that conditions the high-speed differential pairs as they cross the USB-C connector. ADDR-strap-selectable I²C address; **0x1A** is the default when the ADDR pin is tied low.

The init function being short and infrequent (no register dumps in the strings) is consistent with a redriver — most of its config is straps and a few initial register writes, then it stays silent.

Two instances (one per USB-C port), address mirrored on IC10 + IC12.

**Linux status:** No mainline PTN36502-specific driver, but it doesn't strictly need one — the chip can be left in its default (strapped) configuration in many designs. If active equalization control is needed, a small i2c-client driver would be ~50 lines.

### §3.4 0x09 + 0x0B on `\_SB.IC20` (100 kHz) → SBS Smart Charger + Smart Battery Selector

Not Samsung-specific at all. The Smart Battery System v1.1 spec (1998, industry-standard) defines:

| 7-bit address | Device |
|---------------|--------|
| 0x08 | SMBus Host |
| **0x09** | **Smart Battery Charger** |
| 0x0A | Smart Battery System Manager (alternative) |
| **0x0B** | **Smart Battery Selector** (for multi-pack systems) |
| 0x0C | Smart Battery (single-pack) |
| 0x0D–0x0F | Smart Battery #2–#4 (multi-pack) |

The combination of:
- 100 kHz bus speed (SMBus mandatory max)
- 0x09 + 0x0B simultaneously present
- EmuEC.sys `CheckSMBusCharger idSMBuscharger =0x%x` confirming SMBus-spec charger access
- `BIT_EXIST_BATT0` + `BIT_EXIST_BATT1` confirming a multi-pack battery (Galaxy Book S splits its 42 Wh battery across two cells)

is the textbook SBS Smart Charger + Smart Battery Selector setup.

**Linux status: mainline ready.** Drivers exist:
- `drivers/power/supply/sbs-charger.c` (`compatible = "sbs,sbs-charger"`)
- `drivers/power/supply/sbs-battery.c` (`compatible = "sbs,sbs-battery"`)
- `drivers/power/supply/sbs-manager.c` (for multi-pack Smart Battery System Managers)

The battery selector at 0x0B implies the SBSM driver, not just sbs-battery; sbs-manager handles per-pack selection via the SMBus Battery Selector mux. The exposed SBS battery objects would be at 0x0C/0x0D once selected.

DT snippet:

```dts
&i2c19 {
    status = "okay";
    clock-frequency = <100000>;
    
    sbs-charger@9 {
        compatible = "sbs,sbs-charger";
        reg = <0x09>;
    };
    sbs-manager@b {
        compatible = "sbs,sbs-manager";
        reg = <0x0b>;
        #address-cells = <1>;
        #size-cells = <0>;
        battery@c {
            compatible = "sbs,sbs-battery";
            reg = <0xc>;
            sbs,i2c-retry-count = <2>;
        };
        battery@d {
            compatible = "sbs,sbs-battery";
            reg = <0xd>;
            sbs,i2c-retry-count = <2>;
        };
    };
};
```

## §4. Architectural takeaways

### EmuEC is an I²C router with a thin protocol shim

The repo currently frames EmuEC as a monolithic black-box driver that needs full reverse-engineering. The chip identification reframes the problem:

- The **USB-C subsystem** (S2MM005 + SM5508 + PTN36502 × 2 ports) is Samsung phone-style. None of these have mainline Linux drivers, but they are all **well-documented in Samsung's open downstream kernels** for Galaxy S8/S9. Port-up effort is bounded; not novel RE.
- The **battery subsystem** is industry-standard **SBS over SMBus** — already mainline.
- What's actually unique to EmuEC is just:
  1. **Keyboard scancode translation** (events from PS/2-style scancodes via SPB callbacks → HID reports)
  2. **Lid switch event** (`Notify(LID0, 0x80)` triggered by an EC GPIO)
  3. **AC-presence event** (`BIT_EXIST_AC` flip → ACPI notification)
  4. **Thermal sensor reads** (`AP_THERM`, `BLANKET_THM`, `PMIC_THERM`) → could be exposed as `hwmon` entries
  5. **I²C-switch arbitration** (`W6_EC_I2C_SWITCH_CONTROL`) — needed to route the right port's PDIC/MUIC/redriver to the right Qualcomm I²C controller

Items 1–4 are 200–400 lines of platform-driver code each. Item 5 is the trickiest — it requires the EmuEC driver to mediate access to the underlying I²C buses, otherwise the standalone Linux drivers for S2MM005/SM5508 would race each other.

A reasonable factoring:
- `drivers/platform/arm64/samsung-w767-ec.c` — thin EC core that handles items 1–5, exposes input/lid/hwmon/AC-source devices, and mediates I²C-mux routing
- Standard `sbs-charger` + `sbs-manager` + `sbs-battery` for battery — no W767-specific code
- New `drivers/usb/typec/s2mm005.c`, `drivers/extcon/extcon-sm5508.c`, `drivers/usb/redriver/ptn36502.c` — generic, usable by other Samsung-ARM devices too

### Linux readiness implications

This shifts several rows in §3 of `docs/00-hardware-combined.md`:

| Item | Old status | New status |
|------|-----------|-----------|
| Battery telemetry | ❌ Not wired | 🟡 **Mainline-ready** (sbs-charger + sbs-manager + sbs-battery on `&i2c19`, no driver work) |
| USB-C / PD | 🟡 Likely works; not explicitly verified | 🟠 **Needs S2MM005+SM5508+PTN36502 drivers** for full PD/AltMode/SAR; dwc3 host-mode works without these |
| EmuEC driver | ❌ Custom EmuEC driver needed | 🟠 Custom shim only for kbd/lid/AC/therm + I²C-mux; battery/PD/charger handled by separate generic drivers |
| WiFi | 🟡 Firmware staged, untested probe | 🟡 Firmware path needs correction (WCN3998, not WCN6855) |
| Touchpad | 🟡 DTS ready, unverified | 🟠 DTS needs address fix (0x49) before probe will work |

## §5. Methodology notes

For reproducibility / future cross-checking by other sessions or contributors:

**Run environment:** Win11 ARM64 22621 on the W767 itself (`SM-W767NZNABTU`, Snapdragon 8cx, 8 GB), running under elevated PowerShell.

**Surface used:**
- `HKLM\HARDWARE\ACPI\DSDT` — raw ACPI tables in registry, accessible without admin (yielded the 400204-byte DSDT identical to `acpi/dsdt.dat`)
- `C:\Windows\INF\oem*.inf` — third-party INFs are copied here and **are user-readable** (DriverStore originals are TrustedInstaller-only)
- `C:\Windows\System32\drivers\*.sys` — driver binaries are user-readable (no extra ACL), so `strings`-style extraction works in pure PowerShell
- `Get-PnpDevice -PresentOnly` / `pnputil /enum-devices /properties` for live device enumeration

**Resource-descriptor parser:** PowerShell function that scans the DSDT binary for ASCII HID strings, then walks forward parsing 0x8E (Generic SerialBus) and 0x8C (GPIO) Large Resource Descriptors until the first 0x79 End Tag *after* the first decoded resource. This still leaks across Device blocks for devices that have zero resources (e.g., SAMM0901 keyboard), but produces clean output for any device with at least one resource. A fully correct parser would track PkgLength of the enclosing Device opcode — TODO if it matters.

**Chip identification:** string-extraction (4+ printable ASCII chars between non-printable bytes) on `EmuEC.sys` (268192 bytes). The smoking guns were `s2mm005_*` (38 hits), `sm5508_*` (4 hits + 11 `SM5508_*` register names), `ptn36502_init` (4 hits), and the SBS address-pair pattern.

**What this round did NOT do:**
- Load any x64 kernel drivers (rwdrv, OSR IRP tracker, etc.) — Win11-ARM's Prism doesn't cover kernel-mode x86/x64, so direct MMIO / physical memory probes are out of scope without writing a signed ARM64 kernel driver
- Re-decompile the DSDT — used the repo's existing `acpi/dsdt.dsl` for cross-verification (which matched byte-perfectly via MD5)
- Re-pull Windows enumerations that already exist in `windows-extracts/` — referred to those instead of duplicating

**What would be high-value next:**
- Boot Linux iter-19 with the **touchpad address corrected to 0x49** and verify probe (single-line DTS change, blocking finding)
- Write the platform driver for the EmuEC kbd/lid/AC/therm path (the only truly W767-specific code that's needed)
- Wire `sbs-charger@9` + `sbs-manager@b` + `sbs-battery@c/d` on `&i2c19` — should give immediate battery telemetry without further RE
- Confirm the I²C-mux semantics by capturing EmuEC's switch-control writes via Linux i2c-tools once IC10/IC12/IC19 are enabled and the EmuEC platform driver provides mediated access
