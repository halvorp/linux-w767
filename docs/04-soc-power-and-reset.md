# 04 — SoC-Level Subsystems (Power, Clocks, Remoteprocs, Thermals, I/O)

> **Scope.** This document documents the SoC-internal subsystems of the
> Samsung Galaxy Book S (SM-W767NZNABTU) around the Qualcomm Snapdragon 8cx
> / SC8180X platform: power-management (PMICs, regulators, power-domains),
> clocks, thermals, sleep states, remote processors (Hexagon aDSP / cDSP /
> modem / SLPI), the fabric (NoC interconnects), IOMMU/SMMU units, firmware
> blobs, audio, wireless, modem, battery, UFS storage, memory, and the EFI
> boot surface.
>
> Sibling documents cover:
>
> * Display / eDP panel / MDSS / DSI / DP / GPU presentation — see the
>   sibling "Display subsystem" document.
> * Samsung-platform specifics (EMEC, touchpad, keyboard, SAM* ACPI IDs,
>   Samsung OSD service, etc.) — see the sibling "Samsung platform" document.
> * Bus device inventory (complete I²C/SPI/UART/GPIO/SPMI enumeration with
>   addresses, pinctrl, interrupt lines) — see the sibling "Bus inventory"
>   document.
>
> Evidence is quoted verbatim from the indicated sources. File paths are
> absolute and refer to the in-tree sources under `/home/peter/Documents/GalaxyBookS_Linux/`.

---

## 1. Executive summary

Machine identity, taken directly from `win-extract/baseboard.txt`,
`win-extract/bios.txt` and `win-extract/computerinfo.txt`:

```
Manufacturer         : SAMSUNG ELECTRONICS CO., LTD.
Product              : SM-W767NZNABTU
Base-Board version   : SGLA125A6I-C01-G001-S0001+10.0.19041
BIOS (AMI)           : P02AHP.003.241226.WY.1518 (2024-12-26)
BIOS family string   : QCOM   - 8180
System family / SKU  : Galaxy Book Series / GALAXY A5A5-PAHP
Form factor          : Slate (CsPCSystemTypeEx : Slate)
CPU                  : Snapdragon (TM) 8cx @ 2.84 GHz, 8 logical cores
Firmware type        : UEFI  (BiosFirmwareType : Uefi)
RAM                  : 8 GiB LPDDR4X, Samsung, SMBIOSMemoryType = 30 (LPDDR4X)
Secure Boot /
 Code-Integrity      : HypervisorEnforcedCodeIntegrity running (HVCI)
```

DTS binding and current Linux-boot status for every subsystem covered below:

| Subsystem            | Mainline compatible                            | Linux status on w767 | Windows service / driver |
|----------------------|------------------------------------------------|----------------------|--------------------------|
| ADSP (audio DSP)     | `qcom,sc8180x-adsp-pas`                        | loads                | `qcadsprpcd` + `qcauddev8180` (Aqstic) |
| CDSP (compute DSP)   | `qcom,sc8180x-cdsp-pas`                        | loads                | `qcadsprpcd` (shared)    |
| MPSS (modem)         | `qcom,sc8180x-mpss-pas`                        | does not load        | `mbb` / `qcmbb8180`      |
| SLPI (sensor island) | `qcom,sc8180x-slpi-pas`                        | not configured (no DT node on mainline) | `qcsensors8180` + `qcslimbus` |
| Primary PMIC A       | `qcom,pm8150` @ SPMI USID 0                    | configured           | `qcpmic8180` / `qcpmicapps8180` |
| Companion PMIC C     | `qcom,pm8150c` @ SPMI USID 4, `qcom,pmc8180c` @ USID 5 | configured           | `qcpmicgpio8180`, PMIC-LPG |
| Extension PMIC E     | `qcom,pm8150` @ SPMI USID 8                    | configured (pmc8180-e) | `secpmic3p.inf`        |
| GCC clock controller | `qcom,gcc-sc8180x`                             | yes (mainline default) | kernel-internal        |
| GPU CC               | `qcom,sc8180x-gpucc`                           | enabled (gpu `okay`) |                          |
| Display CC           | `qcom,sc8180x-dispcc`                          | enabled (`dispcc { status = "okay"; }`) | `qcdx8180` |
| Video CC             | `qcom,sc8180x-videocc`                         | not enabled          | `qcvs/qcvss`             |
| Camera CC            | `qcom,sc8180x-camcc`                           | not enabled          | `qccamplatform8180` et al. |
| Interconnect fabric  | `qcom,sc8180x-{gem,mmss,config,system,aggre1,aggre2,compute}-noc` + `qcom,sc8180x-osm-l3` | mainline default | `qctree8180` |
| Apps SMMU            | `qcom,sc8180x-smmu-500`, `arm,mmu-500`         | mainline default     | `qcsmmu8180` + `qciommu8180` |
| Adreno SMMU          | `qcom,sc8180x-smmu-500`, `qcom,adreno-smmu`    | mainline default     | in `qcdx8180`            |
| WLAN (WCN3990)       | `qcom,wcn3990-wifi`                            | DT node enabled      | `qcwlan` / `qcwlan8180`  |
| Bluetooth over UART13| `qcom,wcn3998-bt`                              | DT node enabled      | `BthMini` / `QcBluetooth` (QCOM0471) |
| MPSS modem (X24 LTE) | `qcom,sc8180x-mpss-pas`                        | not loading          | `mbb` (QCOM0489)         |
| Battery / fuel-gauge | not mainline (Samsung EMEC ACPI path)          | absent               | `qcbattminiclass` + `SAM0604` EMEC |
| UFS 3.0 host + PHY   | `qcom,sc8180x-ufshc`, `qcom,sc8180x-qmp-ufs-phy` | enabled              | native UFS in UEFI      |
| TSENS                | `qcom,sc8180x-tsens`, `qcom,tsens-v2`          | mainline default     | `qcthermal`/`qcpep`      |
| PMIC temp-alarms     | `qcom,spmi-temp-alarm`                         | mainline default     | `qcpep`                  |
| Cpufreq / LMH        | `qcom,sc8180x-cpufreq-hw`, `qcom,sc8180x-lmh`  | mainline default     | `qcpep`                  |

---

## 2. Remoteproc subsystems

The SC8180X/8cx carries four Peripheral-Authenticated-Subsystem (PAS)
processors that Linux drives via `qcom_pas` / `remoteproc`:

* aDSP (audio Hexagon)
* cDSP (compute Hexagon)
* MPSS (modem Hexagon; Snapdragon X24 LTE)
* SLPI (Sensor Low-Power Island Hexagon)

Each must have a reserved-memory region, a signed ELF `.mbn`, CX (and for
modem MSS) power domain votes, AOSS-QMP access, and SMP2P + GLINK channels.

### 2.1 ADSP — `remoteproc_adsp`

From `mainline-dts/sc8180x.dtsi` lines 3733–3764 (verbatim):

```
remoteproc_adsp: remoteproc@17300000 {
    compatible = "qcom,sc8180x-adsp-pas";
    reg = <0x0 0x17300000 0x0 0x4040>;

    interrupts-extended = <&intc GIC_SPI 162 IRQ_TYPE_EDGE_RISING>,
                          <&adsp_smp2p_in 0 IRQ_TYPE_EDGE_RISING>,
                          <&adsp_smp2p_in 1 IRQ_TYPE_EDGE_RISING>,
                          <&adsp_smp2p_in 2 IRQ_TYPE_EDGE_RISING>,
                          <&adsp_smp2p_in 3 IRQ_TYPE_EDGE_RISING>;
    interrupt-names = "wdog", "fatal", "ready",
                      "handover", "stop-ack";

    clocks = <&rpmhcc RPMH_CXO_CLK>;
    clock-names = "xo";

    power-domains = <&rpmhpd SC8180X_CX>;
    power-domain-names = "cx";

    qcom,qmp = <&aoss_qmp>;

    qcom,smem-states = <&adsp_smp2p_out 0>;
    qcom,smem-state-names = "stop";

    status = "disabled";

    remoteproc_adsp_glink: glink-edge {
        interrupts = <GIC_SPI 156 IRQ_TYPE_EDGE_RISING>;
        label = "lpass";
        qcom,remote-pid = <2>;
        mboxes = <&apss_shared 8>;
    };
};
```

Board-level override in `dts-stage-v2/sc8180x-samsung-w767.dts` lines 685–689:

```
&remoteproc_adsp {
    status = "okay";
    memory-region = <&adsp_mem>;
    firmware-name = "qcom/samsung/w767/qcadsp8180.mbn";
};
```

Reserved-memory region (same file, lines 53–56):

```
adsp_mem: memory@97800000 {
    reg = <0x0 0x97800000 0x0 0x2000000>;
    no-map;
};
```

* Base: `0x97800000`, Size: `0x02000000` (32 MiB).
* Firmware: `/lib/firmware/qcom/samsung/w767/qcadsp8180.mbn` (11 008 656 bytes).
* QMI service registry metadata ships in `adspr.jsn` (`domain: adsp, subdomain: root_pd, qmi_instance_id: 74`), `adspua.jsn` (`subdomain: audio_pd`).
* Linux-boot status: **loads** (confirmed by test iteration 16 + ADSP loader
  messages). The Hexagon is a Hexagon 690 — Windows surfaces it as
  `FriendlyName : Qualcomm(R) Hexagon(TM) 690 DSP` (from `pnp_all.txt`).
* Windows driver chain: `qcadsprpcd8180.inf` (user-mode RPC daemon) +
  `qcadsprpc8180.inf` (kernel transport). Audio-specific codec stack layered
  above at ACPI device `ADSP\VEN_QCOM&DEV_0410`, service `qcslimbus`,
  "Qualcomm(R) Bus Device". Codec driver: `qcauddev8180.inf` /
  `qcauddev8180_ss.inf` (Samsung SKU variant).

### 2.2 CDSP — `remoteproc_cdsp`

From `mainline-dts/sc8180x.dtsi` lines 2455–2486:

```
remoteproc_cdsp: remoteproc@8300000 {
    compatible = "qcom,sc8180x-cdsp-pas";
    reg = <0x0 0x08300000 0x0 0x4040>;

    interrupts-extended = <&intc GIC_SPI 578 IRQ_TYPE_EDGE_RISING>,
                          <&cdsp_smp2p_in 0 IRQ_TYPE_EDGE_RISING>,
                          <&cdsp_smp2p_in 1 IRQ_TYPE_EDGE_RISING>,
                          <&cdsp_smp2p_in 2 IRQ_TYPE_EDGE_RISING>,
                          <&cdsp_smp2p_in 3 IRQ_TYPE_EDGE_RISING>;
    interrupt-names = "wdog", "fatal", "ready",
                      "handover", "stop-ack";

    clocks = <&rpmhcc RPMH_CXO_CLK>;
    clock-names = "xo";

    power-domains = <&rpmhpd SC8180X_CX>;
    power-domain-names = "cx";

    qcom,qmp = <&aoss_qmp>;

    qcom,smem-states = <&cdsp_smp2p_out 0>;
    qcom,smem-state-names = "stop";

    status = "disabled";

    glink-edge {
        interrupts = <GIC_SPI 574 IRQ_TYPE_EDGE_RISING>;
        label = "cdsp";
        qcom,remote-pid = <5>;
        mboxes = <&apss_shared 4>;
    };
};
```

