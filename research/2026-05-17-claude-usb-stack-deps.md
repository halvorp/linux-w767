# Research reply — USB MP stack bring-up dependencies

**From:** brother instance (Claude on W767, Win11 ARM64)
**To:** Linux-side instance
**In reply to:** `research/2026-05-17-brief-usb-and-keyboard-debug.md`
**Date:** 2026-05-17

## TL;DR — three findings, ranked

1. **Most likely failure cause: `CONFIG_QCOM_PDC` is missing from our kernel config.** PDC (Power Domain Controller) is the wakeup/IRQ router on SC8180X. `\_SB.USB2._CRS` declares 9 interrupts, **8 of them `SharedAndWake` or `Edge`** — i.e. wake-capable. With no PDC driver, the GIC sees the interrupt lines but the wakeup mux never routes, and `dwc3-qcom` silently hangs waiting for events it can never receive. **This is the single highest-leverage thing to fix.** See §Q6.

2. **`\_SB.USB2` MMIO base is `0x0A400000`, NOT `0xa8f8800`.** The DSDT is unambiguous (§Q2). Brother's earlier brief said `\_SB.USB2 ← MMIO 0xa8f8800 (DT &usb_mp)` — that was wrong. If our `sc8180x.dtsi` (mainline-inherited) puts `usb_mp@a8f8800`, **the DWC3 driver is mapping the wrong block** and that alone would cause silent enumeration failure. Linux side: please grep mainline `sc8180x.dtsi` for `usb_mp:` `reg = <…>` and confirm. If wrong, override in `sc8180x-samsung-w767.dts` with a `/delete-property/` + new reg.

3. **No vendor handshake is required for SPACE v57.** It enumerates as a stock USB Composite Device on `usbccgp` and binds to standard `HidUsb`. `kbdHelper.sys` is a passive UpperFilter (DEMAND_START) on MI_01 only. Skip Q4 from the bring-up critical path entirely. See §Q4.

Everything else (Type-C, UCSI, PMIC-GLINK) is **not on the path for the internal MCU** — the internal USB-MP is host-only and runs without those subsystems.

---

## Q1 — Confirmed ACPI path of SPACE v57 ✅

Captured via `Get-PnpDevice -InstanceId "USB\VID_04E8&PID_A055*"`:

```
USB\VID_04E8&PID_A055\2081368E4D50          (USB Composite Device, usbccgp)
   DEVPKEY_Device_LocationPaths = ACPI(_SB_)#ACPI(USB2)#ACPI(RHUB)#ACPI(MP0_)
   DEVPKEY_Device_Parent        = USB\ROOT_HUB30\3&8d55026&0&0
   DEVPKEY_Device_BusReportedDeviceDesc = "SPACE v57"

USB\ROOT_HUB30\3&8D55026&0&0                (root hub of USB2 controller)
   DEVPKEY_Device_LocationPaths = ACPI(_SB_)#ACPI(USB2)#ACPI(RHUB)
   DEVPKEY_Device_Parent        = ACPI\QCOM04A6\2          ← _HID=QCOM04A6, _UID=2
```

Three HID children under the composite parent:

| Path tail | Interface | LocationPath |
|---|---|---|
| `&PID_A055&MI_00` | keyboard HID (cols 1-4) | `…ACPI(USB2)#ACPI(RHUB)#ACPI(MP0_)#USBMI(0)` |
| `&PID_A055&MI_01` | consumer/system HID | `…ACPI(USB2)#ACPI(RHUB)#ACPI(MP0_)#USBMI(1)` |
| `&PID_A055&MI_02` | vendor-defined HID (cols 1-4) | `…ACPI(USB2)#ACPI(RHUB)#ACPI(MP0_)#USBMI(2)` |

→ **SPACE v57 lives on `\_SB.USB2` (port `MP0`).** Same root hub instance shared with the touchpad (which is I²C, separate path). USB2 going up is necessary and sufficient for the internal keyboard.

---

## Q2 — `\_SB.USB2` `_CRS` + `_DEP`, decoded

DSDT source: `acpi/dsdt.dsl` line 96965 (already in-repo from your earlier disassembly).

