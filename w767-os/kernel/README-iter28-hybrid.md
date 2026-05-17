# iter-28-fedora-hybrid.config

Hybrid kernel config: Fedora 7.0.0-62.fc45 aarch64 base + targeted flips
for the W767 iter-28 USB-MP probe instrumentation experiment.

## How to reproduce the build

```bash
# 1. Download Fedora kernel-devel for the .config
curl -fsSL -O 'https://kojipkgs.fedoraproject.org/packages/kernel/7.0.0/62.fc45/aarch64/kernel-devel-7.0.0-62.fc45.aarch64.rpm'
rpm2cpio kernel-devel-7.0.0-62.fc45.aarch64.rpm | cpio -idmv

# 2. Use that .config as starting point in your Linux 7.0 tree
cd /path/to/linux-v7.0
cp /path/to/kernel-devel-extract/usr/src/kernels/7.0.0-62.fc45.aarch64/.config .config

# 3. Apply our flips (drivers needed for W767 USB bring-up, flipped from =m to =y)
for opt in USB_DWC3 USB_DWC3_QCOM PHY_QCOM_QMP_USB PHY_QCOM_QMP \
           PHY_QCOM_USB_SNPS_FEMTO_V2 USB_XHCI_PLATFORM; do
    scripts/config --enable $opt
done

# 4. Strip Fedora signing/trust-keys (we're not signing modules)
sed -i 's/^CONFIG_MODULE_SIG/# &/' .config
sed -i 's/^CONFIG_SYSTEM_TRUSTED_KEYS=.*/CONFIG_SYSTEM_TRUSTED_KEYS=""/' .config
sed -i 's/^CONFIG_SYSTEM_REVOCATION_KEYS=.*/CONFIG_SYSTEM_REVOCATION_KEYS=""/' .config

# 5. Apply the iter-28 dwc3 instrumentation patch
patch -p1 < /path/to/linux-w767/kernel-patches/iter28-diag/0001-*.patch

# 6. olddefconfig + build
make ARCH=arm64 CROSS_COMPILE=aarch64-linux-gnu- olddefconfig
make ARCH=arm64 CROSS_COMPILE=aarch64-linux-gnu- -j$(nproc) Image dtbs
```

## Why Fedora-hybrid (not our own minimal config)

Across iter-19..24 we hit a recurring pattern: silently-dropped CONFIG_*
when starting from allnoconfig + small merge fragment. The fragments
turned out to be missing PM=y (iter-19), SYSFB+SIMPLEDRM+FB_EFI (iter-20),
PSCI cpuidle (iter-22), QCOM_PDC (iter-23). Each round burned a boot
test to discover one more missing piece.

Starting from Fedora's ~8500-flag config sidesteps the discovery cost.
The only flips we need on top are the dwc3 stack going =y instead of =m,
because our minimal busybox initramfs has no module loader and cannot
load Fedora's modules on demand. Once iter-28 unlocks USB probe, a
later iter could either keep this config or design a proper minimal
based on what we now know is required.

## What's instrumented

`kernel-patches/iter28-diag/0001-dwc3-qcom-legacy-pr_emerg-probe-instrumentation.patch`
adds pr_emerg prints at every step + every error return in dwc3_qcom_probe.
The /init refresh-loop initramfs (iter-24 layout) filters dmesg for these
lines and re-paints them on tty0 every 3 seconds.

## Expected outcome

See `research/2026-05-17-iter28-dwc3-instrumentation.md` for the
interpretation matrix on what photo-shows-X means for what iter-29
fix path would be.