Board override lines 759–764:

```
&remoteproc_cdsp {
    status = "okay";

    memory-region = <&cdsp_mem>;
    firmware-name = "qcom/samsung/w767/qccdsp8180.mbn";
};
```

Reserved region lines 58–61:

```
cdsp_mem: memory@99800000 {
    reg = <0x0 0x99800000 0x0 0x800000>;
    no-map;
};
```

* Base `0x99800000`, size `0x00800000` (8 MiB).
* Firmware: `qccdsp8180.mbn` (3 114 644 bytes).
* Service-registry: `cdspr.jsn` (`domain: cdsp, subdomain: root_pd, qmi_instance_id: 76`).
* Linux-boot status: **loads**. Both ADSP and CDSP load cleanly in the
  current iteration 16 DTS.
* Windows: no dedicated user-friendly node; compute HVX offload is driven by
  `qcadsprpcd` over the same RPC transport. The `qclistensm_swc8180.inf`
  contains the soundmodel listener code that runs on CDSP.

### 2.3 MPSS — `remoteproc_mpss`

From `mainline-dts/sc8180x.dtsi` lines 2422–2453:

```
remoteproc_mpss: remoteproc@4080000 {
    compatible = "qcom,sc8180x-mpss-pas";
    reg = <0x0 0x04080000 0x0 0x4040>;

    interrupts-extended = <&intc GIC_SPI 266 IRQ_TYPE_EDGE_RISING>,
                          <&modem_smp2p_in 0 IRQ_TYPE_EDGE_RISING>,
                          <&modem_smp2p_in 1 IRQ_TYPE_EDGE_RISING>,
                          <&modem_smp2p_in 2 IRQ_TYPE_EDGE_RISING>,
                          <&modem_smp2p_in 3 IRQ_TYPE_EDGE_RISING>,
                          <&modem_smp2p_in 7 IRQ_TYPE_EDGE_RISING>;
    interrupt-names = "wdog", "fatal", "ready", "handover",
                      "stop-ack", "shutdown-ack";

    clocks = <&rpmhcc RPMH_CXO_CLK>;
    clock-names = "xo";

    power-domains = <&rpmhpd SC8180X_CX>,
                    <&rpmhpd SC8180X_MSS>;
    power-domain-names = "cx", "mss";

    qcom,qmp = <&aoss_qmp>;

    qcom,smem-states = <&modem_smp2p_out 0>;
    qcom,smem-state-names = "stop";

    glink-edge {
        interrupts = <GIC_SPI 449 IRQ_TYPE_EDGE_RISING>;
        label = "modem";
        qcom,remote-pid = <1>;
        mboxes = <&apss_shared 12>;
    };
};
```

Board override 766–771:

```
&remoteproc_mpss {
    status = "okay";

    memory-region = <&mpss_mem>;
    firmware-name = "qcom/samsung/w767/qcmpss8180_XEF.mbn";
};
```

Reserved region 48–51:

```
mpss_mem: memory@8d800000 {
    reg = <0x0 0x8d800000 0x0 0x0a000000>;
    no-map;
};
```

* Base `0x8d800000`, size `0x0a000000` (160 MiB).
* Firmware: `qcmpss8180_XEF.mbn` (78 520 448 bytes, ~75 MiB).
* Service-registry: `modemuw.jsn` (`domain: modem, subdomain: wlan_pd, qmi_instance_id: 180`, declares `elf_loader` + `wlan/fw` services — the Snapdragon X24 loads the WCN3990 firmware from within MPSS).
* Linux-boot status: **does not load** in iteration 16. MPSS is the largest
  PAS, it has the most regulator/clock requirements, and mainline support
  for `qcom,sc8180x-mpss-pas` is recent; firmware auth typically fails until
  QMP sub-system-handover and the "mss" rpmhpd are both satisfied. The
  sibling research notes already flag modem + audio cellular path as
  out-of-scope for first boot.
* Windows driver chain: `qcmbb8180.inf` → `mbb` service exposes
  `QCMS\VEN_QCOM&DEV_0489&SUBSYS_SSKU_AHP` as "Snapdragon (TM) X24 LTE
  Modem". A companion `qcmbrg8180.inf`/`qcrmnetbridge8180.inf` bridges MBIM
  traffic. Thermal is `QCOM04AF` "Qualcomm Modem Limiting Thermal Device".
* SIM tray: W767 has an internal nano-SIM slot; carrier-provisioning mcfg
  blobs (`mcfg_hw.mbn.1`, `mcfg_sw.mbn.5…14`) are bundled in the firmware
  tree for the X24 baseband.

### 2.4 SLPI — sensor low-power island

There is **no `remoteproc_slpi`** node in the current mainline `sc8180x.dtsi`
(see grep results — only ADSP, CDSP, MPSS are present in the
`remoteproc@…` search). The platform does have SLPI SMP2P plumbing:

```
# mainline-dts/sc8180x.dtsi, 759–779
smp2p-slpi {
    compatible = "qcom,smp2p";
    ...
    slpi_smp2p_out: master-kernel { ... };
    slpi_smp2p_in: slave-kernel { ... };
};
```

* Firmware is present as `qcslpi8180.mbn` (6 734 068 bytes) so the SLPI can
  be brought up once a `qcom,sc8180x-slpi-pas` node is added (the binding
  exists for sm8150 and sm8250; SC8180X expects the same layout).
* Hexagon image identity: `Qualcomm(R) Hexagon(TM) 690 DSP` — Windows
  publishes a second instance at `ACPI\QCOM048A\2&DABA3FF&0` bound to
  `qcadsprpcd`.
* Sensor-stack linkage: Windows uses `qcsensors8180.inf` +
  `qcsensorsconfigcls8180.inf` with SLPI as the compute target for BH1733
  light-sensor, hall sensor, cover-detect, etc. `sx9360grip.inf` is the
  Samsung grip-proximity driver sitting on a SLPI-routed I²C.
* Linux-boot status: **not yet configured**. Plan: add an `slpi_mem`
  reserved-memory region and a `remoteproc_slpi` PAS node modelled on
  `remoteproc_adsp`.

### 2.5 SMEM / SMP2P / AOSS plumbing shared by all four

`mainline-dts/sc8180x.dtsi` lines 632–637:

```
smem_mem: smem@86000000 {
    compatible = "qcom,smem";
    reg = <0x0 0x86000000 0x0 0x200000>;
    no-map;
    hwlocks = <&tcsr_mutex 3>;
};
```

* `aop_mem @ 0x85f00000` (128 KiB) — always-on processor image.
* `aop_cmd_db @ 0x85f20000` (128 KiB) — command DB for AOSS/RPMh.
* `xbl_mem @ 0x85d00000` (1.25 MiB) — eXtensible-BootLoader runtime.
* `hyp_mem @ 0x85700000` (6 MiB) — hypervisor heap.
* `gpu_mem: memory@98715000` (8 KiB) — Adreno zap-shader area.
* On-board additions in `sc8180x-samsung-w767.dts`: `rmtfs_mem @
  0x85500000` (2 MiB, client-id 1, VMID 15), `wlan_mem @ 0x8bc00000`
  (1.5 MiB), and an `scss_mem @ 0x9a000000` (20 MiB) placeholder.

`aoss_qmp` sits at `0x0c300000` as a mailbox client used by every PAS
subsystem to request power-collapse votes:

```
aoss_qmp: power-management@c300000 {
    compatible = "qcom,sc8180x-aoss-qmp", "qcom,aoss-qmp";
    reg = <0x0 0x0c300000 0x0 0x400>;
    interrupts = <GIC_SPI 389 IRQ_TYPE_EDGE_RISING>;
    mboxes = <&apss_shared 0>;
    #clock-cells = <0>;
};
```

---

## 3. Power / regulators

### 3.1 PMIC topology

The platform mounts **three** Qualcomm PMICs on the SPMI bus. From
`mainline-dts/sc8180x-pmics.dtsi`:

| SPMI USID | Compat          | Role                    |
|-----------|-----------------|-------------------------|
| `0x0`     | `qcom,pm8150`   | Primary PMIC A (`pmc8180_0`). Hosts PON, power-key, `pmc8180_temp@2400` temp-alarm, `pmc8180_adc@3100` VADC, ADC-TM, RTC, gpio@c000 (10 GPIOs) |
| `0x1`     | `qcom,pmc8180`  | PMIC A USID-1 half (regulator/LDO manager surface; no sub-nodes in dtsi) |
| `0x2`     | `qcom,smb2351`  | SMB2351 companion charger (disabled on Samsung W767 by `sc8180x-samsung-w767.dts`) |
| `0x4`     | `qcom,pm8150c`  | Companion PMIC C (`pmic@4`). Hosts PON (disabled), `pmc8180c_temp@2400`, `pmc8180c_adc@3100`, ADC-TM, `pmc8180c_gpios` (12 pins) |
| `0x5`     | `qcom,pmc8180c` | PMIC C USID-1 — carries `pmc8180c_lpg` PWM (WLED / backlight / RGB LEDs). |
| `0x6`     | `qcom,pm8150c`  | PMIC C alternate (disabled)  |
| `0x8`     | `qcom,pm8150`   | Extension PMIC E (`pmc8180_2`). Hosts its own gpio@c000 (10 pins) |
| `0xa`     | `qcom,smb2351`  | second SMB2351 — disabled on W767 |

On the Samsung W767 board, the second `smb2351` entries at USIDs 2 and 10
are explicitly disabled:

```
# dts-stage-v2/sc8180x-samsung-w767.dts, 773–780
&spmi_bus {
    pmic@2 {
        status = "disabled";
    };
    pmic@a {
        status = "disabled";
    };
};
```

### 3.2 Fixed / top-level board regulators

```
# dts-stage-v2/sc8180x-samsung-w767.dts, 92–110
vph_pwr: vph-pwr-regulator {
    compatible = "regulator-fixed";
    regulator-name = "vph_pwr";
    regulator-min-microvolt = <3700000>;
    regulator-max-microvolt = <3700000>;
};

vreg_s4a_1p8: pm8150-s4 {
    compatible = "regulator-fixed";
    regulator-name = "vreg_s4a_1p8";
    regulator-min-microvolt = <1800000>;
    regulator-max-microvolt = <1800000>;
    regulator-always-on;
    regulator-boot-on;
    vin-supply = <&vph_pwr>;
};
```

* `vph_pwr` — the system battery rail (3.7 V fixed, raw battery pass-through).
* `vreg_s4a_1p8` — the PM8150 SMPS4 that rests on always-on 1.8 V.