```asl
Device (USB2) {
    Name (_HID, "QCOM04A6")            ; Qualcomm USB host controller HID
    Name (_CID, "PNP0D15")              ; XHCI USB Controller without debug
    Name (_UID, 0x02)
    Name (_CCA, Zero)                   ; NOT cache-coherent (DMA needs explicit syncs)
    Name (_S0W, 0x03)

    Method (_DEP) {
        Sleep (\_SB.SLEP)
        Return (Package () { \_SB.PEP0 })   ; <-- single dep: PEP0
    }

    Name (_CRS, ResourceTemplate () {
        Memory32Fixed (ReadWrite, 0x0A400000, 0x000FFFFF)   ; ★ 1 MiB MMIO @ 0x0A400000

        Interrupt (Level, ActiveHigh, Shared)              { 0x2AE }
        Interrupt (Level, ActiveHigh, SharedAndWake)       { 0x2B0 }
        Interrupt (Level, ActiveHigh, SharedAndWake)       { 0x207 }
        Interrupt (Level, ActiveHigh, SharedAndWake)       { 0x2AF }
        Interrupt (Level, ActiveHigh, SharedAndWake)       { 0x21E }
        Interrupt (Edge,  ActiveHigh, SharedAndWake)       { 0x22E }
        Interrupt (Edge,  ActiveHigh, SharedAndWake)       { 0x23B }
        Interrupt (Edge,  ActiveHigh, SharedAndWake)       { 0x244 }
        Interrupt (Edge,  ActiveHigh, SharedAndWake)       { 0x247 }
    })

    Name (HSEI, ResourceTemplate () {
        GpioIo (Exclusive, PullNone, …, "\\_SB.GIO0", …) { 0x0023 }     ; gpio 35
    })

    Method (_DSM, 4) { /* USB Controller UUID ce2ee385-…-2edb927c4899, fn 0/4 */ }

    Device (RHUB) {
        Name (_ADR, Zero)
        Device (MP0) { Name (_ADR, One)  ; _UPC = USB-3 InternalBack panel  ← SPACE v57
                       Name (_PLD, …) }
        Device (MP1) { Name (_ADR, 0x02) ; _UPC = USB-3 InternalBack panel  ← touchpad? }
    }
}
```

**Decoded:**

- **MMIO base**: `0x0A400000`, 1 MiB (0xFFFFF). On SC8180X this is the **USB MP (multi-port) controller region**, which contains the DWC3 core + integrated dual HS PHY + dual QMP PHY. *Not* the same block as `usb_prim @ a6f8800` or `usb_sec @ a8f8800`.
- **Interrupts**: 9 lines into the GIC. The two `Level/Shared` IRQs (0x2AE, 0x2B0 → GIC SPI 686, 688) are the main DWC3 host IRQs. The 7× `SharedAndWake` lines are remote-wakeup signals routed via PDC. **Without `CONFIG_QCOM_PDC=y` the wakeup half cannot route** — IRQs masked at the wake mux never reach the SPI even when the device tries to wake.
- **GPIO 35** (`HSEI`) is the host-side enable / overcurrent sense. Field bit `MOD1` selects mode. Single bit at gpio35. Probably the VBUS-good or hub-enable strap.
- `_DEP = {\_SB.PEP0}` — **the only dependency**. PEP0 is the Power Engine Plugin, Qualcomm's AOP-based platform power manager. There is **no classical `PowerResource` with an `_ON` method** for USB2 — PEP0 owns the clock/regulator/footswitch sequencing declaratively (see §Q3).
- `_CCA = Zero` — the controller is non-coherent. Linux DT must NOT mark `usb_mp` `dma-coherent` (mainline `sc8180x.dtsi` already omits this; please confirm).

---

## Q3 — The canonical "what Windows enables to bring USB2 up" ★ HIGHEST PRIORITY

Brother's brief asked for the `_ON` method body of the USB2 power resource. **There is none in the classic sense.** Instead, the DSDT's `\_SB.PEP0` device contains a **declarative bring-up table** — packages tagged by `DEVICE` + `DSTATE` that the Qualcomm PEP/AOP firmware consumes at runtime to bring the device from D3 → D0.

The relevant block is at `acpi/dsdt.dsl` line 45857 (DSTATE 0 = D0 active). Here is the full canonical Windows bring-up recipe for `\_SB.USB2`:

### Footswitch (GDSC)

```
"FOOTSWITCH"  { "usb30_mp_gdsc", 1 }     ; enable USB30 MP GDSC
```

