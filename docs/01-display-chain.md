# 01 — Display, GPU & Backlight Chain (Samsung Galaxy Book S, SM-W767)

SoC: Qualcomm SC8180X (Snapdragon 8cx Gen 1). Panel: 13.3" eDP.
Scope: everything from the GPU/DPU block on the SoC to the panel's internal backlight driver, plus the Samsung-specific overlay drivers.

## 1. Executive summary

The Galaxy Book S has an Adreno 680 GPU on a single SoC display block (SC8180X DPU + eDP PHY) driving a 13.3" eDP panel. On Windows the full stack is:

1. `qcdxkm8180.sys` (KMD, Qualcomm) — driver date 09/24/2021, version 27.20.1640.0, the core DirectX display miniport that also owns panel power sequencing, ABA (adaptive backlight) and the CABL content-adaptive-brightness pipeline.
2. `paneldriver.sys` + `PanelManagerSvc.exe` (Samsung) — HID side-channel that sits on top of the ACPI `SSPN` device (HID `SAM0101`, CID `C17C144D`) and brokers OEM-specific brightness/ABA/panel commands.
3. `SamsungOSD.exe` — user-space brightness-percentage popup.

The Qualcomm INF writes **only two** registry values into `Miniport\DISPLAY\Config`: `PanelCfg1BrightnessMinLuminance = 200` (0.2 cd/m², i.e. 200 mcd/m²) and `PanelCfg1BrightnessMaxLuminance = 319970` (~320 cd/m²). There is **no** `Miniport\BACKLIGHT` subkey and **no** `BacklightPmic*` override — the KMD ships generic support for PMIC-WLED, PWM and PMI brightness, but the shipping `qcdx8180.inf` for `SUBSYS_CLS08180` (the machine's match, REV_0D12) leaves backlight control defaulted. Combined with the characteristic `mcd/m²` luminance range and the presence of `EDPDPCDRead` / `EDPOverrideDPCDCaps` knobs, the evidence points to **DPCD-AUX panel-internal backlight control (eDP 1.4 `EDP_DISPLAY_CONTROL_REGISTER` + `EDP_BACKLIGHT_BRIGHTNESS_MSB/LSB`, DPCD 0x720/0x722)** as the brightness path, not PMIC WLED or an EC side channel.

What works on mainline Linux today: `msm/dpu1` + `msm/adreno` cover DPU + GPU; `msm/edp` exists as a controller driver. What is blocked: no `aux-bus { panel-edp { } }` child under `mdss_edp` in the current DTS (iter-16), no backlight node, and the Samsung `SSPN` overlay has no mainline binding. Until DPCD-AUX backlight is wired up (`backlight = <&mdss_edp>` plus aux-bus panel probing), the Book S will boot black-screen or full-brightness only.

## 2. Hardware topology

```
+-------------------------------------------------------------------+
|                 SC8180X SoC (Snapdragon 8cx Gen 1)                |
|                                                                   |
|   +-----------+        +----------------+      +--------------+   |
|   |  Adreno   |  AXI   |   MDSS / DPU   |      |  Display     |   |
|   |  680 GPU  |<------>|   (mdss@ae0_,  |      |  Clock Ctrl  |   |
|   |  (gpu@    |        |   mdp@ae01000) |<---->|  (dispcc@    |   |
|   |  2c00000) |        +-------+--------+      |   af00000)   |   |
|   +-----+-----+                |               +--------------+   |
|         |                      | DPU-OUT                          |
|         | GMU                  v                                  |
|   +-----v-----+        +----------------+                         |
|   | gmu@      |        |    eDP CTL     |                         |
|   | 2c6a000   |        | (mdss_edp@     |                         |
|   +-----------+        |   ae9a000)     |                         |
|                        +-------+--------+                         |
|                                |  MAIN  LINK  (4 lanes)           |
|                                v         +   AUX  CH  (I2C-like)  |
|                        +----------------+                         |
|                        |  eDP DSI PHY   |                         |
|                        | (edp_phy@      |                         |
|                        |   aec2a00)     |                         |
|                        +-------+--------+                         |
+--------------------------------|----------------------------------+
                                 |  eDP cable (4 main lanes + AUX)
                                 v
+-------------------------------------------------------------------+
|              13.3" eDP panel assembly (inside lid)                |
|  +---------------------------+   +--------------------------+     |
|  |  Panel TCON + DPCD regs   |   |  Internal WLED/LED       |     |
|  |  (0x000 capability page,  |-->|  backlight driver        |     |
|  |   0x700 eDP extension;    |   |  (controlled via panel   |     |
|  |   brightness @0x722/0x723)|   |   TCON from DPCD regs)   |     |
|  +-------------+-------------+   +--------------------------+     |
|                ^                                                  |
|                | AUX brightness writes                            |
+----------------|--------------------------------------------------+
                 |
                 |        +--------------------------+
                 +--------| Samsung EC / SSPN side-  |
                          | channel (HID SAM0101,    |
                          | ACPI _HID=C17C144D)      |
                          | -- opaque, goes through  |
                          | PanelDriver.sys          |
                          +--------------------------+
```

The Samsung side-channel operates via a separate ACPI device (`SSPN`, see section 7), not via the eDP cable itself. See `02-samsung-platform.md` for the full SSPN / HID SAM0101 flow. It is drawn here only to show that two independent paths can reach the panel: the DPCD AUX link (Qualcomm-owned) and the Samsung OEM HID pipe (Samsung-owned).

## 3. Qualcomm display kernel driver (`qcdxkm8180.sys`)

### 3.1 File role and size

`qcdxkm8180.sys` is the Qualcomm DirectX kernel-mode display miniport driver for SC8180X. 2,921,832 bytes, AArch64 PE, built `Sep 24 2021 06:20:24` (qcdx-ascii.txt:5612-5613). It covers the full display stack: GPU scheduling (adreno), MDSS/DPU programming, DSI/eDP controller, panel power sequencing, backlight, ABA, CABL and video (Ved*) decode/encode KMD glue. The Samsung Book S matches the `QCDX_Inst_CLS_8180.NT` INF section (`InfSection="QCDX_Inst_CLS_8180.NT"`, display-class.reg:19).

### 3.2 `BacklightPmic*` configuration knobs

Registry-value names consumed by the driver when a board INF sets them. **None of these are populated on the Book S** — the INF doesn't emit a `BacklightPmic*` block for this SUBSYS — but the driver supports them. Extracted from `/tmp/qcdx-ascii.txt:7795-7833`:

| Registry value | Purpose |
|---|---|
| `BacklightType` | Selects backlight controller class (PMIC WLED / PWM / PMI / DPCD-AUX / ...) |
| `BacklightPmicModel` | Which PMIC chip drives WLED (pm8150/pm8150l/pmc8180c/...) |
| `BacklightPmicControlType` | Register-level control mode |
| `BacklightPmicNum` | PMIC SID index |
| `BacklightPmicBankSelect` | SPMI bank |
| `BacklightPmicPWMSizeinBits` | PWM resolution |
| `BacklightPmicPWMGlitchRemoval` | Glitch filter enable |
| `BacklightPmicPWMFrequency` | PWM freq (Hz) |
| `BacklightSteps` | Discrete brightness steps exposed |
| `BacklightDefault` | Boot-time default level |
| `BacklightLowPower` | Level used in modern-standby |
| `BacklightPmicAdvancedConfig` | Gate for the WLED-advanced block below |
| `BacklightPmicWledInternalModResolution` | WLED PWM internal modulation bits |
| `BacklightPmicWledModulationClkSel` | WLED modulator clock source |
| `BacklightPmicWledDimmingMethod` | Analog / digital / hybrid dimming |
| `BacklightPmicWledOvp` | Over-voltage-protection threshold |
| `BacklightPmicWledIlim` | String-current limit |
| `BacklightPmicWledFeedbackCtrl` | Feedback pin selection |
| `BacklightPmicWlepLoopCompRes` | Loop comp resistor (typo in driver: "Wlep") |
| `BacklightPmicWledVrefControl` | Vref adjustment |
| `BacklightPmicWledFullScaleCurrent` | Full-scale mA |
| `BacklightPmicWledModulatorSrcSel` | Modulator source mux |
| `BacklightPmicWledCabcEnable` | PMIC-side CABC input enable |
| `BacklightPmicOLEDWledAvddVoltage` | OLED AVDD voltage (N/A for this LCD) |
| `PMIPowerPmicModel` / `PMIPowerPmicNum` / `PMIPowerConfig` | Companion PMI-power rail |

Per-level luminance targets the driver consumes (`qcdx-ascii.txt:7806-7817`):

| Value | Purpose |
|---|---|
| `BrightnessMaxLuminance` | Global max, mcd/m² |
| `BrightnessMinLuminance` | Global min, mcd/m² |
| `BrightnessRangeLevel0` .. `BrightnessRangeLevel9` | 10 per-step luminance targets |

On the Book S, only `BrightnessMax/MinLuminance` (319970 / 200 mcd/m²) actually land in the registry, and they land under `Miniport\DISPLAY\Config` as `PanelCfg1Brightness{Max,Min}Luminance` (see section 5 and INF section 4.1) — not under `Miniport\BACKLIGHT`.

### 3.3 `CABL*` / adaptive-brightness knobs

CABL = Content Adaptive Backlight. These are the registry values the driver reads (twice, once under a BRIGHTNESS_CONTROL block and again as raw names). Source: `/tmp/qcdx-ascii.txt:7554-7568, 7841-7851`:

| Registry value | Purpose |
|---|---|
| `AdaptiveBrightnessFeature` | Master gate for ABA (see 3.4) |
| `CABLEnable` | Master gate for CABL |
| `CABLMinUserLevel` | Minimum OS slider % before CABL engages |
| `CABLMinBacklightLevel` | Floor for backlight reduction |
| `CABLFilterThreshold` | Image-stats frame-diff threshold |
| `CABLPerQualFilterThreshold` | Per-quadrant filter threshold |
| `CABLDeactivatingSlope` | Ramp rate when disengaging |
| `CABLChangingBacklightSlope` | Ramp rate when adjusting |
| `CABLScaleRatioUpperLimit` | Max allowed gamma scale |
| `CABLDistortionRate` | Perceived-distortion budget |
| `CABLReservedLT` | Reserved (ambient-light tie-in) |
| `CABLLuxEndPoint` | Ambient-lux saturation point |
| `CABLBacklightResponseX` / `Y` | Backlight LUT axis |
| `CABLGammaResponseX` / `Y` | Gamma-compensation LUT axis |

The CABL tuning data is also loaded from an XML file, `qccablconfig.xml` (`qcdx-ascii.txt:5607`). The diagnostic block (`qcdx-ascii.txt:5585-5602`) prints the current CABL state including backlight ratio, power-savings mode (`High (Aggressive)` / `Medium` / `Low (Conservative)`) and min threshold.

### 3.4 Brightness code path (key strings)

Relevant UTF-16 strings (from `/tmp/qcdx-utf16.txt`):

| Line | String | Meaning |
|---|---|---|
| 28 | `\Callback\QCDXDL_BACKLIGHT_CHANGE` | Named kernel callback object fired on brightness change |
| 37 | `Miniport\BACKLIGHT` | Registry subkey the driver reads for per-adapter BL config — **not present on this machine** |
| 38 | `DisableCABL#` | Per-instance knob |
| 44 | `ForceCablOn#` | Per-instance knob |
| 45 | `BacklightLockEvent` | Named event guarding BL updates |
| 47 | `BacklightABAThread` | Worker thread that runs ABA loop |
| 105 | `PanelCfg#BrightnessMinLuminance` | Per-panel luminance floor |
| 106 | `PanelCfg#BrightnessMaxLuminance` | Per-panel luminance ceiling |
| 120 | `AUXSYNCLOCK` | eDP AUX channel lock primitive |
| 121 | `AUXEVENT` | eDP AUX event |
| 162-165 | `QCDXDL_EVT_REGISTER` / `_UNREGISTER` / `_AW` / `_AR` | Display-link callbacks, `_AW`/`_AR` = AUX write/read |
| 166-176 | `paneldriver.sys` / `\Device\paneldriver` / service path | Qualcomm KMD explicitly loads Samsung's `paneldriver.sys` as a dependent miniport |
| 253-255 | `Miniport\PanelOverride`, `PanelEdid` | EDID override path |

ABA path in plain terms: `BacklightABAThread` runs periodically (gated by `AdaptiveBrightnessFeature`), reads current image stats, consults the CABL LUTs, and writes a new backlight level via `QCDXDL_BACKLIGHT_CHANGE`. The underlying transport is whatever `BacklightType` resolves to. Because no `BacklightType` is set on this machine (section 5), the driver falls back to its default — and the presence of `AUXSYNCLOCK` / `AUXEVENT` as dedicated primitives in the same binary is consistent with a DPCD-AUX default.

Diagnostic string block (`/tmp/qcdx-ascii.txt:5585-5602`) is what `dxdiag`-style dumps emit:

```
[Display #%i - Brightness Configuration]
Max brightness: %i.%i nits
Min brightness: %i.%i nits
OS brightness level: %i%% (%i.%i nits)
ABA brightness level: %i%% (%i.%i nits)
Smooth Brightness: %s
Smooth Brightness Transition Time: %ims
Content Adaptive Brightness (CABL): %s
CABL Backlight ratio: %i
CABL Power Savings Mode: %s
CABL Min Threshold: %i
```

### 3.5 Other files in the driver package

Full listing of `qcdx8180.inf_arm64_c1c5f5f4255a7d2a/` (sizes in bytes):

| File | Size | Role |
|---|---:|---|
| `qcdxkm8180.sys` | 2,921,832 | KMD (this document) |
| `qcdxkmsuc8180.mbn` | 14,240 | KMD signed ucode / firmware blob |
| `qcvss8180.mbn` | 1,159,200 | GPU/video subsystem firmware |
| `qcdx11arm64xum8180.dll` | 6,692,512 | D3D11 UMD (arm64ec) |
| `qcdx11arm32um8180.dll` | 1,920,920 | D3D11 UMD (arm32) |
| `qcdx11x86um8180.dll` | 2,478,416 | D3D11 UMD (x86-on-arm64) |
| `qcdx11chpeum8180.dll` | 3,693,928 | D3D11 UMD (CHPE) |
| `qcdx12arm64xum8180.dll` | 14,679,888 | D3D12 UMD (arm64ec) |
| `qcdx12arm32um8180.dll` | 5,602,832 | D3D12 UMD (arm32) |
| `qcdx12x86um8180.dll` | 6,598,384 | D3D12 UMD (x86-on-arm64) |
| `qcdx12chpeum8180.dll` | 7,555,592 | D3D12 UMD (CHPE) |
| `qcdxarm64xcompiler8180.DLL` | 24,671,656 | Shader compiler (arm64ec) |
| `qcdxarm32compiler8180.DLL` | 8,569,968 | Shader compiler (arm32) |
| `qcdxx86compiler8180.DLL` | 10,616,432 | Shader compiler (x86) |
| `qcdxchpecompiler8180.dll` | 14,419,856 | Shader compiler (CHPE) |
| `qcdxsdarm64x.dll` | 5,060,688 | GPU statistics (arm64ec) |
| `qcdxsdarm32.dll` | 933,192 | GPU statistics (arm32) |
| `qcdxsdx86.dll` | 1,286,880 | GPU statistics (x86) |
| `qcdxsdchpe.dll` | 2,941,952 | GPU statistics (CHPE) |
| `qchdcpumd8180.dll` | 69,184 | HDCP content-protection UMD |
| `qcvidenc*um*.DLL` (8 files) | 85 KB–1,023 KB | MFT H.264/HEVC video encoder |
| `qcdx8180.inf` | 70,542 | INF (UTF-16) |
| `qcdx8180.cat` | 1,046,612 | Driver catalog signature |

None of the UMDs own the backlight — all brightness decisions happen inside `qcdxkm8180.sys`.

## 4. Qualcomm display INF (`qcdx8180.inf`)

Raw INF is UTF-16 (70,542 bytes); decoded copy at `/tmp/qcdx8180-2021.inf`. `DriverVer = 09/24/2021, 27.20.1640.0000` (line 19).

### 4.1 `[QCDX_PanelcfgOverrides]`

Exact contents (`qcdx8180-2021.inf:306-308`):

```ini
[QCDX_PanelcfgOverrides]
HKR, Miniport\DISPLAY\Config, PanelCfg1BrightnessMinLuminance, %REG_DWORD%, 200
HKR, Miniport\DISPLAY\Config, PanelCfg1BrightnessMaxLuminance, %REG_DWORD%, 319970
```

Interpretation: the `PanelCfg#` schema takes luminance in millicandela per square metre (mcd/m², i.e. nits × 1000). The Book S panel range is therefore **0.2 nits → 319.97 nits**. A 0.2 nits floor is implausible as a true panel minimum and strongly suggests a computed/perceptual floor rather than a PWM-duty floor — consistent with DPCD-AUX brightness, which is an abstract 16-bit value mapped by the panel TCON.

This section is `AddReg`-ed only in the `CLS_8180` / `MTP_8180` / `CLS_7180` / `IDP_7180` instances (`qcdx8180-2021.inf:137, 143, 149, 155`). The base `QCDX_Inst.NT` (line 128-132) — used for the bare `ACPI\VEN_QCOM&DEV_043A` fallback — does *not* include it. The Book S does match `SUBSYS_CLS08180` (display-class.reg:20), so it does receive the override.

### 4.2 `[QCDX_SoftwareDeviceSettings]`

Exact contents (`qcdx8180-2021.inf:294-304`):

```ini
[QCDX_SoftwareDeviceSettings]
HKR,, InstalledDisplayDrivers,     %REG_MULTI_SZ%, qcdx11arm64xum8180, qcdx11arm64xum8180, qcdx11arm64xum8180, qcdx12arm64xum8180
HKR,, VgaCompatible,               %REG_DWORD%, 0
HKR,, UserModeDriverName,          %REG_MULTI_SZ%, <>, %13%\qcdx11arm64xum8180.dll, ...
HKR,, UserModeDriverNameWow,       %REG_MULTI_SZ%, <>, qcdx11x86um8180.dll, ...
HKR,, UserModeDriverNameWow2,      %REG_MULTI_SZ%, <>, %13%\qcdx11arm32um8180.dll, ...
HKR,, UserModeDriverNameX64,       %REG_MULTI_SZ%, ""
HKR,, ContentProtectionDriverName, %REG_SZ%,       qchdcpumd8180.dll
HKR,, GpuStatisticsDriverName,     %REG_SZ%,       %13%\qcdxsdarm64x.dll
HKR,, GpuStatisticsDriverNameWow,  %REG_SZ%,       qcdxsdx86.dll
HKR,, GpuStatisticsDriverNameWow2, %REG_SZ%,       %13%\qcdxsdarm32.dll
```

Nothing display- or backlight-specific here — this just wires up the UMDs for each ABI flavour. Relevant only because the D3D11 UMD slot is triple-listed (`qcdx11arm64xum8180` × 3), which is the `UMD, UMD, UMD, UMD12` array shape Windows expects.

### 4.3 2020 → 2021 rename pattern

The `"8180"` suffix on every filename (`qcdxkm8180.sys`, `qcdx11arm64xum8180.dll`, etc.) is a rename of the earlier non-suffixed Qualcomm WDDM driver set and of the earlier 2020 vintage that used e.g. `qcdxkm8180_20.sys` / `qcdxkmsuc.mbn`. The shipping 2021 package `27.20.1640.0` matches the files on disk 1:1 and maps to the driver date `09/24/2021`. Not enumerated further.

## 5. Registry state on the actual machine

Source: `/home/peter/Documents/GalaxyBookS_Linux/win-extract/display-class.reg` (export of `HKLM\SYSTEM\CurrentControlSet\Control\Class\{4d36e968-e325-11ce-bfc1-08002be10318}`).

Active adapter instance is `\0000` (display-class.reg:12-98):

| Value | Contents |
|---|---|
| `DriverDesc` | `Qualcomm(R) Adreno(TM) 680 GPU` |
| `DriverVersion` | `27.20.1640.0` |
| `DriverDate` | `9-24-2021` |
| `InfPath` | `oem61.inf` |
| `InfSection` | `QCDX_Inst_CLS_8180.NT` |
| `MatchingDeviceId` | `ACPI\VEN_QCOM&DEV_043A&SUBSYS_CLS08180` |
| `HardwareInformation.ChipType` | `Adreno 680` |
| `HardwareInformation.BiosString` | `Version 72.41` |

The active monitor / display instance is `REV_0D12` (display-class.reg:113, inside the VolatileSettings GUID): `ACPI\VEN_QCOM&DEV_043A&SUBSYS_CLS08180&REV_0D12`. **Notably, `REV_0D12` does NOT appear in the INF's `[QC.NTARM64]` match list** (`qcdx8180-2021.inf:105-126` lists REV_0D13, REV_0D15, REV_0D01, REV_0D11, REV_0912/0913/0900/0910 — not 0D12). So the Book S falls through to the bare-SUBSYS match `SUBSYS_CLS08180` → `QCDX_Inst_CLS_8180` (line 113), which is exactly what `InfSection` in the registry confirms.

Subkey hierarchy actually present under `\0000\`:

```
\0000\Miniport\
\0000\Miniport\DISPLAY\
\0000\Miniport\DISPLAY\Config\
    PanelCfg1BrightnessMinLuminance = 0x000000c8   (= 200 mcd/m²)
    PanelCfg1BrightnessMaxLuminance = 0x0004e1e2   (= 319970 mcd/m², ~320 nits)
\0000\VolatileSettings\
\0000\Configuration\...
```

**Significant absences** (the driver looks for these but the INF doesn't write them, so they aren't there):

| Subkey the KMD probes | Present? | Source |
|---|---|---|
| `Miniport\BACKLIGHT` | **NO** | qcdx-utf16.txt:37 is looked up; reg export has no such key |
| `Miniport\BACKLIGHT\BacklightType` | **NO** | qcdx-ascii.txt:7795 |
| `Miniport\BACKLIGHT\BacklightPmic*` | **NO** | qcdx-ascii.txt:7796-7833 |
| `Miniport\PanelOverride` | **NO** | qcdx-utf16.txt:253 |
| `Miniport\Calibration\Default` | **NO** | qcdx-utf16.txt:63 |
| `Miniport\ESD` | NO | qcdx-utf16.txt:54 |
| `Miniport\IRQPolling` | NO | qcdx-utf16.txt:124 |
| `Miniport\QDSS` | NO | qcdx-utf16.txt:231 |

This is the central finding for backlight architecture: **the KMD has no PMIC backlight configuration on this machine.** Either (a) the backlight default path (eDP DPCD AUX) is used and needs no configuration, or (b) backlight is delegated entirely to the Samsung `paneldriver.sys` overlay. See section 8.

## 6. Samsung overlay drivers

### 6.1 PanelDriver.sys (HID → SAM0101 → C17C144D)

58,056-byte ARM64 kernel driver. INF path: `paneldriver.inf_arm64_7cd3a695b9ab839b/PanelDriver.inf`, driver date `03/03/2020, 0.1.0.5`, class `System`. PDB path leaks the source tree: `E:\depot\space\src\Drivers\Display\ARM64\Release\PanelDriver.pdb` (note "space" = Samsung Space Project codename for the Galaxy Book S family).

INF match:

```ini
[Standard.NTARM64]
%PanelDriver.DeviceDesc%=PanelDriver_Device, ACPI\VEN_SAM&DEV_0101&SUBSYS_C17C144D
```

i.e. it binds to the ACPI `SSPN` device whose `_HID` is `SAM0101` and `_CID` (compatible) is `C17C144D`. The device object it exposes is `\Device\SAMOControl` / `\DosDevices\SAMOControl` (found in UTF-16 strings). A `PanelManagerSvc_Service_Inst` companion user-mode service is registered by the same INF and started at boot (start-type 2).

Key string hits in `PanelDriver.sys` UTF-16:

| String | Meaning |
|---|---|
| `OEM\PANELCABLECOUNT` | OEM registry knob — possibly cable-toggle counter for panel-hotplug workaround |
| `\Callback\QCDXDL_BACKLIGHT_CHANGE` | **Same** named callback as in `qcdxkm8180.sys` — PanelDriver.sys either fires it or listens on it, bridging Samsung HID commands into the Qualcomm KMD brightness-change notification |
| `\Device\SAMOControl` | Public IOCTL device for user-mode (`PanelManagerSvc.exe`) |
| `Samsung Panel driver` | Description |

Full SSPN / SAM0101 / C17C144D walk-through (ACPI methods, IOCTLs, touch-panel vs display-panel multiplexing) is in `02-samsung-platform.md` — not duplicated here. Only the display-chain take-away matters: PanelDriver.sys participates in brightness by attaching to `QCDXDL_BACKLIGHT_CHANGE`, so backlight policy is a two-party negotiation between `qcdxkm8180.sys` and `paneldriver.sys`.

### 6.2 PanelManagerSvc.exe

37,064-byte ARM64 user-mode service. Opens `\??\SAMOControl` (the kernel device above) and writes its state under `SOFTWARE\Samsung\SamsungSettings`. Description: `Samsung PanelManager Service`. No strings suggest it sends brightness levels directly — it looks like a policy daemon that reads OEM registry settings and pushes them into the kernel driver on session/power events.

### 6.3 SamsungOSD.exe

`samsungosdservice.inf_arm64_3f053c852a460b82/` contains two binaries:

| File | Size | Role |
|---|---:|---|
| `SamsungOSD.exe` | 38,088 | Per-session UI popup — draws the brightness/volume percentage overlay |
| `SamsungOSDService.exe` | 39,880 | System-session WTS host that spawns the per-session OSD |
| `vcruntime140.dll` | 101,352 | Redistributable |

PDB leak: `D:\P4\WM_MAIN\WMLab\APP\Windows10\SamsungOSD\SamsungOSDService\obj\Release\SamsungOSDService.pdb`. These are purely cosmetic — they don't actually change brightness, they only display the level. They are mentioned here only because on Linux the equivalent is handled by the desktop environment (GNOME/KDE) and no driver work is needed.

## 7. ACPI SSPN device (brief, self-contained quote)

For self-containment only — full analysis lives in `02-samsung-platform.md`. DSDT block at `/home/peter/Documents/GalaxyBookS_Linux/acpi-decompile/dsdt.dsl:95245-95313`:

```asl
Device (SSPN)
{
    Name (_HID, "SAM0101")
    Name (_UID, Zero)
    Name (_SUB, "C17C144D")
    Method (_DEP, 0, NotSerialized)
    {
        Sleep (\_SB.SLEP)
        Return (Package (0x02) { \_SB.IC16, \_SB.GIO0 })
    }
    Name (AVBL, Zero)
    Method (_REG, 2, NotSerialized) {
        If ((Arg0 == 0x9A)) { AVBL = Arg1 }
    }
    Method (_STA, 0, NotSerialized) { Return (0x0F) }
    Method (GFTV, 0, NotSerialized) { Local0 = Zero ; Return (Local0) }
    Method (_CRS, 0, NotSerialized)
    {
        Name (RBUF, ResourceTemplate ()
        {
            I2cSerialBusV2 (0x002C, ControllerInitiated, 0x00061A80,
                AddressingMode7Bit, "\\_SB.IC16",
                0x00, ResourceConsumer, , Exclusive, )
            GpioIo (Shared, PullNone, 0x0000, 0x0000, IoRestrictionOutputOnly,
                "\\_SB.GIO0", 0x00, ResourceConsumer, , ) { 0x0019 }
            GpioInt (Edge, ActiveHigh, Exclusive, PullNone, 0x1388,
                "\\_SB.GIO0", 0x00, ResourceConsumer, , ) { 0x0074 }
        })
        Return (RBUF)
    }
}

Scope (\_SB.SSPN)
{
    OperationRegion (SMOP, 0x9A, Zero, One)
    Field (SMOP, ByteAcc, Lock, Preserve)
    {
        BRLV, 8      // "Brightness Level"? 8-bit, single-byte region
    }
}
```

Key facts for the display-chain perspective:

1. `SSPN` hangs off I²C bus `\_SB.IC16` at slave address **0x2C** (400 kHz; `0x00061A80` = 400000). This is the I²C side-channel marked in the topology diagram.
2. GPIO pin 0x19 is an output (likely reset/enable). GPIO pin 0x74 is an active-high edge interrupt (likely attention / wake).
3. An 8-bit `BRLV` field is exposed via a custom `OperationRegion` of type `0x9A`. The name and width are strongly suggestive of "Brightness Level". Because this goes through a vendor-defined region (0x9A, not `SystemMemory`/`SystemIO`), only a driver that registers for that region can read/write it — that driver is `paneldriver.sys`.
4. `_HID SAM0101` + `_SUB C17C144D` exactly match the `PanelDriver.inf` binding above.

So `SSPN` provides Samsung's out-of-band brightness channel: write `BRLV` through the 0x9A region → `paneldriver.sys` sees it → translates to an I²C 0x2C transaction or GPIO pulse → EC/panel-board acts.

## 8. Backlight architecture analysis

Three plausible mechanisms can deliver a brightness slider change to the LED string inside the panel. Each gets its own evidence table.

### 8.1 Candidate A — PMIC WLED on pmc8180c

Hypothesis: the on-board companion PMIC pmc8180c drives a WLED string to the panel backlight, analogous to pm8150l's `leds@d800` on phone platforms.

Evidence for:

| For | Source |
|---|---|
| Qualcomm KMD has a fully-populated `BacklightPmicWled*` register bank | qcdx-ascii.txt:7821-7833 |
| Phone-family template `pm8150l_wled` exists in mainline with `label = "backlight"` | pm8150l.dtsi:134-143 |

Evidence against:

| Against | Source |
|---|---|
| `pmc8180c` node in mainline has **only** `pmc8180c_lpg` (PWM), no `wled` child | sc8180x-pmics.dtsi:322-335 |
| Book S registry has no `BacklightPmic*` value under `Miniport\BACKLIGHT` (the subkey itself is absent) | display-class.reg (section 5) |
| `BacklightType` is not set → driver does not select PMIC path | qcdx-ascii.txt:7795 (defined but not populated) |
| On a clamshell eDP panel, WLED is typically integrated inside the panel assembly and driven off the panel's own boost, not by the SoC-side PMIC | Design practice |

Verdict: **ruled out**. No WLED hardware on pmc8180c is exposed to mainline; no registry config on Windows.

### 8.2 Candidate B — Samsung SSPN / EC side channel

Hypothesis: brightness is a Samsung-proprietary HID transaction to the EC over I²C bus `\_SB.IC16` at slave 0x2C, driven by `paneldriver.sys` via the `SMOP` 0x9A operation region and its `BRLV` field.

Evidence for:

| For | Source |
|---|---|
| SSPN device exists with `BRLV` 8-bit field (naming suggests "brightness level") | dsdt.dsl:95312-95318 |
| I²C 0x2C resource + GPIO attention IRQ | dsdt.dsl:95283-95301 |
| `PanelDriver.sys` is explicitly loaded by `qcdxkm8180.sys` | qcdx-utf16.txt:166-176 |
| `PanelDriver.sys` subscribes to `\Callback\QCDXDL_BACKLIGHT_CHANGE` | PanelDriver.sys UTF-16 strings |

Evidence against:

| Against | Source |
|---|---|
| `02-samsung-platform.md` establishes that the SAM0101/C17C144D pipe is a generic HID transport used for keyboard Fn keys, ALS ambient light, covers, etc.; it can **originate** brightness key events but there is no evidence it **executes** the actual brightness change |
| `BRLV` being 8-bit would cap brightness at 256 levels, which doesn't match `BrightnessRangeLevel0..9` (10 stops) or the 16-bit DPCD values the KMD's AUX primitives suggest |
| An 8-bit field in a custom region could also be a simple status byte ("is brightness change in progress"), not a setter |
| On mainline today there is no driver for SAM0101/C17C144D — if backlight ran through this, mainline Linux would have no path at all, matching the stalled state but not providing a fix |

Verdict: **plausible as a secondary channel** (e.g. ambient-light feedback, Fn-key event injection) but unlikely to be the primary brightness setter.

### 8.3 Candidate C — eDP DPCD AUX (panel-internal)

Hypothesis: the KMD writes 16-bit brightness values via DPCD-AUX to registers `EDP_BACKLIGHT_BRIGHTNESS_MSB` (0x722) / `LSB` (0x723), with enable/mode control through `EDP_DISPLAY_CONTROL_REGISTER` (0x720) and `EDP_BACKLIGHT_MODE_SET_REGISTER` (0x721), per VESA eDP 1.4 §5.2.

Evidence for:

| For | Source |
|---|---|
| KMD exports `EDPDPCDRead`, `EDPOverrideDPCDCaps`, `EDPOverrideDPCDStatus` registry knobs — the driver actively reads/writes DPCD | qcdx-ascii.txt:7863, 7874-7875 |
| Dedicated AUX synchronisation primitives `AUXSYNCLOCK`, `AUXEVENT`, and AUX read/write callbacks `QCDXDL_AR` / `QCDXDL_AW` | qcdx-utf16.txt:120-121, 164-165 |
| `Connection: eDP` and strings `Raw DPCD:`, `dpcd`, `edpcd` confirm active DPCD programming | qcdx-ascii.txt:5629, 5799-5805 |
| Luminance range specified in **mcd/m² (0.2–319.97 nits)** is the VESA DPCD luminance encoding; PMIC PWM drivers use duty cycle (0–255 or 0–4095), not mcd/m² | INF line 307-308, KMD value names 7806-7817 |
| Min of 0.2 nits is nonsensical as a physical PWM floor but sensible as a DPCD-reported luminance floor | INF line 307 |
| The absence of `Miniport\BACKLIGHT` config on the Book S registry is consistent with "use the panel's self-declared DPCD backlight caps" — the driver doesn't need to be told what PMIC to talk to because it talks to the panel over AUX | display-class.reg (section 5) |
| `QCDXDL_BACKLIGHT_CHANGE` callback is fired by the KMD and **listened** to by `paneldriver.sys` — not the other way around; Samsung gets notified of brightness, doesn't drive it | qcdx-utf16.txt:28 + PanelDriver.sys |

Evidence against:

| Against | Source |
|---|---|
| BOE TE133FHE-TS0 class panels in Chromebooks are known to sometimes require PWM (legacy) brightness even when DPCD-caps are advertised; need panel EDID + DPCD 0x701 caps to confirm | VESA eDP 1.4 |

Verdict: **current best hypothesis.**

### 8.4 Current best hypothesis

Brightness flow on the Book S is:

```
user slider / Fn key
      |
      +-- HID event via paneldriver.sys (SAM0101/C17C144D) -----\
      |                                                         |
      +-- GPU power setting API                                  v
                                                     Windows power policy
                                                         service
                                                                |
                                                                v
                                              qcdxkm8180.sys (BacklightABAThread)
                                                                |
                                              reads PanelCfg1BrightnessMin/Max
                                              applies CABL if enabled
                                                                |
                                              AUXSYNCLOCK / AUXEVENT / QCDXDL_AW
                                                                |
                                                                v
                                              DPCD write @ 0x722/0x723 (16-bit)
                                                                |
                                              + fires QCDXDL_BACKLIGHT_CHANGE --
                                                                                |
                                                                                v
                                                                  paneldriver.sys
                                                                  updates SSPN.BRLV
                                                                  for OEM bookkeeping
```

Implication for mainline Linux: wire `backlight = <&mdss_edp>;` on the `edp-panel` node and add `enable-edp-backlight;` or rely on drm_panel_edp generic detection. The `drm/msm/edp` driver supports DPCD backlight via the `drm_edp_backlight_*` helpers in recent kernels. No PMIC backlight node, no PWM, no SSPN driver required for the baseline brightness slider.

## 9. Mainline Linux mapping

| Windows component | Mainline DTS node | Mainline driver | Status |
|---|---|---|---|
| Adreno 680 GPU (KMD sched portion) | `gpu@2c00000` `compatible = "qcom,adreno-680.1", "qcom,adreno"` | `drivers/gpu/drm/msm/adreno` | Mainline (sc8180x.dtsi:2262-2268); disabled by default, enabled in board DTS |
| GPU GMU | `gmu@2c6a000` | `drivers/gpu/drm/msm/adreno/a6xx_gmu.c` | sc8180x.dtsi:2326 |
| GPU clock controller | `gpucc: clock-controller@2c90000` `compatible = "qcom,sc8180x-gpucc"` | `drivers/clk/qcom/gpucc-sc8180x.c` | sc8180x.dtsi:2370-2371 |
| DPU / MDSS top (`qcdxkm8180.sys` DPU programming) | `mdss: display-subsystem@ae00000` `compatible = "qcom,sc8180x-mdss"` | `drivers/gpu/drm/msm/disp/dpu1` | sc8180x.dtsi:2973-2974 |
| MDP / DPU controller | `mdss_mdp: display-controller@ae01000` | same | sc8180x.dtsi:3013 |
| eDP controller (`EDP*` registry keys) | `mdss_edp: displayport-controller@ae9a000` `compatible = "qcom,sc8180x-edp"` | `drivers/gpu/drm/msm/dp` (shared DP/eDP controller) | sc8180x.dtsi:3429-3430 |
| eDP PHY | `edp_phy: phy@aec2a00` `compatible = "qcom,sc8180x-edp-phy"` | `drivers/phy/qualcomm/phy-qcom-edp.c` | sc8180x.dtsi:3506-3507 |
| Display clock controller | `dispcc: clock-controller@af00000` `compatible = "qcom,sc8180x-dispcc"` | `drivers/clk/qcom/dispcc-sc8180x.c` | sc8180x.dtsi:3523-3524 |
| eDP panel (Qualcomm's `PanelCfg#` + INF luminance) | `aux-bus { panel { compatible = "edp-panel"; } }` | `drivers/gpu/drm/panel/panel-edp.c` (EDID + DPCD probed) | Present in iter-16 (sc8180x-samsung-w767.dts:641-657) |
| Backlight (DPCD) | **implicit via `backlight` prop on panel pointing to `&mdss_edp`** | DRM EDP backlight helpers (`drm_edp_backlight_*`) | **Missing in iter-16** — `backlight = <&backlight>` is commented out (line 647) |
| `BacklightPmicWled*` (not used) | would be `pmc8180c_wled` | `drivers/leds/leds-qcom-lpg.c` or `drivers/video/backlight/qcom-wled.c` | Not needed; no WLED on pmc8180c (see 8.1) |
| `paneldriver.sys` / SSPN (SAM0101/C17C144D) | ACPI `SSPN` device, binding TBD | None — would need new driver on top of Samsung HID transport | See `02-samsung-platform.md` |
| `PanelManagerSvc.exe` / `SamsungOSD.exe` | n/a | Desktop environment (GNOME Shell / KDE plasma) provides equivalent popup | n/a |
| `qchdcpumd8180.dll` (HDCP) | n/a | HDCP via DRM subsystem (`drm_hdcp_*`) | Not display-blocking |

Gap list (what needs to be done in the board DTS to get a working panel):

1. `&mdss_edp` is already okay in iter-16; fine.
2. `aux-bus` child `panel { compatible = "edp-panel"; }` exists but has **no** `backlight = <&...>` property.
3. No explicit `backlight` node — for DPCD AUX the panel driver can auto-manage it via `drm_edp_backlight_enable()` if the panel advertises support in DPCD 0x701. Confirm by reading DPCD caps once mainline bring-up reaches first light.

## 10. Open questions

1. **What value does `Miniport\DISPLAY\Config\PanelCfg1BacklightControlType` resolve to on the Book S?** The KMD string `BacklightPmicControlType` (qcdx-ascii.txt:7797) is read per-instance; if the key exists on the running machine but wasn't captured in `display-class.reg`, it would settle the PMIC-vs-DPCD question directly. *Evidence needed:* a registry dump of `HKLM\SYSTEM\CurrentControlSet\Control\Class\{4d36e968-...}\0000\Miniport\BACKLIGHT\*` after boot, or a full `\0000\` subtree export from a live Windows session.
2. **Does the BOE TE133FHE-TS0 panel advertise DPCD backlight in register 0x701?** If bit 0 (`EDP_BACKLIGHT_BRIGHTNESS_AUX_SET_CAP`) is set, Candidate C is proven. *Evidence needed:* raw DPCD dump from 0x700-0x72F. Can be obtained from a working mainline first-light boot with `drm.debug=0x10`.
3. **What is the real meaning of `SSPN.BRLV`?** Is it a setter (OS writes → EC acts) or a getter (EC writes → OS reads)? *Evidence needed:* trace `_REG(0x9A, ...)` activation + disassemble `PanelDriver.sys` handler for the 0x9A region.
4. **Does `paneldriver.sys` implement any I²C 0x2C writes, or only GPIO/interrupt handling?** *Evidence needed:* IDA/Ghidra of `PanelDriver.sys` (58 KB, tractable) looking for `HidIoSendIrp`-style OEM bridge + I²C read/write sequences.
5. **Is `OEM\PANELCABLECOUNT` significant?** The string in `PanelDriver.sys` hints at eDP cable-toggle counting for ESD/hotplug workaround. *Evidence needed:* trace the registry value across boots, or disassembly.
6. **Does the Qualcomm KMD also read ambient light to drive ABA on this machine?** The KMD has `AdaptiveBrightnessFeature` + `CABLLuxEndPoint`. Samsung has an ALS via SSPN. *Evidence needed:* whether `qcdxkm8180.sys` calls into `paneldriver.sys` for lux, or via a separate sensor HID-class driver.
7. **What is the exact clamp of `BrightnessRangeLevel0..9`?** The INF ships no override for these, so the KMD falls back to compiled-in defaults. *Evidence needed:* binary grep of `qcdxkm8180.sys` for the default array.
8. **Does mainline `msm/dp` correctly treat `qcom,sc8180x-edp` as an eDP controller (with AUX backlight helpers wired)?** *Evidence needed:* check `drivers/gpu/drm/msm/dp/dp_display.c` for eDP-specific code paths and `drm_edp_backlight_*` usage.
9. **Is `PanelCfg1BrightnessMaxLuminance = 319970` an INF-supplied override of a lower panel EDID maximum, or a true panel cap?** If the panel's DisplayID-or-EDID luminance cap disagrees, this affects HDR/metadata reporting on Linux. *Evidence needed:* `edid-decode` of the panel EDID once readable under Linux.
10. **Why does `REV_0D12` have no explicit INF entry (falls through to bare `SUBSYS_CLS08180`)?** This implies the generic CLS match is intentional and REV_0D12 is not tracked as a special variant — but confirming it matters for panel timing / DPCD-override decisions. *Evidence needed:* comparison with other CLS08180 devices (Surface Pro X, Lenovo Flex 5G).