### 3.3 pmc8180-a (PMIC A) regulators

Summary table built from lines 114–224 of the board DTS. "Constraints not
in ACPI" means the mainline-generated Linaro draft could not determine the
voltage from the ACPI PEP dump, so the Linux node exposes the regulator
but leaves the window unconstrained. Supplies reflect the board-level
`vdd-*-supply = <...>` map above the individual rails.

| Node     | Label            | Min / Max µV            | Purpose (cross-ref to consumers)                               |
|----------|------------------|-------------------------|----------------------------------------------------------------|
| smps5    | `vreg_s5a_2p0`   | not constrained in DTS   | Shared with vdd-l7-l12-l14-l15; typical rail ~2.04 V           |
| ldo1     | (no label)       | 1.200 000 V fixed        | `vdd-l1-l8-l11` via vreg_s6c_1p35 — core-logic 1.2 V           |
| ldo2     | (no label)       | 2.800–2.850 V            | `vdd-l2-l10` off `vreg_bob` — 2.8 V peripheral rail            |
| ldo3     | —                | unconstrained, HPM mode  | `vdd-l3-l4-l5-l18` off vreg_s7c_0p93 — 0.9 V digital           |
| ldo5     | —                | unconstrained, HPM       | same group                                                     |
| ldo6     | —                | 1.200 V fixed, HPM       | `vdd-l6-l9` off vreg_s6c_1p35 — 1.2 V core                     |
| ldo7     | `vreg_l7a_1p8`   | 1.800 V fixed, HPM       | **WCN3990 vddxo** + BT vddxo; USB HS-phy vdd18 fallback        |
| ldo9     | `vreg_l9a_1p3`   | 1.304 V fixed, HPM       | **WCN3990 vdd-1.3-rfa**, BT vddrf                              |
| ldo10    | —                | 2.850–3.000 V, HPM       | `vdd-l2-l10` off BOB — SD / peripheral IO                      |
| ldo11    | —                | unconstrained, HPM       | `vdd-l1-l8-l11` off s6c                                         |
| ldo12    | `vreg_l12a_1p8`  | 1.800 V fixed, HPM       | **USB HS-phy vdda18** across all three QUSB2 PHYs              |
| ldo13    | —                | unconstrained, HPM       | `vdd-l13-l16-l17` off BOB                                       |
| ldo14    | —                | 1.800 V fixed            | `vdd-l7-l12-l14-l15` off s5a_2p0                               |
| ldo15    | —                | unconstrained, HPM       | same group                                                     |
| ldo16    | —                | 2.800 V fixed, HPM       | `vdd-l13-l16-l17` off BOB                                      |
| ldo17    | *disabled*       | (ENOTRECOVERABLE — comment in DTS) | "ldo17 causes failed to get current voltage" — left off in iter16 |
| ldo18    | —                | unconstrained, HPM       | `vdd-l3-l4-l5-l18`                                             |

Board-supply map (quoted verbatim):

```
# dts-stage-v2/sc8180x-samsung-w767.dts, 114–124
pmc8180-a-rpmh-regulators {
    compatible = "qcom,pmc8180-rpmh-regulators";
    qcom,pmic-id = "a";

    vdd-s5-supply = <&vph_pwr>;
    vdd-l1-l8-l11-supply = <&vreg_s6c_1p35>;
    vdd-l2-l10-supply = <&vreg_bob>;
    vdd-l3-l4-l5-l18 = <&vreg_s7c_0p93>;
    vdd-l6-l9-supply = <&vreg_s6c_1p35>;
    vdd-l7-l12-l14-l15-supply = <&vreg_s5a_2p0>;
    vdd-l13-l16-l17-supply = <&vreg_bob>;
    ...
};
```

### 3.4 pmc8180-c (companion PMIC) regulators

| Node      | Label             | Min / Max µV              | Purpose                                                            |
|-----------|-------------------|---------------------------|--------------------------------------------------------------------|
| smps6     | `vreg_s6c_1p35`   | unconstrained, HPM        | 1.35 V buck — supplies to a-l1/l8/l11/l6/l9, c-l2/l3               |
| smps7     | `vreg_s7c_0p93`   | unconstrained, HPM        | 0.93 V buck — logic rail for a-l3/l4/l5/l18                        |
| smps8     | —                 | 1.800 V fixed, HPM        | 1.8 V buck                                                         |
| ldo1      | —                 | unconstrained, HPM        | off `vreg_s4a_1p8`                                                 |
| ldo2      | —                 | 1.200 V fixed, HPM        | off `vreg_s6c_1p35`                                                |
| ldo3      | `vreg_l3c_1p2`    | 1.200 V fixed, HPM        | UFS PHY vdda-pll, USB QMP vdda-phy                                 |
| ldo4      | *disabled*        | (touchscreen) — "causes -ENOTRECOVERABLE" — commented out in iter16 |
| ldo6      | —                 | 1.8–2.95 V                | "sd card vqmmc?" per DTS comment                                   |
| ldo7      | —                 | 3.000 V fixed, HPM        | off `vreg_bob` — BH1733 light sensor candidate                     |
| ldo8      | —                 | 1.800 V fixed             | off `vreg_s4a_1p8`                                                 |
| ldo9      | —                 | 2.504–2.904 V             | off `vreg_bob`                                                     |
| ldo10     | —                 | unconstrained, HPM        | off `vreg_bob`                                                     |
| ldo11     | `vreg_l11c_3p3`   | 3.312 V fixed             | **WCN3990 vdd-3.3-ch0**, BT vddch0                                 |
| bob       | `vreg_bob`        | unconstrained, HPM        | 3.6–3.9 V boost (battery-bypass regulator)                         |

Board-supply-map extract (225–239):

```
pmc8180c-rpmh-regulators {
    compatible = "qcom,pmc8180c-rpmh-regulators";
    qcom,pmic-id = "c";

    vdd-s6-supply = <&vph_pwr>;
    vdd-s7-supply = <&vph_pwr>;
    vdd-s8-supply = <&vph_pwr>; // ?
    vdd-l1-l8-supply = <&vreg_s4a_1p8>;
    vdd-l2-l3-supply = <&vreg_s6c_1p35>;
    vdd-l4-l5-l6-supply = <&vreg_bob>;
    vdd-l7-l11-supply = <&vreg_bob>;
    vdd-l9-l10 = <&vreg_bob>;
    vdd-bob-supply = <&vph_pwr>;
    ...
};
```

PMIC-C also carries the WLED (display backlight) behind `pmc8180c_lpg`
PWM (see sibling Display doc for how the panel-backlight endpoint wires
to `pmc8180c_lpg` channel 4).

### 3.5 pmc8180-e (extension PMIC) regulators

This is a second PM8150 die used primarily for DDR/fabric rails, USB3.3V
analog, and UFS analog supplies.

| Node      | Label            | Min / Max µV              | Purpose                                                    |
|-----------|------------------|---------------------------|------------------------------------------------------------|
| smps4     | `vreg_s4e_0p98`  | unconstrained, HPM        | ~0.98 V buck supplying all e-l1/l3/l4/l5/l6/l8/l9/l11/l18  |
| smps5     | `vreg_s5e_2p05`  | unconstrained, HPM        | ~2.05 V buck supplying e-l7/l12/l14/l15                    |
| ldo1      | `vreg_l1e_0p75`  | 0.752 V fixed, HPM        | **WCN3990 vdd-0.8-cx-mx** (0.75 V CX/MX)                   |
| ldo2      | —                | unconstrained, HPM        | `vdd-l2-l10` from BOB                                      |
| ldo4      | —                | unconstrained, HPM        | `vdd-l3-l4-l5-l18`                                         |
| ldo5      | `vreg_l5e_0p88`  | 0.880 V fixed, HPM        | **UFS PHY vdda-phy**, USB QMP vdda-pll, USB HS vdda-pll    |
| ldo7      | `vreg_l7e_1p8`   | 1.800 V fixed, HPM        | **UFS vccq2** (I/O 1.8 V)                                  |
| ldo8      | —                | unconstrained, HPM        | same group                                                 |
| ldo9      | `vreg_l9e_0p88`  | 0.880–0.912 V, HPM        | another 0.88 V rail                                        |
| ldo10     | `vreg_l10e_2p9`  | 2.504–2.904 V, HPM        | **UFS vcc** (core)                                         |
| ldo12     | `vreg_l12e_1p8`  | 1.800 V fixed, HPM        | touchscreen vddl                                           |
| ldo13     | —                | unconstrained, HPM        |                                                            |
| ldo14     | —                | 1.800 V fixed, HPM        |                                                            |
| ldo15     | —                | 1.800 V fixed, HPM        |                                                            |
| ldo16     | `vreg_l16e_3p0`  | 3.072 V fixed, HPM        | **USB HS-PHY vdda33** for all three QUSB2                  |
| ldo17     | —                | 2.960 V fixed, HPM        |                                                            |

Board supply map (322–334):

```
pmc8180-e-rpmh-regulators {
    compatible = "qcom,pmc8180-rpmh-regulators";
    qcom,pmic-id = "e";

    vdd-s4-supply = <&vph_pwr>;
    vdd-s5-supply = <&vph_pwr>;
    vdd-l1-l8-l11-supply = <&vreg_s4e_0p98>;
    vdd-l2-l10-supply = <&vreg_bob>;
    vdd-l3-l4-l5-l18-supply = <&vreg_s4e_0p98>;
    vdd-l6-l9-supply = <&vreg_s4e_0p98>;
    vdd-l7-l12-l14-l15-supply = <&vreg_s5e_2p05>;
    vdd-l13-l16-l17-supply = <&vreg_bob>;
    ...
};
```

### 3.6 Supply graph

