# 2026-05-17 — iter-21 boot result: first successful boot, PSCI cpuidle missing

**Author:** Claude (Opus 4.7) on the Linux host
**Photos:** see message thread (two phone shots of the GBS screen during the boot)
**Outcome:** kernel + initramfs + shell all running; **all platform devices stuck in deferred-probe** because PSCI cpuidle stack is disabled in our config

## What worked (first time across all iter-N attempts)

1. **EFI handoff** — `BOOTAA64.EFI` (systemd-bootaa64) loaded from the USB ESP, menu appeared, entry auto-selected after timeout
2. **earlycon=efifb** — kernel printed to the UEFI GOP framebuffer from the very first message, visible on the GBS panel
3. **GOP → simpledrm handoff** — `SYSFB=y + DRM_SIMPLEDRM=y + FB_EFI=y` (iter-20) preserved the framebuffer across `ExitBootServices()`, no black gap before drm_msm could probe
4. **ramoops registered** — first dmesg line shows `OF: reserved mem: 0x000000009b500000..0x000000009b5fffff,(1024 KiB) tmap non-reusable ramoops@9b500000`
5. **Initramfs ran** — `/init` mounted /proc /sys /dev, printed the iter-21 diagnostic banner, dropped to busybox shell on tty0
6. **All cmdline quirks took effect** — `earlycon=efifb keep_bootcon clk_ignore_unused pd_ignore_unused arm64.nopauth efi=noruntime iommu.passthrough=0 iommu.strict=0 pcie_aspm.policy=powersupersave` all echoed in kernel banner
7. **Shell came up** — `[w767 ~]#` prompt visible on screen
8. **usbcore + usbhid registered** — the USB *interface drivers* came up (just not any host controllers, see below)

The phone shots show the banner literally complete with `Useful next commands:` (`save-logs`, `cat /sys/fs/pstore/*`, `i2cdetect -y 1`, `poweroff -f`, `reboot -f`) printed to tty0.

## What didn't work and why

After the shell prompt, dmesg flooded with `deferred probe pending` for essentially every platform device on the SoC. The chain (from observed timestamps ~15.4–15.7 s):

```
usb @ a4f8800,a6f8800,a8f8800 ──┐
ufs @ 1d84000 ──────────────────┤
remoteproc @ 4080000, 8300000, 17300000 ──┤  (ADSP, CDSP, MPSS)
mdss @ ae00000 ─────────────────┤
gpu @ 2c00000 ──────────────────┤
geniqup @ 8c0000, ac0000, cc0000 ─┤   (QUP buses)
                                  │
                                  ▼
                          1500000.interconnect ─── (reason unknown)
                                  │
                                  ▼
                          100000.clock-controller ─── wait for supplier
                                  │                   /soc@0/rsc@18200000/
                                  │                   power-controller
                                  ▼
                       rpmhpd @ rsc/power-controller
                                  │
                                  ▼
                          18200000.rsc ─── wait for supplier
                                  │         /psci/power-domain-cpu-cluster0
                                  ▼
                          ❌ PSCI cluster0 PD ── NEVER REGISTERS
```

Verification (built config from this branch):

```
CONFIG_ARM_PSCI_FW=y                    ✓
CONFIG_CPU_IDLE                         # not set     ← ROOT CAUSE
CONFIG_PSCI_CPUIDLE_DOMAIN              <MISSING>
CONFIG_ARM_PSCI_CPUIDLE                 <MISSING>
CONFIG_ARM_PSCI_CPUIDLE_DOMAIN          <MISSING>
CONFIG_DT_IDLE_GENPD                    <MISSING>
CONFIG_DT_IDLE_STATES                   <MISSING>
CONFIG_ARM_CPUIDLE                      <MISSING>
CONFIG_QCOM_RPMHPD=y                    ✓ (correctly enabled, but its
                                            cluster_pd parent never
                                            registers without the above)
```

`sc8180x.dtsi:548–574` declares `cpu_pd0`..`cpu_pd7` each with
`power-domains = <&cluster_pd>`, and `cluster_pd` is the OSI-mode genpd
provider that **requires `CONFIG_ARM_PSCI_CPUIDLE_DOMAIN` to register itself**.
Without it, the rest of the platform device tree can't find its supplier
and stays in deferred-probe limbo. Same class of silent-config-drop bug
as iter-19 (`PM=y` was missing → `DRM_MSM` silently dropped).

