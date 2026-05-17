# iter-34: QMP PHY supply swap + MPSS sysfs auto-start

**Date:** 2026-05-17 (very late)
**Status:** built, awaiting boot
**Based on:** brother's dossier `recon/` (commits `f96d604`/`f49ebe6`/`379dea2`) + kernel source spelunking

## Two surgical fixes

### Fix 1 — QMP USB PHY supplies were SWAPPED and on wrong rail

`recon/04-pep-vote-map.md` documents Windows' PEP votes for `\_SB.URS0.USB0` and `\_SB.URS1.USB1`: both controllers want **LDO9_E (0.912 V)** for `vdda-phy` and **LDO3_C (1.2 V)** for `vdda-pll`.

Our DTS had:
- `vdda-phy = vreg_l3c_1p2` (1.2 V — wrong rail entirely, that's the PLL rail)
- `vdda-pll = vreg_l5e_0p88` (0.88 V — wrong rail; LDO5_E is the digital CX/MX rail, not PLL)

So we had the rails swapped AND `vdda-phy` on the wrong rail. Without the correct analog supply, the QMP USB PHY can't bring up its PLL → `phy_init` defers indefinitely → `dwc3_core_init` returns the "failed to initialize core" we've been chasing since iter-31.

`vreg_l9e_0p88`'s `regulator-min/max` is already 880000/912000 µV, so it can supply 912 mV when consumed by `vdda-phy`. No constraint change needed; just point the supplies right:

```dts
&usb_prim_qmpphy {
    vdda-phy-supply = <&vreg_l9e_0p88>;  /* was vreg_l3c_1p2 */
    vdda-pll-supply = <&vreg_l3c_1p2>;   /* was vreg_l5e_0p88 */
};
&usb_sec_qmpphy {
    vdda-phy-supply = <&vreg_l9e_0p88>;  /* same fix */
    vdda-pll-supply = <&vreg_l3c_1p2>;
};
```

DTB grew 83412 → 83428 bytes (16 B from new phandle refs).

### Fix 2 — MPSS doesn't auto-boot; kick it from /init

`drivers/remoteproc/qcom_q6v5_pas.c` hardcodes `sc8180x_mpss_resource.auto_boot = false` (line 1274). Unlike ADSP/CDSP which auto-boot, MPSS stays `offline` forever unless something writes `start` to `/sys/class/remoteproc/N/state`.

W767 routes WLAN QMI services through MPSS (per `recon/02-pnp-drivers.md`: qcwlan is enumerated as a QCMS child of QCOM041E modem subsystem). So `wlan0` literally cannot appear until MPSS boots.

iter-34 init adds a background loop that polls remoteproc class for the one named "modem", and writes `start` once it sees `state=offline`:

```sh
(
    for i in $(seq 1 30); do
        sleep 1
        for rp in /sys/class/remoteproc/*/; do
            [ -d "$rp" ] || continue
            if [ "$(cat "$rp/name" 2>/dev/null)" = "modem" ]; then
                state=$(cat "$rp/state" 2>/dev/null)
                if [ "$state" = "offline" ]; then
                    echo start > "$rp/state" 2>/dev/null
                    log "iter-34: kicked MPSS at $(basename "$rp")"
                    exit 0
                fi
            fi
        done
    done
) &
```

## Verified during prep, not changed

- **Wifi compatible stays `qcom,wcn3990-wifi`** — `ath10k_snoc` driver's `of_match_table` (line 1717 of `snoc.c`) only matches `wcn3990`. Brother's dossier suggested overriding to `wcn3998-wifi` but the kernel driver wouldn't bind. Chip is physically WCN3998 but the binding/firmware are reused from WCN3990.
- **`vdd-0.8-cx-mx-supply = vreg_l1e_0p75`** for WiFi is correct (752 mV matches PEP's `0x000B7980` vote for LDO1_E).
- **HS PHY supplies** (`vdda-pll = vreg_l5e_0p88`, `vdda18 = vreg_l12a_1p8`, `vdda33 = vreg_l16e_3p0`) were already correct.

## Expected outcome

| Per-frame display | Means |
|---|---|
| `remoteproc0 (modem): running` (was `offline`) | MPSS auto-start kicked, firmware loaded |
| Three USB buses (`usb1`+`usb2`+`usb3`) or `/dev/sda` on either USB-C port | QMP PHY supplies correct → dwc3 cores up |
| `wlan0` appears in NET | MPSS QMI service announces wlanfw → ath10k_snoc binds |
| Two of three boxes lit | Big progress; we know which iter-35 problem to attack |
| None | Either the PEP map I'm reading wrong, or MPSS firmware (qcmpss8180_XEF.mbn) doesn't load (some W767-specific quirk) |

## What's NOT in iter-34

- No kernel config changes (config from iter-31 stays)
- No firmware additions (MPSS firmware already staged in iter-33)
- No initramfs additions beyond the MPSS auto-start hook
- The `diag` script is still there for one-shot shell-side dump

## Files

- DTS: `dts/sc8180x-samsung-w767.dts` (QMP PHY supply blocks)
- Initramfs: `w767-os/initramfs/layout-iter34/init` (MPSS kick) + `bin/diag`
- Image: `/tmp/w767-iter34.img` (local only, 784 MB)
- Dossier sources: `recon/04-pep-vote-map.md`, `recon/08-linux-gap.md`, kernel `qcom_q6v5_pas.c:1270`
