# 08 — Linux Gap Analysis

**The bottom line:** for every Windows-visible device on W767, what mainline Linux driver matches, what compatible string to use, what supplies are needed, what's already in our DTS, what's missing.

**Reading:** by category, ordered from most-likely-to-block-iter-33 down to "future polish."

## Group A — currently blocking iter-32 (USB)

| Device | Linux driver | Compatible | DTS state | Gap |
|---|---|---|---|---|
| URS0 dwc3 (`a600000`) | `dwc3-qcom` + `dwc3` | `qcom,sc8180x-dwc3`, `qcom,dwc3` | declared in DTS, `dr_mode = "host"` set | **iter-32 had `orientation-switch;` on QMP PHY that has no consumer. Branch C removes it.** Q6/Q7 reply, commit `035a854`. |
| URS1 dwc3 (`a800000`) | same | same | same | same as URS0 |
| USB-MP xhci (`a400000`) | `xhci-plat` (via `dwc3` host wrapper) | `qcom,sc8180x-dwc3-mp` | works since iter-28 (KB enumerates) | ✅ done |
| QMP USB3 PHY (each URS) | `phy-qcom-qmp-usb` | `qcom,sc8180x-qmp-usb3-phy` | likely from `sc8180x.dtsi` | check supplies match PEP map: vdda-phy=LDO9_E (0.912V), vdda-pll=LDO3_C (1.2V) |
| USB2 HS PHY (each URS) | `phy-qcom-snps-femto-v2` | `qcom,sc8180x-usb-hs-phy` | likely from DTSI | check supplies: vdda33=LDO16_E (3.0V), vdda18=LDO12_A (1.8V), vdda-pll=LDO5_E (0.88V) |
| Type-C orientation | none usable | — | iter-32 wired `pmic_glink_altmode` but no orientation source exists (see Q6) | **Drop — Branch C** |
| USB-PD controller (I²C 0x09 on IC20) | none in mainline that knows W767's protocol | — | not yet wired | post-USB working: could write a small i2c-driver + typec_switch bridge if PD telemetry is wanted |

**Iter-33 action:** Branch C from Q6/Q7 brief. If still fails, add explicit `vdda-*` properties to the QMP/HS PHY nodes using the rails in [04-pep-vote-map.md](04-pep-vote-map.md).

## Group B — booted but partial (WiFi)

| Device | Linux driver | Compatible | DTS state | Gap |
|---|---|---|---|---|
| WCN3998 WiFi | `ath10k_snoc` | `qcom,wcn3998-wifi` | iter-29 wired with placeholder supplies | needs MPSS QMI handshake to fire. iter-32 reports "probed but no QMI handshake." |
| WCN3990 BT | `hci_qca` over UART | (uart child of qcom,uartn) | not yet wired | needs UART13 (`UR18`, line 70832 in DSDT) + BT power-rails LDO7_A + LDO9_A |
| WiFi power rail | wcn39xx regulator framework | `vdda-supply = <&vreg_l1e_0p8>` | currently using dummy regulator | rewire to LDO1_E (confirmed by Q5 + PEP); keep `vdd-3.3-ch1` dummy (board-side rail, MPSS-managed) |
| MPSS (modem firmware) | `qcom_q6v5_mss` | `qcom,sc8180x-mpss-pas` | iter-31 has MPSS running | ✅ firmware loads; QMI link to ath10k_snoc is the remaining work |

**Iter-33+ action after USB unblocks:** verify QRTR is registered, check `cat /sys/kernel/debug/qrtr/nodes`, confirm wlanfw QMI service appears. If not, MPSS isn't exposing WLAN QMI — that's a Samsung firmware quirk.

## Group C — works since iter-28 (keep working)

| Device | Linux driver | Compatible | DTS state | Status |
|---|---|---|---|---|
| UFS storage (KLUEG8UHDB-C2D1) | `ufshcd-qcom` | `qcom,sc8180x-ufshc`, `qcom,ufshc` | from DTSI | ✅ booted from this since iter-23 |
| eMMC | n/a — no eMMC on W767 | — | — | — |
| µSD slot | `sdhci-msm` | `qcom,sdhci-msm-v5` | `&sdhc_2 { status = "disabled"; };` should be enough — the slot may not be physically populated | check user can `ls /dev/mmcblk*` after iter-33 with a card inserted |
| Internal keyboard MCU | `usbhid` (composite, VID_04E8:PID_A055) | (auto on usb_mp) | ✅ working since iter-28 | — |
| Internal touchpad | same as above (SPACE MCU sub-collection) | (auto) | ✅ iter-28 enumerated but click/scroll quality TBD | needs userspace test |
| TLMM pinctrl | `pinctrl-msm8180x` | `qcom,sc8180x-tlmm` | from DTSI | ✅ working since iter-19 |
| Clock controller (GCC) | `clk-rpmh` + `clk-gcc-sc8180x` | `qcom,sc8180x-gcc`, `qcom,sc8180x-rpmhcc` | from DTSI | ✅ |
| RPMH power domains | `rpmhpd` | `qcom,sc8180x-rpmhpd` | from DTSI | ✅ since iter-22 |
| ADSP (qcadsp8180.mbn) | `qcom_q6v5_adsp` | `qcom,sc8180x-adsp-pas` | iter-31 running | ✅ |
| CDSP (qccdsp8180.mbn) | `qcom_q6v5_pas` | `qcom,sc8180x-cdsp-pas` | iter-31 running | ✅ |
| SMP2P / SMEM / HWSPINLOCK | builtin | n/a | iter-31 fix made these load | ✅ |
| PDC interrupt controller | `irq-qcom-pdc` | `qcom,sc8180x-pdc` | from DTSI | ✅ |
| pmc8180 PMICs | `spmi-pmic-arb` + `pmic_glink` | `qcom,sc8180x-pmic*` | from DTSI | ✅ |

