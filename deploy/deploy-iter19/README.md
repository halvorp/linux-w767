# iter-19 DTB deploy kit

**Goal:** Boot the existing iter-17 Fedora kernel on the Galaxy Book S with the new DTB that adds (1) the correct touchpad node on `&i2c1`, (2) `&spi0` and `&spi3` enables for the future CS35L40 amp control path.

**Risk:** Low. Same kernel as iter-17, additive DTS changes only. iter-17 rollback DTB included.

---

## What's in this directory

| File | md5 | Description |
|---|---|---|
| `sc8180x-samsung-w767.dtb.iter19` | `454a77b5864307d763391dcbe4213682` | New DTB. Built from `dts-stage-v2/sc8180x-samsung-w767.dts` with iter-19 changes folded in. |
| `sc8180x-samsung-w767.dtb.iter17-rollback` | `51c36f8d2be8b13f858a04c093311103` | iter-17 DTB. Copy this back if iter-19 doesn't boot. |

---

## Pre-flight

1. Copy this whole directory onto a USB stick alongside your Live USB Linux distro. Format doesn't matter (FAT32, ext4, exfat — anything the Live USB can read).
2. Confirm the GBS currently boots iter-17 (Phase 1 entry from `w767-os/grub/w767-phase1.conf`).

---

## On the Galaxy Book S, booted into Live USB

```bash
# 1. Identify the iter-17 EFI/boot partition.
#    On the GBS the Fedora root + /boot lives on the internal UFS.
sudo blkid | grep -i 'fedora\|w767\|ext4\|btrfs'
# Expect something like:
#   /dev/sda3:  LABEL="fedora_root"  TYPE="btrfs"
#   /dev/sda1:  LABEL="ESP"          TYPE="vfat"

# 2. Mount the Fedora root (where /boot lives — Fedora puts /boot in /).
sudo mkdir -p /mnt/w767
sudo mount /dev/sdaXX /mnt/w767                    # use the right partition
# If you have a separate /boot partition, also mount it:
sudo mount /dev/sdaYY /mnt/w767/boot               # only if separate

# 3. Confirm the iter-17 DTB is at the expected path.
ls -la /mnt/w767/boot/dtb-w767/qcom/
# Expect: sc8180x-samsung-w767.dtb (the iter-17 one, md5 should equal 51c36f8d...)

# 4. Back up the iter-17 DTB IN PLACE (just in case the rollback file gets lost).
sudo cp /mnt/w767/boot/dtb-w767/qcom/sc8180x-samsung-w767.dtb \
        /mnt/w767/boot/dtb-w767/qcom/sc8180x-samsung-w767.dtb.iter17-bak

# 5. Copy the iter-19 DTB into place (from your USB stick).
USB=/run/media/$USER/<your-usb-label>              # adjust to your Live USB mountpoint
sudo cp $USB/deploy-iter19/sc8180x-samsung-w767.dtb.iter19 \
        /mnt/w767/boot/dtb-w767/qcom/sc8180x-samsung-w767.dtb

# 6. Verify checksum so you know the copy succeeded.
md5sum /mnt/w767/boot/dtb-w767/qcom/sc8180x-samsung-w767.dtb
# Expect: 454a77b5864307d763391dcbe4213682

# 7. Sync + unmount.
sync
sudo umount /mnt/w767/boot 2>/dev/null || true
sudo umount /mnt/w767

# 8. Reboot. Pull the USB stick before POST so the GBS boots from internal storage.
sudo reboot
```

No BLS / grub.cfg edits needed — the iter-17 BLS entry already points at
`/dtb-w767/qcom/sc8180x-samsung-w767.dtb`, which is the file we overwrote.

---

## After boot — what to look for

iter-17 already produced display + GPU via the eDP path. iter-19 should preserve that AND probe the touchpad. Check from a terminal (or via `early-dmesg.service` if the screen doesn't come up):

```bash
# 1. Did the system come up at all?
uptime
uname -a                  # should be 7.0.0-62.fc45.aarch64 (same as iter-17)

# 2. Touchpad: should appear as an i2c-hid device on i2c1 at address 0x02.
ls /sys/bus/i2c/devices/ | grep -i 'i2c1\|hid'
# Look for something like  i2c-HID0001:00  or  0-0002
dmesg | grep -iE 'hid|touchpad|i2c.*0002|i2c-1' | head -20

# 3. SPI buses: confirm they came up (they have no codec children yet, so they'll be quiet).
ls /sys/class/spi_master/
# Expect spi0 and spi3 listed (which are sc8180x QUP slots 0x880000 and 0x88C000).

# 4. Display: confirm no regression vs iter-17.
ls /sys/class/drm/
cat /sys/class/drm/card1-eDP-1/status 2>/dev/null   # expect "connected"

# 5. Capture a journal snapshot in case anything's off.
journalctl -b -k > /tmp/iter19-boot-kernel.log
journalctl -b -u systemd-udevd > /tmp/iter19-boot-udev.log
```

---

## Expected outcomes by likelihood

| Outcome | Action |
|---|---|
| 🎉 Display still works, touchpad enumerates and moves the cursor | Confirmed bus-map fix. Save the journal logs, snapshot the working state, move on to enabling SPI codec child nodes. |
| Display still works, touchpad enumerates but doesn't respond | Touchpad probe succeeded but the HID descriptor at offset `0x0001` may need adjustment (the existing broken `touchscreen@49` guess used `0x00ab`). Save `dmesg | grep hid`. |
| Display still works, touchpad doesn't enumerate | Either GPIO 113 is wrong, `&i2c1` clock is wrong, or there's a regulator dependency the DSDT-derived spec didn't capture. Compare with `i2cdetect -y 1` if available. |
| Display fails to come up (black screen) | Regression — the new DTB broke eDP. Roll back: boot Live USB again, restore the backup with `cp /mnt/w767/boot/dtb-w767/qcom/sc8180x-samsung-w767.dtb.iter17-bak /mnt/w767/boot/dtb-w767/qcom/sc8180x-samsung-w767.dtb`. The Phase 2 entry (initramfs distro) is also there as a fallback. |
| Kernel panic / no boot at all | Roll back via Live USB as above. The `early-dmesg.service` from `galaxybook-s/fixes/` should have captured the early dmesg into `/boot/efi/early_dmesg_*.txt` if userspace got that far; check ramoops at `/sys/fs/pstore/dmesg-ramoops-0` if it didn't. |

---

## What iter-19 changed vs iter-17

Only two DTS deltas, both additive (with one replacement of a broken iter-17 placeholder):

1. **`&i2c1`** — Replaced the non-functional `touchscreen@49` placeholder (which had a wrong slave address and HID descriptor offset, plus a still-pending PMIC regulator issue) with `touchpad@2` using the DSDT-verified spec:
   - reg = `0x02`
   - hid-descr-addr = `0x0001`
   - interrupts = GPIO 113 (same as before — that part was correct)
2. **`&spi0` and `&spi3` enabled** — The two SPI buses that carry CS35L40 amp control (per ACPI `\_SB.SPI1` at MMIO `0x00880000` and `\_SB.SPI4` at `0x0088C000`). No child codec nodes yet — buses are just available for future binding.

Nothing else changed. If iter-17 booted, iter-19 should boot.