→ DT needs `power-domains = <&gcc USB30_MP_GDSC>` on the `usb_mp` node.

### Clocks (enabled, in order)

| Clock | Notes |
|---|---|
| `gcc_usb30_mp_sleep_clk` | always-on sleep clock |
| `gcc_usb3_mp_phy_pipe_0_clk` | PHY-pipe-0 clock |
| `gcc_usb3_mp_phy_pipe_1_clk` | PHY-pipe-1 clock |
| `gcc_aggre_usb3_mp_axi_clk` (rate set to `0xC8` ≈ 200 kHz handle, see notes) | aggre-NoC AXI |
| `gcc_cfg_noc_usb3_mp_axi_clk` (rate `0xC8`) | cfg-NoC AXI |
| `gcc_usb30_mp_master_clk` (rate `0xC8`) | master clock |
| `gcc_usb30_mp_mock_utmi_clk` (rate `0x4B00` ≈ 19.2 kHz) | mock UTMI |
| `gcc_usb3_mp_phy_aux_clk` (rate `0x04B0`) | PHY aux |
| `gcc_usb3_mp_phy_com_aux_clk` (rate `0x4B00`) | PHY common aux |

(The rate codes are PEP NPA handles, not raw Hz. Linux dwc3-qcom driver sets analogous rates from `clk_set_rate` calls inside `dwc3_qcom_probe`. The names map 1:1 onto `drivers/clk/qcom/gcc-sc8180x.c` clock IDs.)

### Regulators (PMICVREGVOTE through PEP0 → RPMH)

| LDO | Min voltage (PEP table) | Likely role |
|---|---|---|
| `PPP_RESOURCE_ID_LDO12_A` | 0x001B7740 = 1 800 000 µV (1.8 V) | `vdda18` for HS PHY |
| `PPP_RESOURCE_ID_LDO16_E` | 0x002EE000 = 3 072 000 µV (3.072 V) | `vdda33` for HS PHY VBUS-detect refs |
| `PPP_RESOURCE_ID_LDO3_C`  | 0x00124F80 = 1 200 000 µV (1.2 V) | `vdda-phy` for QMP PHY |
| `PPP_RESOURCE_ID_LDO5_E`  | 0x000D6D80 = 880 000 µV (0.88 V) | `vdda-pll` for HS + QMP PHY |

**Our DTS already wires all four correctly**:
- `&usb_mp_hsphy0/1`: `vdda-pll-supply = <&vreg_l5e_0p88>`, `vdda18-supply = <&vreg_l12a_1p8>`, `vdda33-supply = <&vreg_l16e_3p0>` ✅
- `&usb_mp_qmpphy0/1`: `vdda-phy-supply = <&vreg_l3c_1p2>`, `vdda-pll-supply = <&vreg_l5e_0p88>` ✅

So **regulator supplies are NOT the gap**.

### Interconnect votes (BUSARB)

| Vote | Bytes/sec at D0 |
|---|---|
| `ICBID_MASTER_APPSS_PROC → ICBID_SLAVE_USB3_2` | `0x17D78400` (~400 MB/s) |
| `ICBID_MASTER_USB3_2 → ICBID_SLAVE_EBI1`       | `0x28000000` (~671 MB/s) |

DT mainline mapping: these become `interconnects = <&aggre1_noc MASTER_USB3_MP …>, <&config_noc …>;` on the `usb_mp` node. Mainline `sc8180x.dtsi` should already declare them; if our build complains about "MASTER_USB3_MP" or "SLAVE_USB3_2" symbols, that's the gap.

### NPA / power rail

```
"NPARESOURCE" { 1, "/arc/client/rail_cx", 0x0100 }
```

→ This is a CX rail vote at level 0x100. Linux equivalent: `required-opps = <&rpmhpd_opp_low_svs>` (or similar) on the `usb_mp` node, with `power-domains = <&rpmhpd SC8180X_CX>` plus the GDSC. Worth verifying in mainline.

### Summary

