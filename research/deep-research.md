# Comprehensive Technical Analysis: Integration and Deployment of Linux on the ARM64 Samsung Galaxy Book S

The Samsung Galaxy Book S represents a watershed moment in the convergence of mobile telecommunications hardware and traditional computing architectures. Powered by the Qualcomm Snapdragon 8cx (SC8180X) System-on-Chip (SoC), this device was engineered to leverage the high-efficiency ARMv8-A architecture to deliver an ultra-portable, "always-connected" experience under the Windows on ARM ecosystem.

However, for systems engineers and Linux enthusiasts, the Galaxy Book S presents a formidable challenge in hardware enablement, kernel optimization, and firmware governance. Transitioning this platform to a Linux environment—specifically Arch Linux ARM or Fedora AArch64—requires a sophisticated understanding of the discrepancies between x86-based ACPI (Advanced Configuration and Power Interface) standards and the Device Tree (DT) mechanisms that define the ARM ecosystem.

The following report provides an exhaustive technical roadmap for the installation, configuration, and hardware optimization of Linux on the Samsung Galaxy Book S with integrated LTE connectivity.

## Hardware Architecture and SoC Specifications

The heart of the Samsung Galaxy Book S is the Snapdragon 8cx Gen 1 (SC8180X), a 7nm FinFET SoC designed specifically for the compute-intensive requirements of laptop form factors. Unlike its predecessors derived from mobile phone chipsets, the 8cx was built to challenge the 15-watt thermal design power (TDP) envelopes of the Intel Core i5 series while maintaining the power efficiency of an ARM processor.

### System-on-Chip Component Matrix

| Subsystem | Specification | Linux Driver/Kernel Module |
|-----------|---------------|----------------------------|
| CPU | Qualcomm Kryo 495 (4x Gold + 4x Silver) | Mainline AArch64 Scheduler |
| GPU | Adreno 680 | Mesa (Freedreno) / KGSL |
| Modem | Qualcomm X24 LTE | ModemManager / QMI / MBIM |
| Audio | Realtek High Definition Audio | `snd-hda-intel` (with `alc298` quirks) |
| Storage | 128GB/256GB UFS 3.0 | `ufshcd-qcom` |
| Connectivity | WCN3998 (Wi-Fi 5 / Bluetooth 5.0) | `ath10k` / `ath11k` |
| Memory | 8GB LPDDR4x | Standard Memory Management Unit (SMMU) |

The Kryo 495 CPU utilizes a "Big.LITTLE" configuration, although it is more accurately described as a performance-efficiency cluster arrangement. The four performance "Gold" cores are capable of higher clock speeds and greater IPC (Instructions Per Cycle), while the four efficiency "Silver" cores handle background tasks to extend battery life. For a Linux distribution to effectively manage this hardware, the kernel scheduler must be aware of the heterogeneous nature of these cores, a feature that has seen significant refinement in the AArch64 mainline kernel since version 5.15.

The Adreno 680 GPU provides significant graphical throughput but relies heavily on the `freedreno` Gallium3D driver in Mesa for open-source acceleration. While basic display functionality is often available through the EFI Framebuffer (`efifb`), high-performance desktop environments like GNOME or KDE Plasma require the full enablement of the GPU's 3D pipelines, which necessitates the extraction and loading of proprietary firmware blobs.

## Firmware Governance and UEFI Implementation

A critical distinction of the Samsung Galaxy Book S, compared to other ARM-based devices like the Pinebook Pro or various Chromebooks, is its adherence to the Unified Extensible Firmware Interface (UEFI) standard. While many ARM devices utilize U-Boot or proprietary bootloaders that require complex unlocking procedures, Samsung has implemented a relatively standard UEFI that allows for external media booting and Secure Boot management.

### Accessing and Configuring UEFI Settings

Accessing the firmware on the Galaxy Book S is the first essential step in any Linux deployment. The device must be completely shut down; modern Windows "Fast Startup" often puts the machine into a deep sleep that bypasses the UEFI interrupt. Upon powering on the device, the **F2** key must be tapped repeatedly to enter the BIOS/UEFI setup utility.

Within the UEFI interface, several settings are paramount for Linux compatibility:

- **Secure Boot Control:** This feature, designed to ensure only signed operating systems can boot, must be set to "Disabled". Most ARM64 Linux distributions do not yet ship with the Microsoft-signed SHIM required for Secure Boot on these platforms.
- **USB Booting:** Ensure that booting from external media is enabled. Samsung's firmware is generally permissive once Secure Boot is disabled.
- **Boot Priority:** While the boot priority can be changed in the UEFI, the manual boot menu is accessible by tapping **F10** during the initial splash screen.

