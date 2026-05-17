# Gemini Deep Research Brief v5 — Samsung Galaxy Book S display + backlight

**Audience:** A research agent (Gemini Deep Research) tasked with finding
authoritative, primary-source information about the **Samsung Galaxy Book S
(SM-W767, Qualcomm SC8180X / Snapdragon 8cx Gen 1)** display and backlight
subsystem, with the goal of unblocking a Linux mainline port.

**Tone for results:** prefer primary sources (datasheets, schematics, FCC
filings, Samsung service docs, Microsoft INF/registry dumps, Linux mailing
list / patchwork posts, gitlab/github commits, postmarketOS wiki + MR
discussions). Secondary sources (forum posts, blog write-ups, YouTube
teardowns, Reddit) are useful as leads but should always be corroborated.
Quote URLs and commit hashes wherever possible. **Don't summarise — cite.**

---

## 1. The device under investigation

- **Marketing name:** Samsung Galaxy Book S
- **Model number:** SM-W767 (also written as SM-W767V / SM-W767U
  for region variants — the Qualcomm-equipped one; the Intel Lakefield
  variant is SM-W767N and is OUT OF SCOPE)
- **Form factor:** 13.3" fanless 2-in-1 ARM laptop, ~960 g
- **Released:** August 2019 (announced); shipping early 2020
- **SoC:** Qualcomm SC8180X (Snapdragon 8cx Gen 1, 7 nm, Kryo 495 cores)
- **RAM:** 8 GB LPDDR4X
- **Storage:** 256 / 512 GB UFS 3.0
- **Display panel (confirmed via Linux probing of the panel ID):**
  - **BOE NV133FHM-N61**
  - 13.3", 1920 × 1080, eDP, 4-lane, no-HPD (lid embedded panel)
  - Panel-internal ID `0x07e7` (mainline `panel-edp.c` knows this as
    `boe,nv133fhm-n61`)
- **Shipped OS:** Windows 10 on ARM (with Samsung-signed firmware blobs)

**Sister / cousin devices** that have varying degrees of mainline Linux
support and may share schematics or chip choices:

- **Lenovo Flex 5G / 14Q60Q82** (also SC8180X) — has mainline DTS
  `arch/arm64/boot/dts/qcom/sc8180x-lenovo-flex-5g.dts`
- **Microsoft Surface Pro X (SQ1)** — SC8180X variant, 2019
- **HP Elite Folio / HP Spectre Folio** — sibling SoCs (SC8180X / SC8280XP)
- **Lenovo ThinkPad X13s** — SC8280XP (Gen 3), well-supported on Linux but
  a *different* SoC

The **Samsung Galaxy Book Go (SM-W737)** uses SC7180 — different SoC, but
same Samsung / Qualcomm laptop family and may share embedded-controller IP.

---

## 2. Current state of the Linux port (April 2026)

A working Fedora Rawhide image (kernel 7.0.0-62.fc45.aarch64) boots
userspace successfully on real SM-W767 hardware. Confirmed working:

- Boots from USB via UEFI, GRUB, BLS entry
- Custom DTS at `qcom/sc8180x-samsung-w767.dtb` — derived from mainline
  `sc8180x-lenovo-flex-5g.dts` with Samsung-specific deltas (panel
  compatible, firmware-name paths, reserved memory regions matching
  jenneron's 6.1 fork, plus pmOS deltas: 10 extra I²C buses, `&dispcc`,
  disabled stray `pmic@2`/`pmic@a`)
- Internal USB-HID keyboard (Caps Lock LED, Fn keys, power button)
- ASIX/Speed Dragon USB-Ethernet → SSH at 192.168.1.118
- Autologin on tty1–4, sshd, persistent journal, graceful shutdown
- ADSP + CDSP remoteprocs running (Samsung firmware loads + authenticates)
- Battery percentage readable via `/sys/class/power_supply/`

**The single blocker preventing a usable local console:** the internal eDP
panel stays dark. Kernel boot messages render fine until shortly after
`systemd` starts services in the "Starting Fedora …" phase, then the screen
goes blank. Plymouth is disabled, GDM is masked, default target is
`multi-user.target`. There is no DRM teardown sequence in dmesg — the
framebuffer just stops being visible.