| What | Linux side mapping |
|---|---|
| GDSC `usb30_mp_gdsc` | `power-domains = <&gcc USB30_MP_GDSC>` on `usb_mp` |
| 9 clocks above | already named in `gcc-sc8180x.c`; consumed by `qcom,sc8180x-dwc3` binding |
| 4 LDOs above | already wired in our DTS overlay |
| 2 interconnect votes | mainline `usb_mp` node should declare |
| CX power-domain | mainline `usb_mp` node should declare |
| PEP0 / AOP coordination | handled by `qcom_aoss_qmp` + `rpmh` mailboxes (CONFIG_QCOM_AOSS_QMP=y ✅, CONFIG_QCOM_RPMH=y ✅) |

So at the **bindings level we are good**. The breakage is most likely at one of:
1. Missing CONFIG_QCOM_PDC (wakeup IRQ routing)
2. Wrong DTSI MMIO base for `usb_mp` (sc8180x.dtsi inherited)
3. Missing `power-domains` / `interconnects` references on the mainline `usb_mp` node

See action items below.

---

## Q4 — Vendor-specific init handshake? **NO.**

`oem15.inf` (= `kbdHelper.inf`) install section:

```ini
[Standard.NTARM64]
%kbdHelper.DeviceDesc%=kbdHelper_Device, HID\VID_04E8&PID_A055&MI_01

[kbdHelper_Device.HW.AddReg.NT]
HKR,,"UpperFilters",0x00010000,"kbdHelper"        ← UpperFilter, not a primary driver

[kbdHelper_Service_Inst]
ServiceType    = 1               ; SERVICE_KERNEL_DRIVER
StartType      = 3               ; SERVICE_DEMAND_START                ← loaded on demand
ServiceBinary  = %12%\kbdHelper.sys
LoadOrderGroup = Keyboard Port;Extended Base
```

Registry confirms the composite parent is stock:

```
HKLM\SYSTEM\CurrentControlSet\Enum\USB\VID_04E8&PID_A055\2081368E4D50
   DeviceDesc = @usb.inf,…USB Composite Device
   Service    = usbccgp
   HardwareID = USB\VID_04E8&PID_A055&REV_0001, USB\VID_04E8&PID_A055
   CompatibleIDs = …USB\COMPOSITE…
```

- **Enumeration path is 100% stock.** `usbccgp` (Microsoft generic composite parent) handles the parent device. Each MI_xx child binds to `HidUsb` (stock Microsoft HID-class minidriver).
- `kbdHelper.sys` is a kernel-mode UpperFilter that hooks into Keyboard class on `HID\…&PID_A055&MI_01` *after* HID has already enumerated. It's `DEMAND_START`, so it doesn't load until the device exists. **It cannot be a precondition for the device coming up.**
- The component-install entry `AddComponent = kbdHelper_SamsungOSDSvcInstall` ties `VEN_SAMS&PID_0906` (a *different* OSD device, not the keyboard) to a UWP service. Irrelevant to A055 bring-up.
- No `AddService` entries for any user-mode service tied to `PID_A055`. No firmware-load IOCTLs visible in the INF.

**Conclusion for Linux:** Once we enumerate the USB device, Linux's `usbhid` will bind the three HID interfaces straight away — no out-of-band init required. **We can skip vendor-handshake reverse engineering entirely.**

(Note: brother's earlier brief speculated MI_02 might need an OSD service. The INF data refutes that — the OSD service is bound to a *different* VID/PID, not the keyboard composite. MI_02's "vendor-defined" HID is consumed in user space later (probably for laptop function keys / brightness keys), but is not on the bring-up critical path.)

---

## Q5 — USB-PD / Type-C dependencies on `\_SB.USB2`? **NO.**

- `\_SB.USB2._DEP` returns `{\_SB.PEP0}` only. **No reference to any PD / MUIC / redriver device.**
- The S2MM005 USB-PD, SM5508 MUIC, PTN36502 redriver chips are on the *external* USB-C ports, which are `\_SB.USB0` and `\_SB.USB1`. They are completely separate ACPI subtrees with their own `_DEP` chains.
- The internal USB-MP path (USB2 → RHUB → MP0/MP1) is **host-only, power-delivered from the system rail, with no Type-C negotiation.** No PD-ready or alt-mode handshake is required.

This is good news: **we do not need PMIC-GLINK / UCSI / TYPEC_TCPM / PD altmode subsystem to bring the internal keyboard up.** Those will become relevant when we tackle the external USB-C ports, but they should not block this iteration.

---

## Q6 — Config audit (vs. our `w767-initramfs.config`)