```
vph_pwr (3.7 V, battery) ───┬─► vreg_bob (BOB, boost, ~3.6–3.9 V)
                            │      ├──► a-ldo2 / a-ldo10 / a-ldo13 / a-ldo16 / a-ldo17
                            │      ├──► c-ldo4..11  (incl. vreg_l11c_3p3 → WCN3990 3.3 V)
                            │      ├──► e-ldo2 / e-ldo10 / e-ldo13 / e-ldo16 / e-ldo17
                            │      └──► vreg_bob rail is itself fed from vph_pwr
                            │
                            ├─► PM8150-A s5 → vreg_s5a_2p0 (≈2.0 V)
                            │      └──► a-ldo7, a-ldo12, a-ldo14, a-ldo15
                            │            ├── a-ldo7  = vreg_l7a_1p8 → WCN3990 vddxo, BT vddxo
                            │            └── a-ldo12 = vreg_l12a_1p8 → USB HS-phy vdda18 x3
                            │
                            ├─► PM8150-C s6 → vreg_s6c_1p35 (≈1.35 V)
                            │      └──► a-ldo1, a-ldo6, a-ldo8, a-ldo9, a-ldo11, c-ldo2, c-ldo3
                            │            └── a-ldo9  = vreg_l9a_1p3 → WCN3990 vdd-1.3-rfa
                            │            └── c-ldo3  = vreg_l3c_1p2 → UFS vdda-pll / USB QMP vdda-phy
                            │
                            ├─► PM8150-C s7 → vreg_s7c_0p93 (≈0.93 V)
                            │      └──► a-ldo3, a-ldo4, a-ldo5, a-ldo18 (low-voltage digital)
                            │
                            ├─► PM8150-C s8 (1.8 V)
                            │
                            ├─► PM8150-A s4 (always-on) → vreg_s4a_1p8 (1.8 V)
                            │      └──► c-ldo1, c-ldo8 (PMIC-local 1.8 V LDOs)
                            │
                            ├─► PM8150-E s4 → vreg_s4e_0p98 (≈0.98 V)
                            │      └──► e-ldo1 (= vreg_l1e_0p75 → WCN3990 0.8 V CX/MX)
                            │      └──► e-ldo3..6, e-ldo8, e-ldo9, e-ldo11
                            │            └── e-ldo5 = vreg_l5e_0p88 → UFS vdda-phy, USB QMP vdda-pll
                            │
                            ├─► PM8150-E s5 → vreg_s5e_2p05 (≈2.05 V)
                            │      └──► e-ldo7 (= vreg_l7e_1p8 → UFS vccq2 I/O 1.8 V)
                            │      └──► e-ldo12 (= vreg_l12e_1p8 → touch vddl)
                            │      └──► e-ldo14, e-ldo15
                            │
                            └─► PMIC-C bob → vreg_bob (see above)
```

### 3.7 Power-domain controllers

* `rpmhpd` (from `apps_rsc` at `0x18200000`): power levels `RETENTION /
  MIN_SVS / LOW_SVS / SVS / SVS_L1 / NOM / NOM_L1 / NOM_L2 / TURBO /
  TURBO_L1`. Named domains: `SC8180X_CX`, `SC8180X_MSS`, `SC8180X_MMCX`.
* `gcc` (at `0x100000`) exposes GDSCs consumed by peripherals, as observed
  in the grep of the dtsi:

| GDSC handle              | Consumers                                             |
|--------------------------|-------------------------------------------------------|
| `UFS_PHY_GDSC`           | `ufs_mem_hc`, `ufs_mem_phy`                           |
| `USB30_PRIM_GDSC`        | `usb_prim` DWC3                                       |
| `USB30_SEC_GDSC`         | `usb_sec` DWC3                                        |
| `USB30_MP_GDSC`          | `usb_mp` multi-port DWC3 (x2 child nodes)             |
| `PCIE_0_GDSC`            | `pcie0 @ 0x01c00000`                                  |
| `PCIE_1_GDSC`            | `pcie1 @ 0x01c10000`                                  |
| `PCIE_2_GDSC`            | `pcie2 @ 0x01c18000`                                  |
| `PCIE_3_GDSC`            | `pcie3 @ 0x01c08000`                                  |

* `gpucc` exposes `GPU_CX_GDSC` and `GPU_GX_GDSC` (consumed by `gpu@2c00000`
  + GMU and Adreno SMMU).
* `dispcc` exposes `MDSS_GDSC` (consumed by `mdss @ 0xae00000`).
* `videocc` (node present at `0xab00000`) would expose the video-codec
  GDSCs once enabled.

---

## 4. Thermal subsystem

### 4.1 TSENS sensor blocks

```
# mainline-dts/sc8180x.dtsi, 3564–3584
tsens0: thermal-sensor@c263000 {
    compatible = "qcom,sc8180x-tsens", "qcom,tsens-v2";
    reg = <0 0x0c263000 0 0x1ff>, /* TM */
          <0 0x0c222000 0 0x1ff>; /* SROT */
    #qcom,sensors = <16>;
    interrupts = <GIC_SPI 506 ...>, <GIC_SPI 508 ...>;
    interrupt-names = "uplow", "critical";
    #thermal-sensor-cells = <1>;
};

tsens1: thermal-sensor@c265000 {
    compatible = "qcom,sc8180x-tsens", "qcom,tsens-v2";
    reg = <0 0x0c265000 0 0x1ff>, <0 0x0c223000 0 0x1ff>;
    #qcom,sensors = <9>;
    interrupts = <GIC_SPI 507 ...>, <GIC_SPI 509 ...>;
    interrupt-names = "uplow", "critical";
    #thermal-sensor-cells = <1>;
};
```

Total 25 on-die sensors. PMIC A and PMIC C each additionally expose a
single die-level temp alarm (`qcom,spmi-temp-alarm`) at SPMI `0x2400`,
with its own ADC channel `ADC5_DIE_TEMP` and the following passive / hot
/ critical trip points:

```
# mainline-dts/sc8180x-pmics.dtsi, 13–65
pmc8180-thermal:   passive 95 °C, hot 115 °C, critical 145 °C
pmc8180c-thermal:  passive 95 °C, hot 115 °C, critical 145 °C
```

### 4.2 Mainline thermal zones

From the bottom of `sc8180x.dtsi` (lines 3993–4394): 25 zones total. Key
groups:

| Zone name          | TSENS sensor           | Trips                                   |
|--------------------|------------------------|-----------------------------------------|
| `cpu0…cpu3-thermal` | `tsens0 1..4`           | critical 110 °C, hyst 1000             |
| `cpu4-top/…bottom`  | `tsens0 7..14`          | critical 110 °C                        |
| `aoss0-thermal`     | `tsens0 0`              | hot 90 °C                              |
| `cluster0-thermal`  | `tsens0 5`              | critical 110 °C                        |
| `cluster1-thermal`  | `tsens0 6`              | critical 110 °C                        |
| `gpu-top-thermal`   | `tsens0 15` + cooling-map → `&gpu` | passive 85 °C, hot 90 °C, critical 110 °C |
| `aoss1-thermal`     | `tsens1 0`              | hot 90 °C                              |
| `wlan-thermal`      | `tsens1 1`              | hot 90 °C                              |
| `video-thermal`     | `tsens1 2`              | hot 90 °C                              |
| `mem-thermal`       | `tsens1 3`              | hot 90 °C                              |
| `q6-hvx-thermal`    | `tsens1 4`              | hot 90 °C (CDSP HVX)                   |
| `camera-thermal`    | `tsens1 5`              | hot 90 °C                              |
| `compute-thermal`   | `tsens1 6`              | hot 90 °C                              |
| `mdm-dsp-thermal`   | `tsens1 7`              | hot 90 °C                              |
| `npu-thermal`       | `tsens1 8`              | hot 90 °C                              |
| `gpu-bottom-thermal`| `tsens1 11` + cooling-map → `&gpu` | passive 85 °C, hot 90 °C, critical 110 °C |

Plus two PMIC zones `pmc8180-thermal` and `pmc8180c-thermal` driven from
the on-die die-temp ADC channels.

Two LMh (Limits Management hardware) blocks hang off cpufreq:

```
lmh@18350800  (cpus = <&cpu4>, big cluster):
    qcom,lmh-temp-arm-millicelsius  = 65000
    qcom,lmh-temp-low-millicelsius  = 94500
    qcom,lmh-temp-high-millicelsius = 95000

lmh@18358800  (cpus = <&cpu0>, little cluster):
    (same trip temperatures)
```

### 4.3 ACPI thermal-zone namespace (Windows view)

From `acpi-decompile/dsdt.dsl` the DSDT declares a large bank of
`ThermalZone()` objects, each exposing `_PSV` (passive), `_CRT`
(critical), `_TC1`/`_TC2` thermal constants, `_TSP` sampling period,
and `_DEP` (dependencies). HIDs in the range `QCOM045C..QCOM046B`
represent per-domain aggregated thermal zones, and HIDs
`QCOM04C0..QCOM04CB` are PMIC/subsystem sub-zones. Summarised map:

| ACPI HID | TZxx              | `_TZD` target(s)                                   | Role                                       |
|----------|-------------------|----------------------------------------------------|--------------------------------------------|
| QCOM045C | TZ0 (UID 0)       | SYSM.CLUS.CPU0..3                                  | Little-cluster CPU aggregate               |
| QCOM045C | TZ1 (UID 1)       | PEP0                                               | Little-cluster throttle zone (passive 3580 = 86 °C) |
| QCOM045D | TZ2/TZ3           | big-cluster CPU0..3 / PEP0                         | Big-cluster CPU aggregate / throttle       |
| QCOM045E | —                 | (UID 64) reported by WMI                           | Camera die                                 |
| QCOM045F | —                 | (UID 1) battery related (PMIC-A via PSUB alias)    | Battery gauge proxy                        |
| QCOM0450 | TZ7               | GPU0.AVS0                                          | GPU AVS (adaptive voltage)                 |
| QCOM049D | TZ5               | GPU0                                               | GPU aggregated                             |
| QCOM049E | TZ9               | AMSS + MPTM                                        | Modem / modem-thermal-mgr                  |
| QCOM044C | TZ33              | SCSS                                               | Subsystem/SCSS                             |
| QCOM0462 | TZ15/TZ16         | CPU0..7 + PMBM / PEP0                              | System-wide skin / battery temperature     |
| QCOM0463 | TZ40              | CPU0..7                                            | Backlight / skin sense ch. 1               |
| QCOM0465 | TZ18/TZ19         | CPU0..7 + PMBM / PEP0                              | Additional skin sense (UID 0/1)            |
| QCOM0466 | TZ41              | CPU0..7                                            | Skin ch. 2                                 |
| QCOM0467 | TZ(…)             | CPU0..7                                            | Skin ch. 3                                 |
| QCOM0469 | TZ20/TZ21         | CPU0..7 + PMBM / PEP0                              | Skin aggregate                             |
| QCOM046A | TZ(…)             | CPU0..7                                            | Skin ch. 4                                 |
| QCOM046B | TZ99              | CPU0..7                                            | Skin ch. 5                                 |
| QCOM04C0..04CB | TZ51..TZ62 | MPA / MPA1 / MBJ0 etc.                             | PMIC / modem / millimetre sub-zones        |

Live readings sampled from `win-extract/acpi_thermal.txt` (WMI
MSAcpi_ThermalZoneTemperature values are tenths of Kelvin, i.e. 3135
= 313.5 K = 40.5 °C):

```
ACPI\QCOM045C\0_0 (CPU little aggregate) ->    CurrentTemperature 3135 (40.5 °C)
ACPI\QCOM045C\1_0 (CPU little throttle)   ->   PassiveTripPoint 3580 (84.9 °C)
ACPI\QCOM045D\1_0 (CPU big throttle)      ->   PassiveTripPoint 3580 (84.9 °C)
ACPI\QCOM049D\0_0 (GPU)                   ->   PassiveTripPoint 3580 (84.9 °C)
ACPI\QCOM0450\0_0 (GPU AVS)               ->   PassiveTripPoint 3680 (94.9 °C)
ACPI\QCOM049E\0_0 (modem)                 ->   PassiveTripPoint 3680 (94.9 °C)
ACPI\QCOM044C\0_0 (SCSS/subsystem)        ->   PassiveTripPoint 3580 (84.9 °C)
ACPI\QCOM045E\64_0 (camera UID 64)        ->   PassiveTripPoint 3780 (104.9 °C), Critical 3880 (114.9 °C)
ACPI\QCOM0462\1_0 (skin big-cluster)      ->   Passive 3980 (124.9 °C!), Critical 4180 (144.9 °C)
ACPI\QCOM0465\1_0 (skin sense aux)        ->   Passive 3980, Critical 4180
ACPI\QCOM0469\1_0 (skin aggregate)        ->   Passive 3980, Critical 4180
```