## Group D — wired up, not yet tested

| Device | Linux driver | Compatible | DTS state | Gap |
|---|---|---|---|---|
| Adreno 680 GPU | `msm` (a6xx) | `qcom,adreno-680.0` | declared | needs `vdda-supply = <&vreg_l3c_1p2>` + `vddcx-supply = <&vreg_l9e_0p912>` per PEP. Currently may be using dummy regulators — check dmesg after iter-33. |
| MDSS / display | `msm` (mdss) | `qcom,mdss-dpu-1.0` | declared, eDP works since iter-17 | ✅ but post-iter-33 verify Branch C didn't break |
| eDP panel BOE07E7 | `panel-edp` (auto-DPCD) | `simple-panel` | yes | ✅ since iter-17 |
| WCD9340 audio codec | `snd-soc-wcd9340` | `qcom,wcd9340-codec` | NOT yet wired | needs `&swr0`/`&slim` configured; needs LDO14_E supply per PEP |
| CS35L41 speaker amps × 2 | `snd-soc-cs35l41-spi` | `cirrus,cs35l41` | NOT yet wired | SPI0 CS0+CS1 per DSDT, see `docs/00-hardware-combined.md` headline-corrections § for chip identity (CS35L41 not L40) |

## Group E — wired in DSDT, mainline driver exists, not yet in our DTS

| Device | Linux driver | Compatible | Notes |
|---|---|---|---|
| Rohm BH1733 ambient light | none in mainline | — | Skip; not worth porting downstream |
| Semtech SX9360 SAR ×4 | `sx9360_i2c` (mainline) | `semtech,sx9360` | Easy add: I²C2 with SAR addresses, supplies = LDO10_A |
| QCOM2466 SDHCI host | `sdhci-msm` (above) | yes | Probably unused; check |
| Battery (QCOM0263) | `qcom-charger` family (pm8150b-based) | `qcom,pmi8998-charger` (approx) | Needs PMIC + EmuEC understanding — see docs/02-samsung-platform.md |
| Touchpad ST device (STMT1234) | n/a — touchpad rides USB-HID | — | No I²C-HID path needed |
| EgisTec fingerprint (SAM0909) | none in mainline | — | Skip; no Linux driver exists for this part |
| Front camera + ISP (QCOM0428/0435/etc) | `qcom-camss` | `qcom,sc8180x-camss` | Heavy work; defer to post-MVP |

## Group F — Samsung-specific stubs needed

| Device | Linux driver | Approach |
|---|---|---|
| EmuEC (SAM0604) | new small kernel driver | Split: PMIC ops via existing `spmi-pmic-arb` + a thin `emuec_ec` driver translating Samsung opcodes for charging telemetry only. See `docs/02-samsung-platform.md` for the opcode list. |
| SAFI (SAM0701) | n/a in Linux | Not needed — its UCSI emulator is unused on Linux |
| UCME (SAM0605) | n/a | Not needed — Linux uses native UCSI or none |
| SVBI (SAMM0901) hotkey decoder | new ~250 LoC platform driver | See research/2026-05-17-claude-keyboard-protocol.md for the protocol details |
| Samsung panel driver (SAM0101) | not needed | Mainline `panel-edp` handles BOE07E7 via DPCD |
| EgisTec FP (SAM0909) | skip | — |
| WlSar (SAM0609) wireless SAR | mainline iwlwifi-like userspace | Likely not needed for boot |

## What this dossier does NOT cover

- Boot args (already in `docs/00-hardware-combined.md` headline corrections #2)
- Firmware blob paths (next file: [03-firmware-manifest.md](03-firmware-manifest.md))
- DSDT line citations per device (see `docs/03-bus-and-device-map.md` §10 master table)
- iter-by-iter history (see `research/2026-05-17-*.md` briefs)

## Iter-33 — Linux side cheat sheet

In order of priority:

1. **Apply Branch C** from Q6/Q7 brief — drop `orientation-switch;` from QMP PHYs, drop the pmic-glink connector graph endpoints, keep `dr_mode = "host"`.
2. **Build, flash, reboot.** Both URS dwc3s should bring up host root hubs immediately.
3. **Test USB-C boot drive** on BOTH connectors (left and right — see Q6/Q7 brief footnote on the _PLD discrepancy).
4. **If dwc3 still fails:** check iter-32 dmesg for `dummy regulator` on the QMP/HS PHY rails. If any of `vdda-*` rails show dummy, wire them per the table in Group A above.
5. **Once dwc3 works:** check WiFi via `cat /sys/kernel/debug/qrtr/nodes` — does the wlanfw QMI service appear? If yes, ath10k_snoc should bring `wlan0` up. If no, that's a Samsung MPSS firmware issue.
6. **Once WiFi works (or rule it out):** queue audio (`snd-soc-wcd9340` + `cirrus,cs35l41` SPI children) for iter-34. PEP rails ready: LDO14_E for codec.
