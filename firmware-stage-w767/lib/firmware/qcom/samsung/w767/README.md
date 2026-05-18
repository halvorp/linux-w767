# W767 Windows-extracted firmware blobs — inventory

All blobs were extracted from `C:\Windows\System32\DriverStore\FileRepository\`
on a W767 (Samsung Galaxy Book S, sc8180x / Snapdragon 8cx Gen 1) running
Windows 11 ARM64. Each subdirectory tracks one functional area. When two
DriverStore versions of the same driver were present, the newer one was
selected (based on `LastWriteTime`).

## Tree

```
qcom/samsung/w767/
├── wifi/   — 11× bdwlan*, data.msc                       [iter-47]
├── gpu/    — qcdxkmsuc8180.mbn, qcvss8180.mbn           [this commit]
├── bt/     — crbtfw21.tlv, 7× crnv21.*                  [this commit]
├── pm/     — SPMD.bin                                    [this commit]
└── README.md
```

## wifi/ — Qualcomm WCN3998 / ath10k_snoc

Already documented in `research/2026-05-18-claude-iter47-wifi-board-data.md`.
Source: `qcwlan8180.inf_arm64_59a3e6c522523a44`.

## gpu/ — Adreno 680 / Venus video subsystem

Source: `qcdx8180.inf_arm64_c1c5f5f4255a7d2a` (signed 2021-10-11; supersedes
`a6eeb0588aa111aa` from 2020-05-07). INF class = `Display`, friendly name
"Qualcomm(R) Adreno(TM) 680 GPU".

| File | Size | SHA-256 (head) | Identity |
|---|---:|---|---|
| `qcdxkmsuc8180.mbn` | 14 240 | `7bc9035fc38bcdc4…` | "QcDx Kernel-Mode-Setup uC" — Hexagon ELF + Qualcomm CASS-SBL3 cert chain. Small auxiliary micro-controller firmware for the Adreno display path. NOT the same as Linux mainline `a630_gmu.bin` (which is ~100 KB). Likely a DPU helper uC. |
| `qcvss8180.mbn` | 1 159 200 | `f998434fbbb1930b…` | "QC Video SubSystem" = **Venus video codec firmware**. Hexagon ELF, signed. This is the analog of mainline Linux's `qcom/venus.mbn` / `qcom/sc8180x/venus.mbn`. Required when `qcom,sc8180x-venus` compatible binds. |

**Linux notes:**
- Adreno 680 mainline expects `a630_*` (or `a640_*`/`a680_*`) firmware files
  pulled from `linux-firmware.git`. The Windows blobs above do **not** drop
  in directly — they're signed for the Windows driver's hash chain.
- Venus: the singular `qcvss8180.mbn` can likely be split into `venus.b00`-`venus.b07`
  via `dd`/mkbootimg-style ELF segment extraction (each PT_LOAD becomes one
  `bNN` file) and placed at `/lib/firmware/qcom/sc8180x/`. Worth testing if
  iter-47 ever needs video acceleration; pure display works without Venus.

## bt/ — Bluetooth (WCN3998 BT side, qcbtfmuart8180)

Source: `qcbtfmuart8180.inf_arm64_ba0b068654fae2c1` (signed 2021-06-01;
supersedes `96d9d71023361ae6` from 2019-12-06). UART-attached BT — the BT
side of WCN3998 talks to the SoC via a high-speed UART (`qcom,qca-bt`).

| File | Size | SHA-256 (head) | Role |
|---|---:|---|---|
| `crbtfw21.tlv`  | 229 860 | `337ff0656ffe6619…` | BT firmware patch, QCA TLV format. Header `01 e0 81 03` is standard QCA patch-info TLV. Direct Linux mapping: `/lib/firmware/qca/crbtfw21.tlv`. |
| `crnv21.bin`    |  4 710 | `b803e66675b06e4a…` | NVM init blob, default variant. |
| `crnv21.b3c`    |  4 746 | `2fc8074088c2b696…` | NVM variant (b3c == b45). |
| `crnv21.b44`    |  4 814 | `feba7f6e85de1e1e…` | NVM variant (b44 == b46 == b47 == b71). |
| `crnv21.b45`    |  4 746 | `2fc8074088c2b696…` | (dup of b3c) |
| `crnv21.b46`    |  4 814 | `feba7f6e85de1e1e…` | (dup of b44) |
| `crnv21.b47`    |  4 814 | `feba7f6e85de1e1e…` | (dup of b44) |
| `crnv21.b71`    |  4 814 | `feba7f6e85de1e1e…` | (dup of b44) |

Of the seven NVM names, there are only **three distinct payloads** — most
extensions are aliases the driver may resolve by board ID. `crnv21.bin` is
the catch-all default.

**Linux notes:**
- Default mainline path: `qca/crbtfw21.tlv` + `qca/crnv21.bin`. Drop both
  in and the `hci_qca` driver should bind.
- If hci_qca complains about wrong NVM size, try replacing `crnv21.bin`
  with one of `b3c/b44/b45/b46/b47/b71` — same selection problem WLAN has.
- Like WLAN, **no static board→file mapping** exists in the INF — the BT
  driver picks at runtime via HCI vendor command.

## pm/ — Power management

Source: `qcsubsys_ext_scss8180.inf_arm64_dcf9b1c49cd7b2fe` (the SCSS = Sensor
& Compute Sub-System subsystem extension; also serves SLPI).

| File | Size | SHA-256 (head) | Role |
|---|---:|---|---|
| `SPMD.bin` | 1 562 | `d2a421ef6365684b…` | Magic `41 65 6f 42` ("AeoB"). Contains ASCII string `\DEVICE` in the header. Subsystem Power-Management Daemon configuration blob — read by `qcsubsys.sys` at SLPI bring-up. Tiny static config, no signed payload. |

**No other PM firmware exists in DriverStore.** Specifically absent:
- No `rpmh.mbn` / RPMh micro-controller firmware — RPMh runs from on-die ROM
  loaded by the bootloader before any OS starts.
- No `aop.mbn` / Always-On-Processor firmware — same as RPMh; AOP is loaded
  by ABL before kernel handoff.
- No PEP firmware blob — `qcpep.wd8180.inf` ships only a `.sys` driver; the
  PEP "votes" it issues are programmed into the driver itself, not into a
  separate firmware file. Brother already captured the vote table in
  `recon/04-pep-vote-map.md`.

`SPMD.bin` is genuinely the only file-based PM payload on Windows.

## Categories deliberately NOT staged

Captured but excluded from this commit because they were extraneous to the
GPU/BT/Kbd/PM ask, or already captured elsewhere:

- `qcsubsys_ext_adsp8180\qcadsp8180.mbn` (11 MB) — ADSP firmware (brother
  already running, but staging it could improve consistency)
- `qcsubsys_ext_cdsp8180\qccdsp8180.mbn` (3.1 MB) — CDSP firmware
- `qcsubsys_ext_mpss8180\qcmpss8180_XEF.mbn` (78.5 MB) — full MPSS firmware
  (brother's `wlanmdsp.mbn` is a subset of this)
- `qcsubsys_ext_scss8180\qcslpi8180.mbn` (6.7 MB) — SLPI firmware
- `qcauddev8180_ss\qcwdsp8180.mbn` (2.1 MB) — WCD audio codec firmware
- `qcipa8180\ipa_fws.elf` (37 KB) — IPA microcode
- `qccamisp8180\CAMERA_ICP_AAAAAA.elf` (3.6 MB) — Camera ISP firmware
- `qctree8180\*.mbn` — HDCP / PlayReady DRM blobs (not useful for Linux)

If brother needs any of these, ask and they get staged on the next round.

## What does NOT exist as a firmware blob on Windows

The "missing" categories for completeness, with the reason there is nothing
to stage:

- **Keyboard / touchpad firmware.** W767's keyboard is `ACPI\SAMM0901`
  routed through `ACPI\SAM0604` (Samsung EmuEC, the opcode-translating
  embedded controller). The EmuEC firmware lives in the MCU's internal SPI
  flash and is never re-loaded from the OS. The touchpad is
  `ACPI\VEN_STMT&DEV_1234&SUBSYS_C17C144D` — STMicroelectronics I²C HID,
  standard HID-over-I²C, no firmware blob (descriptor pulled at probe time).
  Samsung's `SAM0701` ("Samsung Firmware Interface") and `safidrv.inf` ship
  only a managed-code `Samsung.Firmware.dll` (Samsung's update agent), not
  device firmware.
- **RPMh / AOP / PEP firmware** — see PM section above.
- **GPU shader firmware** (SQE/CPC/GMU equivalents on Adreno) — embedded
  inside `qcdxgkrnl8180.sys` as resources, not extractable as standalone
  files. Linux mainline uses the open-source equivalents from
  `linux-firmware.git`.