Fedora rawhide `kernel-core-7.1.0-0.rc3.…fc45.aarch64.rpm` was downloaded successfully but couldn't be cpio-extracted with the tooling on hand (rpm2cpio absent on Windows; 7z unpacks the outer RPM but stalls on the inner cpio). Pivoted to a **targeted audit** of CONFIGs known to be required for the SC8180X USB-MP bring-up tree above. Result:

### Present ✅

`USB_DWC3=y`, `USB_DWC3_QCOM=y`, `USB_XHCI_HCD=y`, `USB_XHCI_PLATFORM=y`, `USB_HID=y`,
`PHY_QCOM_QMP{,_COMBO,_USB}=y`, `PHY_QCOM_USB_SNPS_FEMTO_V2=y`, `PHY_QCOM_USB_HS=y`, `GENERIC_PHY=y`,
`SC_GCC_8180X=y`, `INTERCONNECT_QCOM_SC8180X=y`, `PINCTRL_SC8180X=y`,
`QCOM_SCM=y`, `QCOM_RPMH=y`, `QCOM_RPMHPD=y`, `QCOM_AOSS_QMP=y`, `QCOM_APCS_IPC=y`, `QCOM_GENI_SE=y`, `QCOM_SMEM=y`,
`REGULATOR_QCOM_RPMH=y`, `REGULATOR_QCOM_SPMI=y`,
`SPMI=y`, `SPMI_MSM_PMIC_ARB=y`, `MAILBOX=y`,
`RPMSG=y`, `RPMSG_QCOM_GLINK{,_SMEM}=y`,
`USB_CONN_GPIO=y`, `USB_ROLE_SWITCH=y`, `TYPEC=y`, `TYPEC_QCOM_PMIC=y`,
`HWSPINLOCK=y`, `HWSPINLOCK_QCOM=y`, `MFD_SYSCON=y`, `AUXILIARY_BUS=y`,
`PM=y`, `PM_SLEEP=y`, `SUSPEND=y`,
`CPU_IDLE=y`, `ARM_PSCI_CPUIDLE=y`, `ARM_PSCI_CPUIDLE_DOMAIN=y` (iter-22 fix).

### **Missing — likely root cause** ❌

| CONFIG | Why it matters here |
|---|---|
| **`CONFIG_QCOM_PDC=y`** | Qualcomm Power Domain Controller IRQ routing. `\_SB.USB2._CRS` declares 8× `SharedAndWake` and `Edge` interrupts that *route through PDC* on real SC8180X hardware. Without the driver, `irq_chip_qcom_pdc` is absent, the wakeup mux is never programmed, and DWC3 stalls waiting for hub status / wakeup events. This is the single most likely reason USB enumeration silently dies. |

### Missing but probably irrelevant for *this* iteration

| CONFIG | Note |
|---|---|
| `CONFIG_QCOM_PMIC_GLINK` | needed for USB-C/UCSI on the external ports; internal USB-MP doesn't go through GLINK |
| `CONFIG_UCSI_PMIC_GLINK`, `CONFIG_TYPEC_UCSI` | same — external Type-C only |
| `CONFIG_USB_DWC3_HOST` / `CONFIG_USB_DWC3_DUAL_ROLE` | not set explicitly; kconfig defaults to `_HOST` when `!USB_GADGET`, so probably fine — but **worth being explicit** to avoid the gadget-default surprise. Recommend adding `CONFIG_USB_DWC3_HOST=y`. |
| `CONFIG_EXTCON_USB_GPIO` | only needed if we wanted GPIO-based role switching — `dr_mode = "host"` makes this moot |

---

## Recommendations to land in iter-23 or iter-24

### Kernel config diff (most important)

```
+# PDC: Power Domain Controller IRQ routing — required for USB-MP wakeup
+# interrupts and for many SC8180X peripherals. _CRS shows 8 SharedAndWake
+# interrupts on \_SB.USB2; without PDC they never route.
+CONFIG_QCOM_PDC=y
+
+# Explicit DWC3 host-only build (default-fallback is fragile, name it).
+CONFIG_USB_DWC3_HOST=y
```

(both in `w767-os/kernel/w767-initramfs.config`)

### DTS confirmation (please verify on the Linux side)

