# Deep-research brief v4: Samsung Galaxy Book S on Fedora Rawhide — hybrid DTS ready, pre-boot

**Context**: This is the fourth round of research on getting Linux fully working on
the Samsung Galaxy Book S (SM-W767, Qualcomm SC8180X). Briefs v1–v3 provided the
chronology; v3 captured the live SSH session. **This brief is pre-boot** — we've
just built a custom hybrid DTS based on your (Gemini's) v3 analysis and are
about to flash + boot blind. We want you to look ahead: what's likely to break
next, what's already known to work, and where the best minimum patch set lives.

Your v3 analysis directly informed this round. We added `regulator.ignore_unused`
(your recommendation). We built the hybrid DTS. We removed the `msm` blacklist.
Now validate and extend.

## What we just built (iteration 9, not yet booted)

A **custom `sc8180x-samsung-w767.dtb`** compiled against kernel 7.0 bindings,
derived from mainline `sc8180x-lenovo-flex-5g.dts` with these Samsung-specific
edits:

1. `model = "Samsung Galaxy Book S (SM-W767)"; compatible = "samsung,w767", "qcom,sc8180x";`
2. `&mdss_edp { aux-bus { panel { compatible = "boe,nv133fhm-n61"; ... } } }`
   — replaces generic `"edp-panel"` so the `panel-edp.c` driver uses the
   exact profile for the BOE NV133FHM-N61 (ID 0x07e7), already present in
   mainline kernel 7.0.
3. `&mdss_edp_out { data-lanes = <0 1 2 3>; remote-endpoint = <&panel_in>; };`
   (4-lane eDP, explicit).
4. **Firmware-name paths** for `&gpu_zap_shader`, `&remoteproc_adsp`,
   `&remoteproc_cdsp`, `&remoteproc_mpss` moved from
   `qcom/sc8180x/LENOVO/82AK/*.mbn` to `qcom/samsung/w767/*.mbn`. MPSS file is
   specifically `qcmpss8180_XEF.mbn` (Samsung variant, not Lenovo's `_nm`).
5. Removed `&pcie3` + `&pcie3_phy` + `pcie3_default_state` pinctrl (Samsung
   Galaxy Book S has no PCIe-attached devices).
6. Everything else kept verbatim from mainline Flex 5G DTS (PMIC regulators,
   USB PHYs, pmic-glink, UFS, tlmm pinctrl).

The DTB compiles cleanly against mainline `sc8180x.dtsi` + `sc8180x-pmics.dtsi`
(kernel 7.0). Size 101 KB. Magic `d00dfeed`. Warnings only about QUP duplicate
unit-addresses (present in upstream too, harmless).

## Current kernel cmdline (iteration 9)

```
root=UUID=... rootflags=subvol=root
iommu.passthrough=0 iommu.strict=0 arm-smmu.disable_bypass=0
pcie_aspm.policy=powersupersave
clk_ignore_unused pd_ignore_unused regulator.ignore_unused
arm64.nopauth
systemd.unit=multi-user.target systemd.firstboot=0
systemd.log_level=debug systemd.log_target=kmsg printk.devkmsg=on
log_buf_len=4M initcall_debug keep_bootcon
earlycon=efifb
reserve_mem=2M:4096:oops ramoops.mem_name=oops ramoops.record_size=0x4000 ramoops.console_size=0x4000
```

Notable changes from v3's captured cmdline:
- **Removed** `modprobe.blacklist=msm` — with the Samsung DTS, we want msm to
  bind. v3's experiment with force-loading msm live produced the
  `failed to bind 2c00000.gpu: -19` trace we reported.
- **Added** `regulator.ignore_unused` — per your (Gemini's) v3 root-cause
  hypothesis: the late-boot regulator shutdown may be what turns off the panel
  even when brightness/bl_power look healthy.

## The v3 state we inherit (unchanged since the SSH session ended)

- `/etc/machine-id` = real UUID, `systemd.firstboot=0` — no interactive block.
- Firmware blobs present at BOTH
  `/usr/lib/firmware/qcom/samsung/w767/*` (30 files inc. JSON service maps)
  AND `/usr/lib/firmware/qcom/sc8180x/LENOVO/82AK/*` (same files mirrored).
  a680_sqe.fw + a680_gmu.bin at `/usr/lib/firmware/qcom/`.
- `sshd` enabled, root password `galaxy`, autologin drop-ins for tty1..4.
- `early-dmesg.service` wired to `sysinit.target.wants` — writes dmesg to
  `/boot/efi/early_dmesg.txt` on boot.
- `gbs-ip-report.service` wired to `multi-user.target.wants` — writes IP info
  to ESP.
- Persistent journal (`Storage=persistent` in
  `/etc/systemd/journald.conf.d/persistent.conf`) — disk-logs from boot #1.
- No ethernet dongle this round (user moved the device); blind boot, then
  USB back to the Fedora build host for post-mortem.

## Key symptoms from v3's live SSH session (for reference)

Live-loaded msm sequence captured over SSH:
```
[1749.239] calling panel_edp_init @ 3633
[1749.283] panel-simple-dp-aux aux-ae9a000.displayport-controller: supply power not found, using dummy regulator
[1749.312] WARNING: drivers/gpu/drm/panel/panel-edp.c:814 at generic_edp_panel_probe+0x13c/0x280
[1749.792] panel-simple-dp-aux aux-ae9a000.displayport-controller: Unknown panel BOE 0x07e7, using conservative timings
[1749.806] msm_dpu ae01000.display-controller: bound ae90000.displayport-controller (ops msm_dp_display_comp_ops [msm])
[1749.818] msm_dpu ae01000.display-controller: bound ae98000.displayport-controller (ops msm_dp_display_comp_ops [msm])
[1749.828] msm_dpu ae01000.display-controller: bound ae9a000.displayport-controller (ops msm_dp_display_comp_ops [msm])
[1749.839] adreno 2c00000.gpu: supply vdd not found, using dummy regulator
[1749.848] adreno 2c00000.gpu: supply vddcx not found, using dummy regulator
[1749.920] msm_dpu ae01000.display-controller: failed to load adreno gpu
[1749.929] msm_dpu ae01000.display-controller: failed to bind 2c00000.gpu (ops a3xx_ops [msm]): -19
```

remoteproc states (after boot with Flex 5G DTB + LENOVO firmware path):
```
remoteproc0 modem: state=offline firmware=qcom/sc8180x/LENOVO/82AK/qcmpss8180_nm.mbn
remoteproc1 cdsp:  state=running firmware=qcom/sc8180x/LENOVO/82AK/qccdsp8180.mbn
remoteproc2 adsp:  state=offline firmware=qcom/sc8180x/LENOVO/82AK/qcadsp8180.mbn
```

CDSP loads the Samsung blob (mirrored to Lenovo path) OK. ADSP and MPSS do
not. **Only CDSP powers up; that's a key asymmetry that we don't yet
understand.** With iteration 9's Samsung DTS, firmware-name for all three is
now `qcom/samsung/w767/qc{adsp,cdsp,mpss}8180.mbn` — we'll see if the same
asymmetry persists.

pmic-glink warnings from v3:
```
qcom_pmic_glink pmic-glink: Failed to create device link (0x180) with supplier usbprim-sbu-mux for /pmic-glink/connector@0
qcom_pmic_glink pmic-glink: Failed to create device link (0x180) with supplier usbsec-sbu-mux for /pmic-glink/connector@1
qcom_pmic_glink pmic-glink: Failed to create device link (0x180) with supplier 88e8000.phy for /pmic-glink/connector@0
qcom_pmic_glink pmic-glink: Failed to create device link (0x180) with supplier 88ed000.phy for /pmic-glink/connector@1
qcom_pmic_glink pmic-glink: Failed to create device link (0x180) with supplier a600000.usb for /pmic-glink/connector@0
qcom_pmic_glink pmic-glink: Failed to create device link (0x180) with supplier a800000.usb for /pmic-glink/connector@1
synth uevent: /devices/platform/pmic-glink/pmic_glink.power-supply.0/power_supply/qcom-battmgr-bat: failed to send uevent
```

Your v3 analysis flagged these as potentially **fatal** for battery/backlight
orchestration — not benign as we'd initially treated them. We're inheriting
Flex 5G's `orientation-gpios = <&tlmm 38 GPIO_ACTIVE_HIGH>, <&tlmm 58 GPIO_ACTIVE_HIGH>;`
and the SBU mux pin assignments (`gpio152/100` + `gpio188/187`) — these are
Lenovo's; Samsung's may differ.

## What we want from Gemini, specifically

### A. Validate or refine the backlight / display-subsystem hypothesis

With the new DTS (proper `boe,nv133fhm-n61` + firmware paths + msm not
blacklisted) **and** `regulator.ignore_unused`, does anything *else* have to
be set to get the Samsung Galaxy Book S internal eDP panel to light up?

Candidates we haven't tried:
1. **`sysfb_apply_efi_quirks` regression**: you mentioned this in v3 as a
   kernel 6.1.23+ regression. Is there a concrete patch/commit reverting or
   fixing it that applies to 7.0? Any cmdline workaround?
2. **Panel-specific `power-supply` at the panel node** — the live dmesg
   showed `panel-simple-dp-aux: supply power not found, using dummy
   regulator`. Which PMIC rail should that reference on Samsung hardware?
   Flex 5G DTS doesn't specify a `power-supply` on the panel node at all —
   is that a bug, or fine? What does jhovold's X13s DTS do?
3. **eDP PHY supplies** — `&mdss_edp` on Flex 5G has pinctrl but no
   `vdda-phy-supply`/`vdda-pll-supply`. The audit showed `qcom-edp-phy:
   supply vdda-phy not found, using dummy regulator`. Which PMIC rail?
4. **Backlight `enable-gpios` polarity/pin**. Flex 5G uses
   `<&pmc8180c_gpios 8 GPIO_ACTIVE_HIGH>`. Is Samsung's also pmc8180c_gpios
   pin 8, or a different pin? The audit's Windows PnP extraction showed
   Samsung specific GPIO references — can you correlate?

### B. Root-cause the `failed to bind 2c00000.gpu: -19` under mainline Flex 5G DTS

We want a confident answer before iteration 9 fails in case it persists. Our
current theories (rank these):
1. Zap-shader firmware signature mismatch. Samsung's
   `qcdxkmsuc8180.mbn` Windows blob is signed against Samsung hardware hashes.
   Under Lenovo DTS path it was loaded but maybe the GPU's PAS still rejected
   it. Under Samsung DTS path it's now at the "right" Samsung path — does
   this matter, or is the signed content the only thing that matters?
2. GMU (`gmu@2c6a000`) not probing first, causing a phantom `-ENODEV` when
   GPU's `qcom,gmu = <&gmu>` reference resolves to a device that hasn't got a
   driver. Is there an ordering property we need?
3. A6xx driver in kernel 7.0 expects a specific new DT property (maybe
   `power-domains` added in a 6.8+ patch) that's missing from Flex 5G DTS.
4. Something else entirely.

Please find a concrete working reference. Any git tree (jhovold, dylangeraci,
linux-postmarketos-qcom-sc8180x, etc.) that demonstrably lights up the panel
on SC8180X on kernel ≥6.8 — with links.

### C. ADSP / MPSS "offline" while CDSP runs — signature verification asymmetry?

All three remoteprocs use the same `qcom,q6v5-pas` driver and the same
Samsung Windows blobs (mirrored to Lenovo path). CDSP runs; ADSP and MPSS
don't. Why the asymmetry?

Specific asks:
- Does `qcmpss8180_XEF.mbn` (Samsung-signed MPSS) load and authenticate under
  a Lenovo-configured `remoteproc_mpss` node? Is there an OEM hash burned
  into QFPROM that pas-loader checks against, and if so, can we read it
  (at sysfs? at debugfs? via `qcom_scm`?)?
- If the asymmetry is PAS-signature-related, is there a way to strip+re-sign
  the blobs with a dev-signed key the kernel will accept? `pil-signer` tool?
- Alternative: is there a "no-auth" mode (CONFIG_QCOM_MDT_NO_AUTH or similar)
  that skips authentication entirely for dev kernels? Fedora's 7.0 kernel
  config — does it enable signature checks for peripheral images?

### D. The pmic-glink 0x180 errors: are they fatal?

Your v3 said yes. What does the fix look like? Is it:
- A missing DT property on the connector nodes?
- A missing `gpio_sbu_mux` node that Samsung needs that Flex 5G doesn't?
- The GPIO pins being wrong for Samsung (hardware trace required to confirm)?
- Or is the whole pmic-glink needing a DT-level disable until we can wire it
  correctly, with a workaround pinctrl setting for the SBU muxes meanwhile?

### E. Wi-Fi (ath10k_snoc) probe race with ADSP

In v3 we observed `ath10k_snoc` loads but no `wlan0` registers. ADSP is
offline in that state — ADSP hosts the QMI services ath10k needs. If we fix
ADSP authentication (see C), does Wi-Fi automatically start working?

Is there a known DT pattern that expresses the dependency, e.g.:
```
&wifi {
    qcom,rproc = <&remoteproc_adsp>;
    // or
    power-domains = <&rpmhpd SC8180X_CX>, <&remoteproc_adsp SERVICE_LOCATOR>;
};
```
… or does it need to be handled at the driver level?

### F. A question about post-boot behavior

Assume iteration 9 boots + display lights up. The first goal after that is
getting Wayland/GNOME running. Does kernel 7.0 + Samsung DTB + a680_sqe /
a680_gmu firmware support hardware-accelerated rendering on the Adreno 680?
Any known mainline-kernel blockers for `freedreno` / Mesa on SC8180X in 2026?

## The hybrid DTS source (for you to cross-reference)

Staged at `/home/peter/Documents/GalaxyBookS_Linux/dts-stage-v2/sc8180x-samsung-w767.dts` (797 lines). It inherits nearly verbatim
from `arch/arm64/boot/dts/qcom/sc8180x-lenovo-flex-5g.dts` in torvalds master
as of April 2026. Only the node-level deltas listed above differ.

## Desired output from this round

- **Concrete pointer to a working reference tree** for SC8180X + BOE panel +
  mainline-ish kernel. Any fork on GitHub/GitLab/Codeberg. Commit hash + file
  path.
- **If no such tree exists**, confirm explicitly so we stop searching.
- **Minimal additional DT properties** we should add for iteration 10 if
  iteration 9 still blanks. Ranked by likelihood of helping.
- **A working recipe (or definitive "this is not achievable on mainline")**
  for the ADSP/MPSS remoteproc authentication path with Samsung-signed
  firmware on a kernel booted with mainline-style DTS.

## What we're NOT trying to figure out this round

- Audio (Realtek ALC298 + discrete amps).
- Full Wayland/Mesa acceleration (deferred to after display lights up).
- External DisplayPort-over-USB-C.
- Battery fuel-gauge accuracy.
- Fingerprint reader.
- LTE modem operation (MPSS comes first).

## Artefacts already on the build host (for context)

- `audit-20260423-175507.tar.gz` — 1 MB, 64-file tarball from a full live
  on-device audit (SSH session).
- `dts-stage-v2/sc8180x-samsung-w767.dts` — the new DTS.
- `dts-stage-v2/sc8180x-samsung-w767.dtb` — compiled output, 101 KB.
- `mainline-dts/*` — all mainline DTSI/DTS files we reference.
- `firmware-stage/` + injected into image — 100 MB of Samsung-extracted
  Qualcomm blobs (GPU, DSPs, modem, Wi-Fi, BT) + pmOS jenneron fork
  (including JSON service maps).
- `win-extract/` — UTF-8'd Windows PnP dump with full device topology,
  resource-map (interrupt + memory assignments), HID interfaces.
- `imgages/fedora-gbs.raw` — 13 GB Fedora Rawhide image fully configured for
  iteration 9.

All already staged on the build host; no additional extraction needed — the
question is what to look for and what to patch.