The boot reaching the shell despite this is because:
- The initramfs runs entirely from RAM, doesn't need block I/O
- earlycon=efifb owns the framebuffer directly via UEFI's GOP, doesn't need clocks/regulators
- The minimal busybox /init only touches /proc/sys/dev tmpfs mounts

So we got a "live" view of a kernel that booted successfully but failed to bring up any actual hardware. Auto-snapshot to ESP didn't write because USB never enumerated (USB host controllers are in the deferred-probe chain).

## What this round proved (independent of the bug)

- iter-20's `SYSFB + DRM_SIMPLEDRM + FB_EFI` fix is correct: no black-screen gap between EFI handoff and DRM probe
- iter-21's `ramoops` reserved-memory node binds correctly: `OF: reserved mem: ... ramoops@9b500000`
- iter-21's `earlycon=efifb keep_bootcon` works on this firmware
- iter-21's `I2C_HID=y` doesn't cause boot issues (it's built in, just hasn't had a bus to probe on yet)
- The whole boot pipeline — bootloader → kernel → initramfs → shell — is healthy

This is the first time we have **direct visual ground truth** for a custom-kernel boot on the W767.

## iter-22 fix

Add the PSCI cpuidle stack to `w767-initramfs.config`:

```
CONFIG_CPU_IDLE=y
CONFIG_CPU_IDLE_GOV_MENU=y
CONFIG_DT_IDLE_STATES=y
CONFIG_DT_IDLE_GENPD=y
CONFIG_ARM_CPUIDLE=y
CONFIG_ARM_PSCI_CPUIDLE=y
CONFIG_ARM_PSCI_CPUIDLE_DOMAIN=y
```

`ARM_PSCI_CPUIDLE_DOMAIN` is the one that actually registers `cluster_pd` as
a genpd provider. The others are dependencies (CPU_IDLE master + governor +
DT idle-states parser + DT genpd integration + ARM cpuidle base + PSCI
cpuidle driver).

Expected iter-22 behavior:
- `cluster_pd` registers
- `&rpmh_rsc @ 0x18200000` probes
- `&rpmhpd` probes
- All the pmc8180-{a,c}-rpmh-regulators bind
- 100000.clock-controller probes
- Interconnects come up
- USB host controllers probe → `/dev/sda1` appears → ESP auto-snapshot writes
- UFS probes (irrelevant for USB-boot but tests that branch)
- DRM_MSM finishes probing → console moves from earlycon/simpledrm to full eDP
- I2C controllers come up → touchpad@49 has a chance to probe → HID device appears

If iter-22 boots and the auto-snapshot lands on the ESP, we'll have the full
boot-time dmesg + the touchpad enumeration result + the pstore content (if any)
all written to the USB stick for off-device analysis.

## Lessons

1. The "allnoconfig + small merge fragment" build approach keeps biting us on
   silent config drops. Three rounds of this:
   - iter-19: `PM=y` missing → DRM_MSM silently dropped → no display
   - iter-20: `SYSFB`/`DRM_SIMPLEDRM`/`FB_EFI` missing → no GOP handover
   - iter-21: `CPU_IDLE`/PSCI cpuidle missing → no `cluster_pd` → deferred-probe cascade
   At some point switching to `make defconfig` as the base (Fedora-class
   ~1500 flags) and overlaying only board-specific things would amortize
   the discovery cost. Tradeoff: kernel image grows from 19 MB to ~30-40 MB.

2. earlycon=efifb is the killer feature for laptop bring-up. Should have
   been on since iter-19.

3. ramoops works exactly as advertised — even though we didn't need it
   this round (the kernel got far enough that earlycon told us everything),
   it's there for the rounds where it doesn't.

4. The auto-snapshot-to-ESP idea is sound *in principle* but fails when
   the failure mode is "USB never enumerates". A future enhancement worth
   considering: write logs to ramoops's pmsg buffer directly (via
   `/dev/pmsg0` — which doesn't require any block I/O) so the boot
   snapshot survives even when no storage probes.