The presence of a UEFI interface simplifies the bootloader phase, as standard GRUB 2 for AArch64 can be used without the need for specialized flashing tools like `dd` for the initial boot stage. However, the UEFI implementation on the Galaxy Book S is not fully SBSA (Server Base System Architecture) compliant, meaning it does not provide all the hardware description tables required for a "plug-and-play" Linux experience.

## The Kernel Challenge: Device Tree vs. ACPI

The primary technical hurdle for Linux on the Galaxy Book S is the discrepancy in hardware description methods. Traditional x86 laptops use ACPI tables to inform the operating system about the hardware layout, power states, and interrupts. On the Galaxy Book S, the ACPI tables provided by the firmware are specifically tailored for Windows on ARM.

### The Role of PEP Drivers and ACPI Limitations

In the Windows on ARM environment, Microsoft and Qualcomm utilize Power Engine Plugin (PEP) drivers to bridge the gaps in the ACPI implementation. These PEP drivers handle complex power domain management and hardware initialization that is not defined in the static ACPI tables. Because the Linux kernel lacks an equivalent to these PEP drivers, it cannot fully initialize the hardware using ACPI alone.

To circumvent this, Linux developers utilize Device Trees (DT)—static data structures that provide an exhaustive list of the hardware components and their memory addresses. For the Galaxy Book S to boot successfully, the kernel must be supplied with a Device Tree Blob (DTB) specifically compiled for the SC8180X chipset and the SM-W737 board.

### Kernel Mainlining Status

As of late 2024 and early 2025, support for the Snapdragon 8cx has seen significant maturation in the mainline Linux kernel.

| Kernel Version | Milestone Support |
|----------------|-------------------|
| 5.15 | Initial support for SC8180X and Adreno 680. |
| 6.0 | Support for Snapdragon 8cx Gen 3 (SC8280XP). |
| 6.1 | Refined support for Gen 1 (SC8180X). |
| 6.15 | Inclusion of the `samsung-galaxybook` platform driver. |

The kernel version 6.15 is widely considered the "minimum viable" version for a stable experience, as it integrates the platform-specific driver that handles Samsung's unique implementation of Fn keys, battery thresholds, and thermal management.

## Fedora AArch64: Deployment and Configuration

Fedora has emerged as the leading distribution for Snapdragon-based laptops due to its aggressive kernel update cycle and its specialized work on ARM64 bring-up. For the Galaxy Book S, the standard Fedora Workstation ISO may not boot successfully out of the box because it lacks the specific DTB required for the SC8180X firmware.

### Modified ISO Creation for Snapdragon Laptops

System architects recommend utilizing Fedora Rawhide or the latest Fedora 42 (Beta/GA) for the best results, as these versions contain the most recent patches for the Snapdragon X and 8cx series. The deployment involves a "side-loading" process for the Device Tree:

1. **Extract the DTB:** The Device Tree Blobs are often present in the `kernel-core` RPM for AArch64 but are not included in the generic ISO's boot configuration. One must extract the `.dtb` file (specifically `sc8180x-samsung-galaxy-book-s.dtb` or similar) from the RPM using `rpm2cpio`.
2. **Modify the Boot Configuration:** Using tools like `lorax mkksiso`, a custom ISO must be generated that includes the DTB in the `/boot` directory.
3. **GRUB Entry Modification:** The GRUB configuration on the installation media must be updated to include the line `devicetree /boot/dtb/qcom/sc8180x-samsung-galaxy-book-s.dtb` within the main menu entry.

### Kernel Parameters for System Stability

During the initial boot of the installer, the kernel may encounter race conditions or power domain failures that result in an immediate reboot or a "black screen". To mitigate this, several kernel parameters should be appended to the boot command line:

- `clk_ignore_unused`: Instructs the kernel to keep all clocks running, even if the driver subsystem has not explicitly claimed them.
- `pd_ignore_unused`: Prevents the kernel from disabling power domains that may be required by the firmware but are unrecognized by the Linux driver stack.
- `arm-smmu.disable_bypass=0`: This is critical for systems where the SMMU (System Memory Management Unit) blocks access to hardware like the keyboard or touchpad during the initial probe phase.

Once these parameters are applied and the DTB is correctly loaded, the Fedora installer should reach the graphical environment, although Wi-Fi and GPU acceleration will remain disabled until firmware blobs are extracted.

## Arch Linux ARM: Minimalist Implementation

Arch Linux ARM (ALARM) provides an alternative pathway for users who prefer a rolling-release model and absolute control over their system configuration. Unlike Fedora, ALARM does not provide an ISO-based installer. Instead, it relies on a root filesystem (rootfs) tarball that must be manually deployed to the target media.

### Partitioning and Filesystem Architecture

The Galaxy Book S utilizes Universal Flash Storage (UFS), which registers as `/dev/sda` or `/dev/nvme0n1` depending on the kernel version and UFS controller driver. A standard partition scheme for Arch Linux ARM is detailed below:

| Partition | Size | Filesystem | Mount Point | Flags |
|-----------|------|------------|-------------|-------|
| 1 | 512 MiB | FAT32 | `/boot` | ESP, Boot |
| 2 | Remaining | Ext4/Btrfs | `/` | Root |

The installation process involves mounting these partitions on a second Linux machine (or via a live Fedora USB) and extracting the `ArchLinuxARM-aarch64-latest.tar.gz` archive to the root partition as the root user.

### Manual Bootloader and Kernel Setup

Arch Linux ARM requires the manual installation of the bootloader. For the Galaxy Book S, GRUB 2 for EFI is the most reliable choice. After `chroot`-ing into the new Arch installation, the user must install the `grub` and `efibootmgr` packages.

The GRUB installation command for ARM64 UEFI is:

```bash
grub-install --target=arm64-efi --efi-directory=/boot --bootloader-id=GRUB --removable
```

The inclusion of the `--removable` flag is often necessary for ARM UEFI implementations that do not consistently respect `efibootmgr` entries.

Crucially, the user must then place the correct `.dtb` file in `/boot/dtb/qcom/` and update `/etc/default/grub` to include the `devicetree` directive. Without this step, Arch will attempt to boot via ACPI and will likely fail to initialize the keyboard, making local login impossible.

## Firmware Governance and Proprietary Blobs

One of the most persistent challenges in ARM64 Linux deployment is the requirement for proprietary firmware blobs. These files are not included in standard Linux distributions due to licensing restrictions, yet they are essential for the operation of the GPU, Wi-Fi, Bluetooth, and LTE modem.

### The `qcom-firmware-extract` Utility

To resolve this, the community has developed the `qcom-firmware-extract` utility, which was originally designed for the Ubuntu bring-up on Snapdragon laptops. This tool identifies and extracts the necessary blobs from an existing Windows installation.

If the original Windows partition is still present on the device, the process is as follows:

1. Mount the Windows partition (often using `ntfs-3g`) within the Linux environment.
2. Locate the DriverStore: `C:\Windows\System32\DriverStore\FileRepository\`.
3. Run the utility:

   ```bash
   sudo qcom-firmware-extract -d /mnt/windows/System32/DriverStore/FileRepository/
   ```

4. The extracted files must be placed in `/lib/firmware/qcom/`.

Following the extraction and a subsequent reboot, the kernel should be able to initialize the Adreno 680 GPU via the `msm` driver and the WCN3998 Wi-Fi module via the `ath10k` driver.

## Hardware Enablement: LTE, Audio, and GPU

Achieving a fully functional system requires addressing the specific needs of the device's peripheral components. The Samsung Galaxy Book S features high-end multimedia and connectivity hardware that requires precise driver configuration.

### Integrated X24 LTE Modem Connectivity

The integrated Qualcomm X24 LTE modem is a critical component for the "always-connected" use case. In a Linux environment, this modem is managed via `ModemManager`.

The modem communicates with the system using either the QMI (Qualcomm MSM Interface) or MBIM (Mobile Broadband Interface Model) protocol. MBIM is generally preferred for its compatibility with modern Linux network stacks. For the modem to function, the firmware blobs extracted from Windows must be present in the `/lib/firmware/qcom/` directory.

Once the firmware is loaded, the `mmcli -L` command should list the modem. Users can then use `nm-connection-editor` to create a new Cellular connection, specifying the APN provided by their LTE carrier. It is important to note that `ModemManager` may need to be restarted after the initial firmware load to properly enumerate the SIM card slot.

### Audio Subsystem and Speaker Amplifiers

Audio support on the Galaxy Book S is complex due to the use of discrete amplifiers that require specific "quirk" patches to enable. While the Realtek ALC298 codec is supported by the `snd-hda-intel` driver, the speakers will remain silent until the amplifiers are correctly initialized.

Mainline kernel 6.15 includes the `samsung-galaxybook` platform driver, which handles the necessary GPIO toggles for many models. However, if audio is still non-functional, a manual quirk can be forced by creating the file `/etc/modprobe.d/alsa-base.conf` with the following content:

```
options snd-hda-intel model=alc298-samsung-amp-v2-2-amps
```

The "2-amps" or "4-amps" variant should be chosen based on the specific hardware model's speaker count.

### GPU Acceleration and Mesa

Without proper firmware and the `freedreno` driver, the Galaxy Book S will rely on software rendering (LLVMPipe), which consumes significant CPU resources and leads to poor UI performance. With the Adreno 680 fully enabled, the system can leverage hardware acceleration for video playback and desktop effects. Users should ensure that the `mesa-va-drivers-freedreno` (or equivalent) package is installed to enable hardware-accelerated video decoding.

## Power Management and System Stability

ARM-based laptops excel at power efficiency, but achieving the 20-plus hour battery life promised by Samsung requires fine-grained power management under Linux.

### Battery Monitoring and the BIOS Bug

A significant issue reported by the community is the failure of battery level reporting on many Samsung Galaxy Book models. This is caused by a bug in the Samsung BIOS where the ACPI `_FST` (Fan Status) or `_BST` (Battery Status) method returns a reference to a memory location rather than the value itself.

The `samsung-galaxybook` driver integrated into kernel 6.15 attempts to mitigate this, but in some cases, a custom DSDT (Differentiated System Description Table) override is necessary. This involves dumping the ACPI tables, patching the offending methods with a `DerefOf()` call, and instructing the kernel to load the modified table at boot.

### Sleep and Suspend-to-RAM

Suspend-to-RAM (S3) is generally not supported on Snapdragon laptops; instead, they use "Modern Standby" or "S2Idle". For the Galaxy Book S to sleep properly under Linux, the following conditions must be met:

- **Airplane Mode:** Bluetooth and Wi-Fi drivers often prevent the system from entering a low-power state. Enabling Airplane Mode before closing the lid can improve sleep success rates.
- **Display Off:** Ensure that the kernel is correctly turning off the eDP panel before entering the idle state.

Battery drain during sleep remains higher on Linux than on Windows, as the Linux kernel does not yet support all the fine-grained power state transitions of the SC8180X's power management integrated circuits (PMICs).

## Comparative Analysis of Distributions

Choosing between Fedora and Arch for the Galaxy Book S depends on the user's priority regarding stability versus customization.

### Distribution Comparison Matrix

| Feature | Fedora AArch64 | Arch Linux ARM |
|---------|----------------|----------------|
| Ease of Install | Moderate (Requires custom ISO) | Difficult (Manual rootfs setup) |
| Kernel Version | Very Recent (Rawhide/F42) | Bleeding Edge (Rolling) |
| Out-of-box Drivers | Better firmware integration | Minimalist (Requires manual blobs) |
| Platform Drivers | Included in 6.15+ | Requires manual package |
| Documentation | Robust (Fedora Wiki/Matrix) | Community-driven (ALARM forums) |

Fedora is generally recommended for those who want a functional system with the least amount of manual kernel patching, while Arch is preferred by users who intend to build their own kernels and customize every aspect of the hardware description.

## Troubleshooting and Community Resources

The deployment of Linux on Snapdragon laptops is a rapidly evolving field. When hardware components fail to initialize, the `dmesg` log is the primary diagnostic tool.

### Common Failure Points and Resolutions

- **Keyboard Not Working:** This is almost always an SMMU issue. Applying the `arm-smmu.disable_bypass=0` parameter often resolves the block.
- **Reboot Loop:** If the system reboots immediately after the "Starting Linux Kernel..." message, it is likely due to a missing or incorrect DTB.
- **No Wi-Fi/Bluetooth:** Verify that the `ath10k` firmware is present in `/lib/firmware/ath10k/`. Some models may also require the `pd_ignore_unused` parameter to keep the connectivity chip powered.
- **Inverted Touch/Rotation:** Display rotation can be set via `video=eDP-1:panel_orientation=right_side_up` in the kernel parameters.

The `aarch64-laptops` project on GitHub and its associated Matrix/IRC channels (`#aarch64-laptops` on OFTC) are the definitive resources for the most recent DTB files and kernel patches.

## Conclusion: Strategic Recommendations for Deployment

Running Linux on the ARM64 Samsung Galaxy Book S is a technically demanding but ultimately rewarding endeavor. The device offers a unique combination of extreme portability, silent operation, and high-speed LTE connectivity that remains rare in the Linux laptop market.

For a successful deployment, the following strategic steps are recommended:

1. **Prioritize Fedora Rawhide or Fedora 42:** The integrated support for the `samsung-galaxybook` platform driver in kernel 6.15 significantly reduces the amount of manual patching required for Fn keys and battery management.
2. **Maintain the Windows Partition initially:** Use the Windows environment to extract the proprietary firmware blobs using `qcom-firmware-extract`. These blobs are the difference between a sluggish, offline device and a fully accelerated mobile workstation.
3. **Engage with the Community:** The Device Tree and kernel requirements for Snapdragon laptops are in constant flux. Monitoring the `aarch64-laptops` GitHub repository ensures access to the most recent workarounds for the SMMU and BIOS-related bugs.

As the Linux kernel continues to integrate more robust support for the Snapdragon platform—spurred by the arrival of the Snapdragon X Elite—the Galaxy Book S will benefit from a more standardized and stable experience. For professional peers and systems architects, the Galaxy Book S serves as an excellent development platform for the future of ARM-based mobile computing.