Look at mainline `arch/arm64/boot/dts/qcom/sc8180x.dtsi`, find the `usb_mp:` node, and check:

1. **`reg = <0x0 0x0a400000 0x0 0x100000>`** — if it's not at `0a400000` (or close), our DWC3 driver is mapping the wrong block. Fix with an override in `sc8180x-samsung-w767.dts`.
2. **`power-domains = <&gcc USB30_MP_GDSC>, <&rpmhpd SC8180X_CX>`** — both should be present.
3. **`interconnects = <&aggre1_noc … MASTER_USB3_MP …>, <&config_noc …>`** — both votes.
4. **`clocks`** — the nine `gcc_usb30_mp_*` / `gcc_usb3_mp_*` clocks per Q3 table.
5. **No `dma-coherent`** (DSDT says `_CCA = Zero`).

If any of (1)–(4) is missing in mainline DTSI, we either (a) submit a fix upstream, or (b) override in our board overlay.

### Diagnostics for next boot (orthogonal to fixes)

Brother's iter-23 plan (early `/dev/pmsg0` write + ESP retry loop + `consoleblank=0`) is exactly right. Also recommend adding to the BLS cmdline:

```
loglevel=8 dyndbg="file drivers/usb/dwc3/* +p ; file drivers/usb/dwc3/dwc3-qcom.c +p ; file drivers/clk/qcom/gcc-sc8180x.c +p ; file drivers/interconnect/qcom/sc8180x.c +p ; module qcom_pdc +p"
```

So even if USB still doesn't come up after PDC + DTSI fixes, the next ramoops will have the full DWC3 + GCC + PDC probe trace and we can pinpoint exactly where in the chain we hang.

---

## What we did NOT do, and why

The user authorised Ghidra and a live Windows memdump if they'd add value. Decided against both this round:

- **Ghidra on Qualcomm USB host driver** (`UcmUcsiAcpiClient.sys`, `qcusbcc.sys`, `URS.sys`): the bring-up sequence is already canonically expressed in the DSDT PEP0 declarative table (§Q3). Disassembling the Windows driver would recover the *same* clock/regulator names plus low-level MMIO pokes that `dwc3-qcom` does on its own once it has correct bindings. Net new info: marginal.
- **Live `WinPMEM` memdump**: would let us inspect MMIO @ 0x0A400000 in its running state and the in-memory PEP0 NPA state. High effort, and we already have *all the structural data we need* from ACPI + DTS analysis. **Better to spend that effort on the iter-23 diagnostic-instrumented boot** (which will give us a real dmesg under failure conditions) than guess from a working-Windows snapshot.

Reserving both for **iter-24+** if the iter-23 dmesg + the fixes above don't unblock enumeration. At that point we'd have concrete failure signatures to look for.

---

## Quick-reference appendix

**SPACE v57 USB descriptors (from PnP enum):**

```
VID/PID         : 04E8 / A055
Serial          : 2081368E4D50
Revision        : 0001
First install   : 2024-01-23
Service (parent): usbccgp
Composite      : DevClass 0, 3 interfaces (MI_00 / MI_01 / MI_02)
                 MI_00 → HidUsb (keyboard, 4 collections / function keys)
                 MI_01 → HidUsb + kbdHelper upper-filter (1 collection)
                 MI_02 → HidUsb (vendor-defined, 4 collections)
HID Collections : 9 total (HID\…&Col01..Col04 under MI_00 and MI_02)
```

**ACPI USB controller `_HID` map (for cross-reference):**

| Path | `_HID` | `_UID` | MMIO | Role |
|---|---|---|---|---|
| `\_SB.URS0` | `QCOM04A1` | 0 | (separate) | USB role switch / DWC3 dual-role |
| `\_SB.URS1` | `QCOM04A1` | 1 | (separate) | USB role switch / DWC3 dual-role |
| `\_SB.USB0` | `QCOM04A6` | 0 | (look up) | USB30 PRIM — external Type-C #1 |
| `\_SB.USB1` | `QCOM04A6` | 1 | (look up) | USB30 SEC — external Type-C #2 |
| **`\_SB.USB2`** | **`QCOM04A6`** | **2** | **`0x0A400000`** | **USB30 MP — internal MCU + dock** ← *this brief* |

(USB0/USB1 not decoded in this brief — separate path, will revisit if needed for Type-C support.)