Note the asymmetric scale — the "skin" zones sit above 100 °C because
they model virtual-skin temperature, not absolute-die temperature.

### 4.4 Cooling devices and fans

**This is a fanless design.** There is no active cooling device in
either DSDT (no `_AC0..9` fan declarations are emitted) or in the
mainline DTS (no `gpio-fan` / `pwm-fan` node). Passive throttling
is driven entirely by LMh + `qcom,sc8180x-cpufreq-hw` and GPU thermal
cooling-maps (`&gpu THERMAL_NO_LIMIT THERMAL_NO_LIMIT`).

---

## 5. Sleep states and power management

From `win-extract/sleepstates.txt` (verbatim):

```
The following sleep states are available on this system:
    Standby (S0 Low Power Idle) Network Connected
    Hibernate

The following sleep states are not available on this system:
    Standby (S1)
        The system firmware does not support this standby state.
    Standby (S2)
        The system firmware does not support this standby state.
    Standby (S3)
        The system firmware does not support this standby state.
    Hybrid Sleep
        Standby (S3) is not available.
        The hypervisor does not support this standby state.
    Fast Startup
        This action is disabled in the current system policy.
```

* **S0 / Modern Standby ("S0 Low Power Idle, Network Connected")** is the
  only standby path. The platform implements "Connected Standby" — the
  ADSP, WCN3990 and MPSS can maintain radio connectivity while the APSS
  cluster enters `cluster_sleep_aoss_sleep`.
* **S3 is not supported.** This is typical for Windows-on-ARM Qualcomm
  laptops — the firmware does not implement S3.
* **Hibernate (S4)** is enabled.
* Disabling hybrid sleep is a direct consequence of the missing S3.
* From `acpi-decompile/facp.dsl`: power-management via the FACP uses the
  generic PSCI-based sleep path (no legacy SLP_EN/SLP_TYP register).
* Linux cpuidle uses the PSCI domain-idle-states declared in the dtsi
  (`little_cpu_sleep_0`, `big_cpu_sleep_0`, `cluster_sleep_apss_off`,
  `cluster_sleep_aoss_sleep`).

`powercfg.txt` highlights (shortened; full file: `win-extract/powercfg.txt`):

```
Power Scheme GUID: 381b4222-f694-41f0-9685-ff5bb260df2e  (Balanced)
Sleep after:           AC 300 s, DC 180 s
Hibernate after:       AC 0,      DC 0 (disabled by idle)
Hybrid sleep:          AC enabled, DC enabled (but only hibernate engages)
Display off after:     AC 300 s, DC 180 s
Critical battery:      2 %  -> Hibernate
Low battery:           6 %  -> nothing
Reserve battery:       4 %
Adaptive brightness:   off
```

### 5.1 Linux cpuidle model

```
# mainline-dts/sc8180x.dtsi, 543–599
psci {
    compatible = "arm,psci-1.0";
    method = "smc";

    cpu_pd0..cpu_pd3 { power-domains = <&cluster_pd>; domain-idle-states = <&little_cpu_sleep_0>; };
    cpu_pd4..cpu_pd7 { power-domains = <&cluster_pd>; domain-idle-states = <&big_cpu_sleep_0>; };
    cluster_pd { domain-idle-states = <&cluster_sleep_apss_off &cluster_sleep_aoss_sleep>; };
};
```

* `cluster_sleep_apss_off` = APSS cluster power-collapse (L3 retention).
* `cluster_sleep_aoss_sleep` = deeper whole-AOSS sleep — this is the state
  entered for Modern Standby when all subsystems have voted retention via
  RPMh.

---

## 6. Clock controllers

Addresses quoted from `mainline-dts/sc8180x.dtsi`:

| Controller  | Node                         | MMIO base   | Compat                                           |
|-------------|------------------------------|-------------|--------------------------------------------------|
| GCC         | `clock-controller@100000`    | `0x100000`  | `qcom,gcc-sc8180x`                               |
| GPU CC      | `clock-controller@2c90000`   | `0x2c90000` | `qcom,sc8180x-gpucc`                             |
| Video CC    | `clock-controller@ab00000`   | `0x0ab00000`| `qcom,sc8180x-videocc`, `qcom,sm8150-videocc`    |
| Camera CC   | `clock-controller@ad00000`   | `0x0ad00000`| `qcom,sc8180x-camcc`                             |
| Display CC  | `clock-controller@af00000`   | `0x0af00000`| `qcom,sc8180x-dispcc`                            |
| RPMh CC     | child of `apps_rsc@18200000` | N/A (RSC)   | `qcom,sc8180x-rpmh-clk`                          |
| OSM L3      | `interconnect@18321000`      | `0x18321000`| `qcom,sc8180x-osm-l3`, `qcom,osm-l3`             |
| Cpufreq HW  | `cpufreq@18323000`           | `0x18323000`| `qcom,sc8180x-cpufreq-hw`, `qcom,cpufreq-hw`     |

GCC input sources (`sc8180x.dtsi` 796–802):

```
gcc: clock-controller@100000 {
    compatible = "qcom,gcc-sc8180x";
    reg = <0x0 0x00100000 0x0 0x1f0000>;
    #clock-cells = <1>; #reset-cells = <1>; #power-domain-cells = <1>;
    clocks = <&rpmhcc RPMH_CXO_CLK>,
             <&rpmhcc RPMH_CXO_CLK_A>,
             <&sleep_clk>;
    clock-names = "bi_tcxo", "bi_tcxo_ao", "sleep_clk";
    power-domains = <&rpmhpd SC8180X_CX>;
};
```

### 6.1 Major clock consumers and paths

* **Display** → `dispcc` (`af00000`). Input tree (from lines 3523–3552):
  * `bi_tcxo` from `rpmhcc RPMH_CXO_CLK`
  * `dsi0_phy_pll_out_byteclk`, `dsi0_phy_pll_out_dsiclk`
  * `dsi1_phy_pll_out_byteclk`, `dsi1_phy_pll_out_dsiclk`
  * `dp_phy_pll_link_clk`, `dp_phy_pll_vco_div_clk`
  * `edp_phy_pll_link_clk`, `edp_phy_pll_vco_div_clk`
  * `dptx1_phy_pll_link_clk`, `dptx1_phy_pll_vco_div_clk`
  * `power-domains = <&rpmhpd SC8180X_MMCX>;` with `required-opps = <&rpmhpd_opp_low_svs>`.
  Used by MDSS / eDP (detail in sibling display doc).

* **GPU (Adreno 680.1)** → `gpucc` (`2c90000`). Inputs: `bi_tcxo`,
  `GCC_GPU_GPLL0_CLK_SRC`, `GCC_GPU_GPLL0_DIV_CLK_SRC`. GDSCs:
  `GPU_CX_GDSC`, `GPU_GX_GDSC`. GMU opp table declared in-line.

* **UFS (UFS 3.0 host + QMP-UFS phy)** → 8 GCC clocks:

```
clocks = <&gcc GCC_UFS_PHY_AXI_CLK>,
         <&gcc GCC_AGGRE_UFS_PHY_AXI_CLK>,
         <&gcc GCC_UFS_PHY_AHB_CLK>,
         <&gcc GCC_UFS_PHY_UNIPRO_CORE_CLK>,
         <&rpmhcc RPMH_CXO_CLK>,
         <&gcc GCC_UFS_PHY_TX_SYMBOL_0_CLK>,
         <&gcc GCC_UFS_PHY_RX_SYMBOL_0_CLK>,
         <&gcc GCC_UFS_PHY_RX_SYMBOL_1_CLK>;
```

* **USB3 primary** → `GCC_CFG_NOC_USB3_PRIM_AXI_CLK`,
  `GCC_USB30_PRIM_MASTER_CLK`, `…PRIM_SLEEP_CLK`, `…PRIM_MOCK_UTMI_CLK`
  and PIPE clk from `usb_prim_qmpphy`. Similar for USB3 secondary and
  multi-port.

* **WiFi (WCN3990)** uses `RPMH_RF_CLK2` for its 38.4-MHz sleep XO pin.

---

## 7. Interconnects (NoC fabric)

Seven provider blocks, all using `qcom,bcm-voters = <&apps_bcm_voter>`
(from `apps_rsc`):

| Provider          | Node                                | MMIO base        | Role                                         |
|-------------------|--------------------------------------|------------------|----------------------------------------------|
| `config_noc`      | `interconnect@1500000`              | `0x01500000`     | Configuration / register-access fabric       |
| `system_noc`      | `interconnect@1620000`              | `0x01620000`     | System fabric for most cpu-peripheral traffic|
| `aggre1_noc`      | `interconnect@16e0000`              | `0x016e0000`     | Aggregator #1 (USB3-multiport, UFS traffic)  |
| `aggre2_noc`      | `interconnect@1700000`              | `0x01700000`     | Aggregator #2                                |
| `compute_noc`     | `interconnect@1720000`              | `0x01720000`     | CDSP / compute traffic                       |
| `mmss_noc`        | `interconnect@1740000`              | `0x01740000`     | Multimedia (MDP, Video, Camera)              |
| `gem_noc`         | `interconnect@9680000`              | `0x09680000`     | GEM (Generic Embedded Memory) / cache fabric |
| `osm_l3`          | `interconnect@18321000`             | `0x18321000`     | OSM L3 cache interconnect                    |

In addition there are three virtual providers declared at root level
(`camnoc_virt`, `mc_virt`, `qup_virt`) used purely for bandwidth
accounting.

### 7.1 Major paths

Sampled from `sc8180x.dtsi`:

```
/* every CPU -> gem_noc -> DDR */
interconnects = <&gem_noc MASTER_AMPSS_M0 3 &mc_virt SLAVE_EBI_CH0 3>,
                <&gem_noc MASTER_AMPSS_M0 3 &config_noc SLAVE_EBI_CH0 3>;

/* UFS */
interconnects = <&aggre1_noc MASTER_UFS_MEM QCOM_ICC_TAG_ALWAYS
                  &mc_virt SLAVE_EBI_CH0 QCOM_ICC_TAG_ALWAYS>,
                <&gem_noc MASTER_AMPSS_M0 QCOM_ICC_TAG_ALWAYS
                  &config_noc SLAVE_UFS_MEM_0_CFG QCOM_ICC_TAG_ALWAYS>;
interconnect-names = "ufs-ddr", "cpu-ufs";

/* Display (MDSS, from MDP ports) */
interconnects = <&mmss_noc MASTER_MDP_PORT0 QCOM_ICC_TAG_ALWAYS
                  &mc_virt SLAVE_EBI_CH0 QCOM_ICC_TAG_ALWAYS>,
                <&mmss_noc MASTER_MDP_PORT1 QCOM_ICC_TAG_ALWAYS
                  &mc_virt SLAVE_EBI_CH0 QCOM_ICC_TAG_ALWAYS>,
                <&gem_noc MASTER_AMPSS_M0 QCOM_ICC_TAG_ALWAYS
                  &config_noc SLAVE_DISPLAY_CFG QCOM_ICC_TAG_ALWAYS>;
interconnect-names = "mdp0-mem", "mdp1-mem", "cpu-cfg";

/* USB3-mp */
interconnects = <&aggre1_noc MASTER_USB3_2 0 &mc_virt SLAVE_EBI_CH0 0>,
                <&gem_noc MASTER_AMPSS_M0 0 &config_noc SLAVE_USB3_2 0>;

/* Every QUPv3 SE */
interconnects = <&qup_virt MASTER_QUP_CORE_0 0 &qup_virt SLAVE_QUP_CORE_0 0>,
                <&gem_noc MASTER_AMPSS_M0 0 &config_noc SLAVE_QUP_0 0>,
                <&aggre1_noc MASTER_QUP_0 0 &mc_virt SLAVE_EBI_CH0 0>;
interconnect-names = "qup-core", "qup-config", "qup-memory";
```

Every enabled QUPv3 (I²C, SPI, UART) in the board DTS implicitly pulls
its `qup-core / qup-config / qup-memory` votes via the above definition.

BCM voter sits inside `apps_rsc` → `bcm-voter`, i.e. final QoS votes
are made against RPMh.

---

## 8. IOMMU / SMMU

Two MMU-500 instances — `adreno_smmu` (GPU-private) and `apps_smmu`
(everything else).

### 8.1 `adreno_smmu`

```
# mainline-dts/sc8180x.dtsi, 2384–2405
adreno_smmu: iommu@2ca0000 {
    compatible = "qcom,sc8180x-smmu-500", "qcom,adreno-smmu",
                 "qcom,smmu-500", "arm,mmu-500";
    reg = <0 0x02ca0000 0 0x10000>;
    #iommu-cells = <2>;
    #global-interrupts = <1>;
    interrupts = <GIC_SPI 674..688>; /* 9 interrupts */
    clocks = <&gpucc GPU_CC_AHB_CLK>,
             <&gcc GCC_GPU_MEMNOC_GFX_CLK>,
             <&gcc GCC_GPU_SNOC_DVM_GFX_CLK>;
    clock-names = "ahb", "bus", "iface";
    power-domains = <&gpucc GPU_CX_GDSC>;
};
```

SID streams allocated:
* GPU (`adreno-680.1` @ 0x2c00000) → `<&adreno_smmu 0 0xc01>`
* GMU (@ adreno-gmu-680) → `<&adreno_smmu 5 0xc00>`

### 8.2 `apps_smmu`

```
# mainline-dts/sc8180x.dtsi, 3618–3730
apps_smmu: iommu@15000000 {
    compatible = "qcom,sc8180x-smmu-500", "arm,mmu-500";
    reg = <0 0x15000000 0 0x100000>;
    #iommu-cells = <2>;
    #global-interrupts = <1>;
    interrupts = <GIC_SPI 65>, 31 x GIC_SPI (97..118, 181..192),
                 31 x GIC_SPI (315..345, 395..413),
                 10 x GIC_SPI (706..715), 4 x GIC_SPI (640..643),
                 8 x GIC_SPI (768..775);
    dma-coherent;
};
```

* 128 context-banks.
* The listed ~95 SPIs route into each context bank's individual interrupt.

SID streams observed in the dtsi:

| Consumer                         | SID mapping                         |
|----------------------------------|--------------------------------------|
| `qupv3_id_0 @ 0x008c0000`        | `<&apps_smmu 0x4c3 0>`              |
| `qupv3_id_1 @ 0x00ac0000`        | `<&apps_smmu 0x603 0>`              |
| `qupv3_id_2 @ 0x00c80000`        | `<&apps_smmu 0x7a3 0>`              |
| `ufs_mem_hc @ 0x01d84000`        | `<&apps_smmu 0x300 0>`              |
| `usb_prim` dwc3                  | `<&apps_smmu 0x60 0>`               |
| `usb_sec` dwc3                   | `<&apps_smmu 0x140 0>`              |
| `usb_mp` dwc3                    | `<&apps_smmu 0x160 0>`              |
| `mdss @ 0xae00000`               | `<&apps_smmu 0x800 0x420>`          |
| `wifi @ 0x18800000` (WCN3990)    | `<&apps_smmu 0x0640 0x1>`           |

The multi-port USB also has per-root sub-streams, and the Video/Camera
CCs bring their own sets when enabled.

IORT cross-check (`acpi-decompile/iort.dsl`): IORT declares **two
SMMU-3 nodes** (Type 03) at addresses `0x15000000` (span `0x100000`) and
`0x02CA0000`, matching apps_smmu and adreno_smmu one-to-one.

---

## 9. Firmware blobs

All quoted firmware lives under
`/home/peter/Documents/GalaxyBookS_Linux/firmware-stage/lib/firmware/qcom/samsung/w767/`
(mirrors the kernel `firmware-name = "qcom/samsung/w767/…"` path used in
the Samsung DTS).

| File                        | Size (bytes) | Role                                                                     |
|-----------------------------|--------------|--------------------------------------------------------------------------|
| `qcadsp8180.mbn`            | 11 008 656   | aDSP Hexagon image — signed PAS ELF for `remoteproc_adsp` (audio, AVS)   |
| `qccdsp8180.mbn`            | 3 114 644    | cDSP Hexagon image — signed PAS ELF for `remoteproc_cdsp` (HVX, listen)  |
| `qcmpss8180_XEF.mbn`        | 78 520 448   | MPSS modem image — X24 LTE firmware; `_XEF` suffix = export variant      |
| `qcslpi8180.mbn`            | 6 734 068    | SLPI Hexagon image — sensor island firmware                              |
| `qcdxkmsuc8180.mbn`         | 14 240       | Adreno "zap shader" (microcode that asserts GPU TZ/NS crossings)          |
| `qcvss8180.mbn`             | 1 159 200    | Video subsystem image (video-cc / Venus)                                  |
| `qcwdsp8180.mbn`            | 2 140 911    | Wireless DSP firmware (WCN integration)                                   |
| `wlanmdsp.mbn`              | 4 067 104    | **WCN3990 WLAN/BT firmware** — executed inside MPSS wlan_pd              |
| `pr_3_wp.mbn`               | 475 898      | Programmable-resource / WP-3 blob (boot-time PMIC programming)            |
| `hdcp1.mbn`                 | 43 647       | HDCP 1.x content-protection app                                           |
| `hdcp2p2.mbn`               | 114 665      | HDCP 2.2 TA (TrustZone app)                                               |
| `hdcpsrm.mbn`               | 38 625       | HDCP SRM (system-renewability message) blob                               |
| `dxhdcp2.mbn`               | 371 992      | DirectX / GFX-side HDCP2 TA                                               |
| `mcfg_hw.mbn.1`             | 28 576       | X24 carrier hardware-tuning blob (single entry)                           |
| `mcfg_sw.mbn.5 … .14`       | 13 956–42 864| X24 carrier software profiles (10 entries)                                |
| `storsec.mbn`               | 21 436       | Storage-secure TA                                                         |
| `adspr.jsn`                 | 403          | QMI service-registry for aDSP root_pd (`qmi_instance_id: 74`)             |
| `adspua.jsn`                | 555          | QMI service-registry for aDSP audio_pd                                    |
| `battmgr.jsn`               | 516          | QMI service-registry for charger_pd (used by pmic_glink battery manager) |
| `cdspr.jsn`                 | 403          | QMI service-registry for cDSP root_pd (`qmi_instance_id: 76`)             |
| `modemuw.jsn`               | 713          | QMI service-registry for modem wlan_pd (`qmi_instance_id: 180`)           |

Other GCC-level firmware outside `samsung/w767/` but still required:

* `/lib/firmware/qcom/a680_gmu.bin` — 104 328 bytes — GMU microcode for Adreno 680.
* `/lib/firmware/qcom/a680_sqe.fw` — 32 456 bytes — SQE microcode (SPRITE/SQE engine).

### 9.1 Signing notes

All `.mbn` files are signed ELFs with Qualcomm's standard PBL/XBL secure
boot chain. `qcdxkmsuc8180.mbn` is the "zap shader" and is loaded by
the `&gpu` node's `zap-shader` sub-node:

```
# dts-stage-v2/sc8180x-samsung-w767.dts, 431–438
&gpu {
    status = "okay";
    zap-shader {
        memory-region = <&gpu_mem>;
        firmware-name = "qcom/samsung/w767/qcdxkmsuc8180.mbn";
    };
};
```

### 9.2 Cross-check vs. Windows expectations

`win-extract/ReverseEngineering/Logs/05-setupapi.dev.log` shows the Windows
stack pinning on both `qcadsprpc8180.inf` and `qcadsprpcd8180.inf`,
meaning Windows consumes the same aDSP `.mbn` we reference. Driver INFs
grep-confirmed in the pnputil dump:

```
qcpil8180.inf              # PAS loader (== remoteproc_pas)
qcpilext8180.inf           # PAS extension
qcsubsys8180.inf           # Subsystem core
qcsubsys_ext_adsp8180.inf  # aDSP subsystem
qcsubsys_ext_cdsp8180.inf  # cDSP subsystem
qcsubsys_ext_mpss8180.inf  # MPSS subsystem
qcsubsys_ext_scss8180.inf  # SCSS (compute subsystem)
qcsyscache8180.inf         # System cache
qcglink8180.inf            # GLink (== rpmsg/glink in Linux)
qcipcrouter8180.inf        # IPC router (Qualcomm Messaging Interface)
qcpdsr.inf                 # Protected-domain service registry
qcrevrmnet8180.inf / qcrmnetbridge8180.inf  # RMNet IP over modem
```

i.e., all four Hexagon images we ship mirror services that Windows
explicitly installs.

---

## 10. Audio subsystem

The Galaxy Book S uses **Qualcomm's integrated WCD codec path through
the aDSP**, branded as "Qualcomm Aqstic" — **not** an external ALC298 or
TFA/NXP amp. Windows surfaces this directly:

```
Class                       : AudioEndpoint
FriendlyName                : Speakers (Qualcomm(R) Aqstic(TM))
FriendlyName                : Internal Microphone Array - Front (Qualcomm(R) Aqstic(TM))
InstanceId                  : ADSP\VEN_QCOM&DEV_0410&SUBSYS_CLS08180\3&39BFDC76&0&0
Service                     : qcslimbus
```