A Linux backlight class device DOES exist on this build:

```
/sys/class/backlight/backlight/
  brightness:        2048
  max_brightness:    4095
  actual_brightness: 2048
  bl_power:          4
  type:              raw
  scale:             non-linear
```

But poking it does **nothing visible**: ramping `brightness` 0 → 4095 and
toggling `bl_power` between 0/1/4 produces no change in actual panel
luminance. The `actual_brightness` readback also lags the writes (suggests
there's a backing PWM / IO handler queueing writes but the physical channel
isn't driving the panel BL inverter).

Also relevant: `/sys/class/drm/` has only `card0 → simple-framebuffer.0`.
The Adreno 680 GPU (`msm_dpu`) does not finish binding because the GPU
component fails with timeout (`-110 ETIMEDOUT` after we added an explicit
`GPU_GX_GDSC` power-domain — was `-19 ENODEV` before that). With no
`msm_dpu` master, no `card1` materialises and the eDP DPU pipeline never
turns on. So the question "is the panel dark because BL is off, or because
the DPU is off" is currently **both: the BL controller isn't being driven,
AND the DPU pipeline isn't running.** Either alone would blank the panel.

---

## 3. Decompiled ACPI evidence (the gold seam)

We have a full ACPI dump captured from Windows 11 on the same hardware via
Intel ACPICA `acpidump`, decompiled with `iasl -d`:
`/home/peter/Documents/GalaxyBookS_Linux/acpi-decompile/dsdt.dsl` (3.9 MB,
~98 k lines).

### 3.1 No standard ACPI brightness methods

The DSDT contains **no** `_BCM`, `_BCL`, `_BQC`, `_DCS` methods anywhere.
This rules out the standard ACPI video extension path
(`acpi_video_get_brightness_caps_required`, etc.) — Linux's ACPI BL
backend will never bind on this device.

### 3.2 Samsung-defined ACPI device IDs (`SAMxxxx`)

```
SAM0101  →  Device "SSPN"   — ★ Samsung Panel / backlight controller
SAM0204  →  Device "ALS1"   — Ambient Light Sensor (I²C addr 0x29 on I2C8)
SAM0602  →  Device "MCTL"   — ModemCtrl
SAM0603  →  Device "AGNT"   — Agent
SAM0604  →  Device "EMEC"   — ★ Samsung Embedded Controller (multi-bus)
SAM0605  →  Device "UCME"   — UCM Emulation (USB-C connector mux)
SAM0606  →  Device "PM3P"   — Power Management 3rd party (battery?)
SAM0609  →  Device "WSAR"   — WLAN SAR (RF safety)
SAM0701  →  Device "SAFI"   — ★ Samsung Firmware Interface (EC ↔ ACPI op-region)
SAM0909  →  Device "WBDI"   — Windows Biometric Device (fingerprint)
```

The starred three are the ones that matter for display/backlight.

### 3.3 SSPN (SAM0101) — the backlight controller

```
Device (SSPN)
{
    Name (_HID, "SAM0101")
    Name (_UID, Zero)
    Name (_SUB, "C17C144D")               ; Samsung subsystem GUID
    Method (_DEP) { Return ({ \_SB.IC16, \_SB.GIO0 }) }

    Method (_CRS)
    {
        I2cSerialBusV2 (0x002C, ControllerInitiated, 0x00061A80,    ; addr 0x2C, 400 kHz
                        AddressingMode7Bit, "\\_SB.IC16", ...)

        GpioIo  (Shared, PullNone, ..., "\\_SB.GIO0", ...) { 0x0019 }   ; GPIO 25  – enable
        GpioInt (Edge, ActiveHigh, ..., 0x1388,
                                  "\\_SB.GIO0", ...) { 0x0074 }         ; GPIO 116 – interrupt
    }
}

Scope (\_SB.SSPN)
{
    OperationRegion (SMOP, 0x9A, 0, 1)
    Field (SMOP, ByteAcc, Lock, Preserve)
    {
        BRLV, 8                            ; ★ 8-bit brightness level register
    }
}
```

So the Windows driver for `SAM0101`:
1. Talks I²C at **400 kHz** to a chip at **address 0x2C on bus IC16**
2. Asserts/deasserts an enable line on **TLMM GPIO 25**
3. Listens for an interrupt on **TLMM GPIO 116** (edge, active-high)
4. Implements a custom ACPI **OpRegion 0x9A** that exposes `BRLV` (1 byte =
   brightness 0-255) — i.e. other ACPI code (e.g. EC events, Windows
   user-mode brightness slider via Samsung’s firmware interface) writes
   `BRLV` and the I²C handler in the Windows driver translates that into
   a transaction to the backlight chip.

### 3.4 IC16 → mainline `i2c17` mapping

`Device (IC16)` declares MMIO base `0x00C94000` length `0x4000`, IRQ
`0x26B`. SC8180X `qupv3_id_2` lives at `0x00C00000`–`0x00CFFFFF`; each
Serial Engine occupies `0x4000`. Walking SE0..SEn from `0x00C80000`:

```
0xC80000  SE0  → mainline i2c12 (qup_2_geni_0)
0xC84000  SE1  → mainline i2c13
0xC88000  SE2  → mainline i2c14
0xC8C000  SE3  → mainline i2c15
0xC90000  SE4  → mainline i2c16
0xC94000  SE5  → ★ mainline i2c17     ← SSPN lives here
```

**Therefore, in mainline kernel terms, the Galaxy Book S backlight
controller is an I²C device at `&i2c17 { backlight@2c { ... } }` plus two
`tlmm` GPIOs (25 enable, 116 IRQ).** The `i2c17` bus is enabled in our
current iter-15 DTS (we recently ported the pmOS overrides) but no node
exists yet for the chip itself — the kernel doesn't probe it.

### 3.5 EMEC (SAM0604) — the Samsung Embedded Controller

The EMEC device declares it talks to the Samsung EC via **I²C addresses
0x33, 0x25, 0x1A on bus IC10**, **0x33, 0x25 on IC19**, **0x1A on IC12**,
and **0x09, 0x0B on IC20** (slow 100 kHz), plus 6 active-low GPIO IRQ
lines and 3 GPIO inputs. The EC implements OpRegion 0x9C. The `_DEP` on
SSPN does NOT include EMEC — i.e. the backlight chip is independent of the
EC, but the EC may issue brightness change events via the Samsung Firmware
Interface (SAFI) below.

### 3.6 SAFI (SAM0701) — Samsung Firmware Interface

```
Device (SAFI)
{
    Name (_HID, "SAM0701")
    Name (_DDN, "Samsung Firmware Interface")
    OperationRegion (ECM2, 0x9F, 0, 0x38)        ; "EC Memory 2", 56 bytes
    Field (ECM2, ByteAcc, Lock, Preserve)
    {
        ELG0..ELG9, 8 each,                      ; "Event Log" bytes
        AAAA, 128,                               ; 16-byte arg buffer
        MADD, 32,                                ; 32-bit address
        MVAL, 32                                 ; 32-bit value
    }
    Method (PRNT, 1) { ... }                     ; debug print sink
}
```

This looks like a generic Samsung-defined memory window through which
ACPI methods proxy reads/writes to the EC firmware. The `ADBG()` debug
sink in the DSDT routes through SAFI.PRNT.

### 3.7 What's NOT in the DSDT

- No PWM-driven backlight node anywhere. **Therefore the Flex 5G mainline
  approach (`pwms = <&pmc8180c_lpg 4 1000000>`) is wrong for Samsung.**
  pmOS independently arrived at this conclusion — they comment out their
  backlight node entirely.
- No reference to `LP855X`, `LM3xxx`, `RT4831`, `KTD2151`, `BD81xx`,
  `RT8555`, `SGM3140`, or any other named backlight-driver compatible
  string. The chip identity at I²C 0x2C is unknown from ACPI alone — needs
  schematic / FCC photos / Windows INF cross-reference to identify.

---

## 4. Specific research questions for the agent

Number them so we can refer back; prefer **citations over assertions**.

### 4.1 Identifying the backlight chip at I²C 0x2C

**Q1.** What backlight controller / display PMIC IC is mounted on the
Samsung Galaxy Book S (SM-W767) main board, addressable via I²C at 7-bit
address `0x2C`? Common candidates at that address: TI **LM36923**,
TI **LP8556**, NXP **PCA9633** (unlikely — that's an LED driver), ROHM
**BD81876**, Richtek **RT8555**, Kinetic **KTZ8866**, Samsung in-house
silicon.

Approaches:
- **FCC ID database** — Samsung filings for SM-W767 may include internal
  photographs that show silkscreen markings. FCC ID candidates start with
  "A3LSMW767" or similar Samsung prefixes.
- **iFixit / YouTube teardowns** of "Galaxy Book S" — capture chip
  silkscreens.
- **Samsung service manual** for SM-W767 — often available on
  service-manual leak sites (gsmhosting, samsungparts), sometimes on
  Samsung support portals for service centres.
- **Windows driver INF files** — the Samsung backlight driver INF
  (`samsungsspn.inf` or similar — search for `SAM0101` in INF text)
  declares the chip class and may name the silicon vendor in the
  description string.
- **Microsoft Update Catalog** — search "Samsung Galaxy Book S" / "W767" /
  "SAM0101" / "panel". Driver packages are downloadable as `.cab`.
- **Samsung's GPL release portal** (opensource.samsung.com) — they are
  required to publish kernel source even for ARM laptops. Search for
  "W767", "SM-W767", "SC8180X", "Galaxy Book S".

**Q2.** Once the chip is identified, does the mainline Linux kernel
already have a driver for it (under `drivers/video/backlight/` or
`drivers/leds/`)? List the compatible string and required DT properties.

### 4.2 The brightness path on Windows

**Q3.** What does the `SAM0101.sys` (or whichever Samsung driver binds
SSPN on Windows) actually do when the user moves the brightness slider?
Specifically:
- Does it write the new brightness directly to I²C 0x2C, or does it write
  `BRLV` in the ACPI op-region (0x9A) and let an ACPI handler call out to
  the EC?
- Is the **enable GPIO 25** driven only at boot/resume, or per-write?
- What is the I²C register layout of the chip at 0x2C? Single-byte write
  (`SMBus write byte`) or register+value pair (`SMBus write byte data`)?
  Knowing this is essential — the Linux driver needs to issue exactly the
  same transaction sequence the Windows driver does.

Approaches:
- **Reverse-engineer the Windows driver** — it lives in
  `C:\Windows\System32\drivers\` (we have a Windows install on the
  device's eUFS that we could re-mount and copy from). Likely names:
  `samsungsspn.sys`, `qcsspanel.sys`, `ssrtsd.sys`. Disassemble with
  IDA / Ghidra / radare2; search for I²C transaction patterns and look
  for the byte sent before any user-controlled value.
- **Bus snooping with Saleae / FX2 logic analyser** on the laptop's
  internal I²C lines — physical access required, board-level work.
- **WPP / ETW logs** — Samsung driver may emit trace events on write
  if WPP enabled.

### 4.3 Has anyone solved this on Linux already?

**Q4.** Is there a working `sc8180x-samsung-w767.dts` (against any kernel
≥ 6.1) anywhere — not necessarily mainlined — where the user can:
(a) see the kernel boot text on the internal display, AND
(b) adjust backlight brightness?

Hunting grounds:
- `git.kernel.org/pub/scm/linux/kernel/git/` — git log --grep + linux-arm-msm
  ML archive on lore.kernel.org for `SM-W767`, `samsung,w767`, `Galaxy Book S`
- **postmarketOS GitLab** — the device tree at
  `gitlab.com/postmarketOS/pmaports/-/tree/master/device/community/`
  search for `samsung-w767` (we know the device exists in pmOS and is
  tagged "screen broken").
- **postmarketOS sc8180x-mainline kernel fork**:
  `gitlab.com/sc8180x-mainline/linux` — branches, MRs, issues.
- **jenneron/linux** GitHub fork — already at the 6.1 era; check newer
  branches if any.
- **Konsta T (konstakang)** — modder active on Galaxy laptops; any GBS
  Linux notes?
- **xda-developers** "Samsung Galaxy Book S Linux / Android" subforums.
- **Telegram groups** for "Snapdragon laptops Linux" / "linux-on-arm-laptop"
  — names of channels and pinned messages.
- **4PDA.to** (Russian) — often has detailed reverse-engineering notes
  for ARM Samsung devices.

**Q5.** Has anyone published a write-up reverse-engineering Samsung's
`SAM0101` ACPI-to-I²C backlight protocol — for ANY Samsung ARM device
(Galaxy Book Go, Galaxy Book S, Galaxy Book2 360, etc.)? Even a partial
trace of one write transaction would unblock our work.

### 4.4 Adreno 680 GPU bind failure

**Q6.** The current GPU bind failure on a 7.0 kernel + Flex-5G-derived
DTS: `a6xx_gmu_init` succeeds, but `adreno_bind` returns `-110
(ETIMEDOUT)` after we explicitly added `power-domains = <&gpucc
GPU_GX_GDSC>` to `&gpu`. Without that override we got `-19 (ENODEV)`.
What is the correct kernel-7.0 binding for an SC8180X GPU node on a
Samsung-style PMIC layout? Is the `pmc8180-c` PMIC arrangement on the
GBS materially different from the Flex 5G in a way that affects GPU
power rails (`vdd`, `vddcx` are reported as "dummy regulator" — non-fatal
on Flex 5G but maybe fatal here)?

Approaches:
- linux-arm-msm patch series for SC8180X GPU (Akhil P Oommen, Konrad
  Dybcio, Bjorn Andersson have all pushed series in the 2024-2026 window).
- Adreno 680 specific commits — there have been GMU init reworks for
  a6xx in kernels 6.6 → 7.0.

**Q7.** Independently of the panel, can we get the DPU pipeline to bring
up `card1` in `simpledrm`-fallback mode (i.e. without GPU bind) — is there
a way to mark `&gpu { status = "disabled"; }` and still let `msm_dpu` come
up, treating the eDP path as a pure framebuffer? Or is the GPU
genuinely required for any `msm_dpu` activity on SC8180X?

### 4.5 Samsung Embedded Controller protocol

**Q8.** Is there a published protocol for the Samsung ARM laptop EC
(SAM0604 / SAFI SAM0701)? Cousin device drivers on x86 Samsung laptops
include `samsung-laptop` and `samsung-galaxybook` (the latter for x86
Galaxy Book Pro/360 series). The ARM side may share the wire protocol or
may be entirely separate.

- `drivers/platform/x86/samsung-galaxybook.c` (Joshua Grisham's mainlined
  driver) — read it; what's the wire format on x86? Is there any
  indication in his commits of an ARM port?

### 4.6 Samsung-extracted firmware

**Q9.** We have firmware blobs extracted from the Windows DriverStore on
the device:

```
qcdxkmsuc8180.mbn          — zap shader, GPU zap
adsp.mbn / adspr.mbn       — Audio DSP
cdsp.mbn / cdspr.mbn       — Compute DSP
mpss.mbn / pdsp_mba.mbn    — Modem subsystem (MBA + MPSS)
slpi.mbn                   — Sensor low-power island (not loaded yet)
btfm.bin / btnv.bin        — Bluetooth
```

These are signed by Samsung's PIL chain (subject CN includes
`SM-W767`-flavoured strings). Question: are these blobs compatible with
mainline kernel 7.0's PIL/PAS drivers, or do we need a particular
firmware revision? The pmOS firmware repository
(`gitlab.com/postmarketOS/firmware`) — does it carry W767-specific blobs?

### 4.7 The "screen blanks after `Starting Fedora 4...`" symptom

**Q10.** On real hardware, kernel + early systemd messages render fine on
the internal panel via the EFI framebuffer / `simpledrm`. Then partway
through systemd starting services, the screen goes dark and never returns.
Plymouth is `plymouth.enable=0`, GDM is masked, default target is
`multi-user.target`, DPMS / `consoleblank=0` is set. What in a typical
Fedora multi-user-target boot would cause `simpledrm` to hand off
ownership of a framebuffer that `msm_dpu` then never claims back?

Candidates to investigate:
- `systemd-vconsole-setup.service` doing a TTY reset that triggers a
  DPMS off
- `kmscon` / `getty@tty1` re-initialising the console in a way that
  `simpledrm` interprets as suspend
- `systemd-backlight@.service` (often a culprit on partly-supported
  laptops — restores `/sys/class/backlight/<x>/brightness` to a saved
  value; if the saved value is 0, BL turns off)
- `systemd-logind` honouring `HandleLidSwitch=suspend` in suspended
  state caused by faulty lid GPIO (do we know what the LID GPIO is on
  SM-W767? Search the DSDT.)
- `systemd-modules-load` loading something that disables `simpledrm`

### 4.8 Schematics, FCC, and primary docs

**Q11.** Does the **FCC equipment authorisation database** have an entry
for SM-W767 with internal photos / block diagrams visible? Provide the
direct fcc.io URL.

**Q12.** Is there a published Samsung service manual for SM-W767 (the
ARM/Qualcomm variant)? — board layout, schematic excerpt, replacement
parts diagram. Search keywords: `SM-W767 service manual`, `Galaxy Book
S Qualcomm schematic`, `W767 board view`.

**Q13.** Are the **GitHub mirrors of opensource.samsung.com** (some are
unofficial) carrying any "Galaxy Book S" / "Galaxy_Book_S_W767" tarball?
The original OSS portal is hard to search; mirrors like
`github.com/SamsungSourceMirror`, `github.com/dasunpubudumal/samsung-os`,
or scraped index sites may surface the tarball faster.

---

## 5. What we are NOT asking the agent to do

- We are not asking for a kernel patch — that's our job once we have the
  data.
- We are not asking for general Linux-on-Snapdragon overviews — we
  already have those (briefs v2-v4). We're past the survey phase.
- We are not interested in Windows driver download links, only in their
  *contents* (specifically, the I²C transaction sequence the Samsung BL
  driver issues).

---

## 6. Reference documents to cross-check against

- Our current DTS:
  `/home/peter/Documents/GalaxyBookS_Linux/dts-stage-v2/sc8180x-samsung-w767.dts`
- pmOS reference DTS (1125 lines, backlight commented out):
  `/home/peter/Documents/GalaxyBookS_Linux/dts-stage-v2/sc8180x-samsung-w767-pmos.dts`
- Decompiled DSDT:
  `/home/peter/Documents/GalaxyBookS_Linux/acpi-decompile/dsdt.dsl`
- Live audit captured via SSH on real hardware (April 2026):
  `/home/peter/Documents/GalaxyBookS_Linux/audit-extracted/audit-20260423-175507/`
  - `42-backlight-list.txt`
  - `43-backlight-all.txt` (BL class device exists, max=4095, doesn't
    actually drive panel)
  - `LIVE-backlight-dance.txt` (script proves writes are accepted but
    have no visible effect)
  - `40-drm-list.txt`, `45-simpledrm.txt`, `46-drm-debug.txt` (only
    simple-framebuffer; no msm card)
  - `10-dmesg.txt`, `75-firmware-dmesg.txt` (PIL load sequence)
- Earlier briefs:
  - `gemini-research-brief-v2.md` (initial port intent)
  - `gemini-research-brief-v3.md` (post-first-boot logs)
  - `gemini-research-brief-v4.md` (pmOS comparison)

---

## 7. Highest-priority asks (TL;DR for the agent)

If time-constrained, prioritise in this order:

1. **Q1**: Identify the silicon at I²C 0x2C / GPIO 25 / GPIO 116 on
   SM-W767. (Hardware identity unblocks everything else.)
2. **Q3**: Get the I²C transaction sequence the Windows `SAM0101` driver
   issues. (Even one captured byte sequence ends this.)
3. **Q4**: Confirm whether anyone — pmOS, jenneron, Linaro, individual
   hackers — has *ever* shown the SM-W767 internal panel lighting up
   under Linux, even briefly. (If yes → trace their setup. If no → we
   are first.)
4. **Q10**: Diagnose the `simpledrm` blank-after-`Starting Fedora 4...`
   symptom independently — it may be solvable without touching DPU /
   backlight at all (e.g. disable systemd-backlight, disable
   getty-tty-reset).
5. **Q9**: Are our Samsung-extracted firmware blobs the right revision?
   (If they're stale, even fixing DTS won't help.)

Everything else is supporting evidence. Don't worry about presenting
findings in any specific format — Markdown with section headers and
inline cited links is fine. **The user (a software engineer with strong
Linux internals knowledge but no schematic-reading background) will read
the entire output and follow up on the most actionable leads.**