and the driver INFs include:
```
qcauddev8180.inf           # Aqstic audio device
qcauddev8180_ss.inf        # Samsung-SKU audio customisation
qcaudminiport_ss.inf       # audio miniport (Samsung SKU)
qcslimbus8180.inf          # SLIMbus enumerator for codec on ADSP bus
qcacsp_cls8180.inf         # Audio core-services platform
qclistensm8180.inf         # Voice-wake ("listen soundmodel")
qclistensm_swc_ext8180.inf
qclistensm_swc8180.inf     # SoundWire/client version
dax3_ext_qc_dolbyatmos_vlldp1.2.inf   # Dolby Atmos endpoint extension (Dolby PX)
dax3_ext_qc_dolbyaudiopremium.inf     # Dolby Audio Premium extension
dax3_swc_aposvc_arm64.inf             # APO service (DSP effects host)
dax3_swc_hsa_arm64.inf                # HSA (hardware streaming agent)
```

### 10.1 Codec topology

* **Codec IC**: Qualcomm WCD (Aqstic) inside the SoC + WSA speaker-amp
  companion die attached via **SoundWire/SLIMbus**, all routed through
  the aDSP. The W767 does *not* surface an ALC298 or TFA98xx on any
  I²C bus; the ACPI namespace confirms only Qualcomm `QCOM040A` ("Bus
  Device" on aDSP subsystem), `QCOM0410` ("Bus Device" slim-bus enumerator),
  `QCOM040C`, `QCOM0411` etc — all `qcslimbus`/`qcbam`/`qci2c`/`qcspmi`
  Qualcomm internal buses.
* **Speakers**: internal stereo pair driven off the WSA path from the
  aDSP. No discrete class-D amplifier visible.
* **Microphone**: "Internal Microphone Array - Front" — a DMIC array
  connected to the aDSP analog front-end. Windows reports only a front
  array; there is no world-facing array.
* **3.5 mm headphone jack**: routed through the Aqstic HPH output and
  jack-detect GPIOs (USB-C audio-switch path via `usb_prim_qmpphy`
  `mode-switch` is commented out in the current DTS — DP-alt / analog-jack
  switch is still pending).
* **ADSP firmware** (`qcadsp8180.mbn`) hosts:
  * AVS (Audio Virtual Service) — cross-ref `adspua.jsn` declares
    `provider: avs, service: audio` for the `audio_pd` sub-domain.
  * Dolby Atmos / Dolby Audio Premium running as an APO in
    `audio_pd`.
  * Qualcomm "Listen" sound-model service (wake-word / voice trigger)
    running on cDSP (`qclistensm_swc8180.inf`).

### 10.2 Linux driver chain (future work)

Current iteration 16 `sc8180x-samsung-w767.dts` does **not** yet instantiate
the `sound`/`soundwire`/`slim_ngd` nodes — the aDSP loads but no
audio routing is set up. Target bindings (upstream for sc8180x / sm8150):
`qcom,q6dsp-lpass-clocks`, `qcom,sc8180x-lpass-audio`, `qcom,wcd938x` /
`qcom,wcd937x` (equivalent WCD codec), `qcom,wsa8815` speaker amp,
`qcom,soundwire`, `simple-audio-card`. The aDSP `glink-edge` label "lpass"
is the anchor point for all of this.

---

## 11. WiFi / Bluetooth — WCN3990

```
# mainline-dts/sc8180x.dtsi, 3969–3990
wifi: wifi@18800000 {
    compatible = "qcom,wcn3990-wifi";
    reg = <0 0x18800000 0 0x800000>;
    reg-names = "membase";
    clock-names = "cxo_ref_clk_pin";
    clocks = <&rpmhcc RPMH_RF_CLK2>;
    interrupts = <GIC_SPI 414..425 IRQ_TYPE_LEVEL_HIGH>;  /* 12 IRQs */
    iommus = <&apps_smmu 0x0640 0x1>;
    qcom,msa-fixed-perm;
    status = "disabled";
};
```

Enabled + wired on the W767 board:

```
# dts-stage-v2/sc8180x-samsung-w767.dts, 1096–1106
&wifi {
    status = "okay";
    memory-region = <&wlan_mem>;
    vdd-0.8-cx-mx-supply = <&vreg_l1e_0p75>;   /* PMIC-E ldo1 */
    vdd-1.8-xo-supply    = <&vreg_l7a_1p8>;    /* PMIC-A ldo7 */
    vdd-1.3-rfa-supply   = <&vreg_l9a_1p3>;    /* PMIC-A ldo9 */
    vdd-3.3-ch0-supply   = <&vreg_l11c_3p3>;   /* PMIC-C ldo11 */
    //vdd-3.3-ch1-supply = <&vreg_l10c_3p3>;   /* not used?   */
};
```

`wlan_mem` reserved region (44–46):

```
wlan_mem: memory@8bc00000 {
    reg = <0x0 0x8bc00000 0x0 0x180000>;      /* 1.5 MiB MSA */
    no-map;
};
```

* Mainline driver: **ATH10K_SNOC** (`qcom,wcn3990-wifi`). Firmware comes
  from within MPSS — the modem sub-system runs a `wlan_pd` sub-domain
  (`modemuw.jsn`) that loads `wlanmdsp.mbn`. Therefore WLAN is
  **dependent on MPSS being up**. With MPSS currently not loading on
  Linux, Wi-Fi will not bring up until MPSS is sorted.
* `qcom,msa-fixed-perm` tells the PAS loader to assign the MSA
  memory with fixed VMIDs instead of dynamic VMID-Handoff (QCOM
  fuse-restricted platforms).
* Windows service: `qcwlan` (pnp `QCMS\VEN_QCOM&DEV_042B`, "Qualcomm(R)
  Wi-Fi B/G/N/AC (2x2) Svc"). Extension INF: `qcwlan8180_ext.inf`.
* Bluetooth shares the same WCN3990 die over UART13 H4 + BT-specific
  regulators. Board DTS 972–987:

```
&uart13 {
    status = "okay";
    pinctrl-names = "default";
    pinctrl-0 = <&uart13_state>;

    bluetooth {
        compatible = "qcom,wcn3998-bt";

        vddio-supply  = <&vreg_s4a_1p8>;
        vddxo-supply  = <&vreg_l7a_1p8>;
        vddrf-supply  = <&vreg_l9a_1p3>;
        vddch0-supply = <&vreg_l11c_3p3>;
        max-speed     = <3200000>;
    };
};
```

Note that `qcom,wcn3998-bt` is used even though the chip is a WCN3990 —
the bluetooth binding is shared across the WCN3990/3998 family. The SCO
audio path runs directly between BT and aDSP PCM lines (bypassing the
APSS). Windows service for BT UART transport: `qcbtfmuart8180.inf` +
`QcBluetooth` (ACPI\QCOM0471), + HCI driver `BthMini`.

---

## 12. Modem (MPSS) — Snapdragon X24 LTE

* Chip identity (Windows, `pnp_all.txt`):
  `FriendlyName : Snapdragon (TM) X24 LTE Modem`
  `HardwareID   : QCMS\VEN_QCOM&DEV_0489&SUBSYS_SSKU_AHP`
  `Service      : mbb` (MBIM driver)
* Additional device instances:
  * `ACPI\QCOM04AF` "Qualcomm Modem Limiting Thermal Device"
  * `ACPI\SAM0602` "ModemCtrl Device" (Samsung-specific modem-control ACPI device)
  * `SWD\RADIO\{CC75F028-18EF-40A9-9767-EC12C8B6C83E}` (SoftwareDevice "Cellular")
* SIM tray: the W767 has a single physical nano-SIM slot. The `modemctrl`
  driver (`modemctrl.arm64.inf` from Samsung) is the Samsung-specific
  SIM-state / airplane-mode glue that talks to EMEC.
* Firmware: `qcmpss8180_XEF.mbn` + per-carrier `mcfg_sw.mbn.N` + single
  `mcfg_hw.mbn.1`.
* DTS: already configured (section 2.3). **Does not boot on Linux yet**;
  bringing it up requires (a) working rmtfs ("remote filesystem" for modem
  NV storage) — `rmtfs_mem @ 0x85500000` is provisioned in the W767 DTS
  with client-id 1, VMID 15 — and (b) a functional MSS power-domain /
  secure-boot handshake through `aoss_qmp`.

---

## 13. Battery + charger

There is no standalone I²C fuel-gauge IC or external charger IC on the
Samsung W767. Battery/charger is a **hybrid Qualcomm PMIC +
Samsung-EMEC** scheme:

* **Charger hardware**: on-die in PM8150 (primary PMIC-A). SMB2351
  companion chargers (`qcom,smb2351` at SPMI 0x2 and 0xa) are *present*
  on the platform but **disabled** on the W767 board DTS (section 3.1).
  Windows `qcbattminiclass850.inf` drives it via `ACPI\QCOM0263`
  "Qualcomm PMIC Battery Miniclass Device".
* **Fuel-gauge / "gas-gauge"**: handled in firmware by the **Samsung
  EMEC** (embedded MCU reachable over I²C). From DSDT:

```
# acpi-decompile/dsdt.dsl, 95483–95501
Device (EMEC)
{
    Name (_HID, "SAM0604")
    Name (_UID, Zero)
    Name (_SUB, "C17C144D")
    Method (_DEP, 0, NotSerialized)
    {
        Sleep (\_SB.SLEP)
        Return (Package (0x06)
        {
            \_SB.IC10, \_SB.IC20, \_SB.I2C9, \_SB.IC19,
            \_SB.IC12, \_SB.GIO0
        })
    }
    ...
}
```

  The EMEC reports `CHST` (charge state), `CHGC` (charge current), `SOC`
  (state of charge), `VOLT`, `CHTY` (charge type) to the ACPI
  battery device:

```
Device (PM3P)   # ACPI battery proxy
{
    Name (_HID, "SAM0606")
    ...
    Method (GBST, 0, NotSerialized)
    {
        If ((\_SB.EMEC.AVBL == One))
        {
            BSTP [Zero]   = \_SB.EMEC.CHST
            BSTP [One]    = \_SB.EMEC.CHGC
            BSTP [0x02]   = \_SB.EMEC.SOC
            BSTP [0x03]   = \_SB.EMEC.VOLT
            BSTP [0x04]   = \_SB.EMEC.CHTY
        }
        ...
    }
}
```

  `SAM0604` (EMEC) and `SAM0606` (PM3P battery proxy) are the handles.
  The EMEC itself occupies **five** I²C slave addresses — `0x1a, 0x25,
  0x33` on `IC10` / `IC19`, plus `0x09`/`0x0B` on `IC20` and `0x1a` on
  `IC12`. The matching board DTS placeholders (without enabling) are:

```
# dts-stage-v2/sc8180x-samsung-w767.dts, 515–586
&i2c9  { /* 0x1a, 0x25, 0x33 EMEC parts */ };
&i2c11 { /* 0x1a EMEC part */ };
&i2c18 { /* 0x25 EMEC part, 0x33 EMEC part */ };
&i2c19 { /* 0x09 and 0x0b EMEC parts */ };
```

  These are surfaced by the sibling "Samsung platform" doc — the battery
  driver will have to bind to a new `samsung,emec-battery` compatible.
* **AC adapter**: Windows exposes "AC Adapter" via the standard `AC`
  device only after the EMEC reports a valid charger type; no discrete
  ACPI0003 device exists outside `PM3P`.
* **Battery thresholds** (from `powercfg.txt`): Low 6 %, Critical 2 %,
  Reserve 4 %. Windows critical action is "Hibernate".

---

## 14. Storage — UFS 3.0

Single UFS device. No eMMC, no NVMe.

```
# mainline-dts/sc8180x.dtsi, 2185–2254
ufs_mem_hc: ufshc@1d84000 {
    compatible = "qcom,sc8180x-ufshc", "qcom,ufshc", "jedec,ufs-2.0";
    reg = <0 0x01d84000 0 0x2500>;
    interrupts = <GIC_SPI 265 IRQ_TYPE_LEVEL_HIGH>;
    phys = <&ufs_mem_phy>;
    phy-names = "ufsphy";
    lanes-per-direction = <2>;
    iommus = <&apps_smmu 0x300 0>;
    clocks = <&gcc GCC_UFS_PHY_AXI_CLK>,
             <&gcc GCC_AGGRE_UFS_PHY_AXI_CLK>,
             <&gcc GCC_UFS_PHY_AHB_CLK>,
             <&gcc GCC_UFS_PHY_UNIPRO_CORE_CLK>,
             <&rpmhcc RPMH_CXO_CLK>,
             <&gcc GCC_UFS_PHY_TX_SYMBOL_0_CLK>,
             <&gcc GCC_UFS_PHY_RX_SYMBOL_0_CLK>,
             <&gcc GCC_UFS_PHY_RX_SYMBOL_1_CLK>;
    clock-names = "core_clk","bus_aggr_clk","iface_clk","core_clk_unipro",
                  "ref_clk","tx_lane0_sync_clk","rx_lane0_sync_clk","rx_lane1_sync_clk";
    freq-table-hz = <37500000 300000000>, <0 0>, <0 0>,
                    <37500000 300000000>, <0 0>, <0 0>, <0 0>, <0 0>;
    power-domains = <&gcc UFS_PHY_GDSC>;
    interconnects = <&aggre1_noc MASTER_UFS_MEM QCOM_ICC_TAG_ALWAYS
                      &mc_virt SLAVE_EBI_CH0 QCOM_ICC_TAG_ALWAYS>,
                    <&gem_noc MASTER_AMPSS_M0 QCOM_ICC_TAG_ALWAYS
                      &config_noc SLAVE_UFS_MEM_0_CFG QCOM_ICC_TAG_ALWAYS>;
    interconnect-names = "ufs-ddr", "cpu-ufs";
    status = "disabled";
};

ufs_mem_phy: phy-wrapper@1d87000 {
    compatible = "qcom,sc8180x-qmp-ufs-phy";
    reg = <0 0x01d87000 0 0x1000>;
    clocks = <&rpmhcc RPMH_CXO_CLK>,
             <&gcc GCC_UFS_PHY_PHY_AUX_CLK>,
             <&gcc GCC_UFS_MEM_CLKREF_EN>;
    clock-names = "ref","ref_aux","qref";
    resets = <&ufs_mem_hc 0>;
    reset-names = "ufsphy";
    power-domains = <&gcc UFS_PHY_GDSC>;
    #phy-cells = <0>;
    status = "disabled";
};
```

Board override (989–1006):

```
&ufs_mem_hc {
    status = "okay";
    reset-gpios = <&tlmm 190 GPIO_ACTIVE_LOW>;
    vcc-supply   = <&vreg_l10e_2p9>;   /* PMIC-E ldo10, 2.5–2.9 V */
    vcc-max-microamp = <155000>;
    vccq2-supply = <&vreg_l7e_1p8>;    /* PMIC-E ldo7, 1.8 V      */
    vccq2-max-microamp = <425000>;
};
&ufs_mem_phy {
    status = "okay";
    vdda-phy-supply = <&vreg_l5e_0p88>;  /* PMIC-E ldo5, 0.88 V */
    vdda-pll-supply = <&vreg_l3c_1p2>;   /* PMIC-C ldo3, 1.2 V  */
};
```

IRQ 265 on GIC-SPI. SID `0x300` on apps_smmu. Reset GPIO is
TLMM GPIO 190.

---

## 15. Memory + EFI / boot

### 15.1 Memory

From `win-extract/memarray.txt`:

```
DeviceID          : Memory Array 0
StartingAddress   : 2147483648   (= 0x80000000)
EndingAddress     : 10737418239  (= 0x27FFFFFFF)   -> 8 GiB span
```

From `win-extract/physmem.txt`:

```
Manufacturer        : Samsung
Capacity            : 8589934592 (8 GiB)
Speed               : 1805 MT/s       (ConfiguredClockSpeed 1804)
ConfiguredVoltage   : 800 mV          (Min 348, Max 856)
DataWidth / Total   : 16 / 16 (single 16-bit channel? — LPDDR4X has 16-bit
                      channels × 4 grouped to 64-bit; here only Ch.0 is
                      reported in SMBIOS)
BankLabel           : Bank 0
DeviceLocator       : Top - on board      (LPDDR4X soldered)
SMBIOSMemoryType    : 30  (== LPDDR4X)
```

* RAM: 8 GiB LPDDR4X, 1805 MT/s effective, 0.8 V nominal, soldered-down.
* Linux memory node in the dtsi:

```
memory@80000000 {
    device_type = "memory";
    /* We expect the bootloader to fill in the size */
    reg = <0x0 0x80000000 0x0 0x0>;
};
```

  UEFI fills in `0x80000000` base + `0x200000000` (8 GiB) span at run
  time, leaving 6 MiB reserved from `0x85700000` onward for
  hypervisor/smem/aop + the ~160 MiB MPSS + ~32 MiB aDSP + ~8 MiB cDSP +
  ~20 MiB SCSS + ~2 MiB rmtfs + ~1.5 MiB WLAN scattered chunks.

### 15.2 EFI / boot

```
# win-extract/bcd.txt
Firmware Boot Manager
    timeout                  1
    displayorder             {9dea862c-5cdd-4e70-acc1-f32b344d4795} Windows Boot Manager
                             {19466bfd-3f0f-11f1-81b5-806e6f6e6963} Fedora (shim)

Firmware Application (101fffff)
    path    \EFI\fedora\shimaa64.efi
    description  Fedora

Windows Boot Manager
    device  partition=\Device\HarddiskVolume1
    path    \EFI\Microsoft\Boot\bootmgfw.efi
    default {f9bd6507-b9d2-11ee-acd0-f74ec226582d}

Hypervisor Settings
    hypervisorlaunchtype     Auto   (HVCI Hyper-V launched)
    hypervisordebugtype      Serial
    hypervisordebugport      1
    hypervisorbaudrate       115200
```

* **UEFI variant**: AArch64 UEFI (EDK2 + AMI Aptio upstream), version
  `P02AHP.003.241226.WY.1518` (SMBIOS firmware build from AMI 5.13).
* **Secure Boot**: enabled — the Fedora shim entry `shimaa64.efi` is the
  canonical MS-signed shim path the user runs. HVCI requires Secure Boot
  to be on.
* **Boot menu**: FW-Manager points at `bootmgfw.efi`, with `Fedora`
  (`shimaa64.efi`) as a sibling entry.
* **Resume**: `\Windows\system32\winresume.efi` from `C:\hiberfil.sys`.
* **TPM**: TPM 2.0 table present (`acpi-decompile/tpm2.dsl`) — fTPM /
  QSEE-based.
* **Platform debug**: DBG2 describes two debug UARTs — a standard UART
  at `0x00A90000` labelled `\_SB.UARD`, plus a DCC-over-memory at
  `0x0A600000` (port subtype 0x5143 — Qualcomm DCC).

---

## 16. Open questions / gaps

1. **SLPI binding**. There is no `qcom,sc8180x-slpi-pas` binding upstream
   yet; we have the firmware but no DT node. The sensor stack (BH1733,
   grip sensor, hall) cannot come online without it.
2. **MPSS secure-boot handshake**. The current iteration leaves MPSS at
   `status = "okay"` but actual authentication has not yet been observed
   to succeed. Bringing this up needs the AOSS QMP subsystem-handover
   sequence and potentially a kernel log trace from a failed attempt.
3. **Audio routing graph** is entirely missing from the DTS. We have a
   loading aDSP but no `sound`/`soundwire`/WCD-codec/WSA nodes.
4. **USB-C ALT mode switch**. The `usb_prim_qmpphy` `mode-switch` / `svid`
   endpoint blocks are commented out pending the UCSI/PMIC-GLINK ppm.
5. **Thermal cooling-devices**. Mainline only ties `&gpu` into
   gpu-top/gpu-bottom zones; the CPU cooling is done by LMh in firmware
   rather than explicit `cpufreq-cdev` bindings. Worth adding cpufreq
   cooling-maps for each cpu zone.
6. **Battery / EMEC driver**. No mainline binding — will need either a
   Samsung-specific driver or a `samsung,galaxybook-emec` compat layer
   that understands the 5-address I²C/GPIO dance exposed through DSDT
   `SAM0604`.
7. **Charger-SMB2351 path**. Two `qcom,smb2351` PMICs are physically
   present but left disabled on the Samsung DTS. They could in principle
   be re-enabled for better charging telemetry, but today neither
   Windows nor Linux uses them — everything rides the PM8150 internal
   charger + EMEC SoC.
8. **Video/Camera CCs**. Both `videocc` and `camcc` are present in the
   dtsi but not enabled in the W767 board DTS. Video decode in
   `qcvss8180.mbn` is unused; the front camera (`qccamfrontsensor8180`,
   `qccamisp8180`, `qccamplatform8180` on Windows) is similarly dark on
   Linux.
9. **PCIe**. The dtsi declares four PCIe controllers (`pcie0..3`) — not
   wired on the W767 (no slots, no NVMe), but `pcie3` has reserved pin
   definitions (`pcie3_default_state` at GPIO 178/179/180 in the board
   DTS) suggesting an unused M.2 footprint or an internal expansion
   path; needs verification against the physical board.
10. **AC-adapter and battery** still require documentation of the
    Samsung-specific regulators that sit between `vph_pwr` and the
    battery cell (e.g. OVP path, VBUS switch).

---

## 17. Document metrics

Measured with `wc` on the final file:

* **Word count**: 8 187 words
* **Line count**: 1 579 lines
* **Byte count**: 72 251 bytes (~70 KiB markdown)

End of document.
