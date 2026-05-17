# gbs-audit — on-device hardware audit suite

Continuous, on-device hardware diagnostics for the Galaxy Book S iter-17 rootfs
(and anything else systemd-based). Designed so the dev host can read fresh
ground-truth state at any time without manually `ssh`-ing and running probe
commands.

## Layout

| File                          | Purpose |
| ----------------------------- | ------- |
| `gbs-audit-full`              | Comprehensive audit (~80 files, ~2 MB). Runs on boot. |
| `gbs-audit-quick`             | Hot-files refresh (~10 files, ~50 KB). Runs every 5 min. |
| `gbs-audit-full.service`      | systemd unit for the boot-time run. |
| `gbs-audit-quick.service`     | systemd unit for the periodic run (oneshot). |
| `gbs-audit-quick.timer`       | systemd timer (5-min cadence). |
| `install-on-fedora.sh`        | Drops everything onto target + enables units. |
| `pull-from-target.sh`         | Pulls `/var/log/gbs-audit/` back to dev host. |

## Output (on target)

| Path                                            | Lifecycle |
| ----------------------------------------------- | --------- |
| `/var/log/gbs-audit/full/audit-$TS/`            | Boot-time full audit. Last 5 retained. |
| `/var/log/gbs-audit/full/audit-$TS.tar.gz`      | Tarball of same. |
| `/var/log/gbs-audit/full/latest -> audit-$TS`   | Symlink to most recent full. |
| `/var/log/gbs-audit/quick/latest/`              | Most recent quick audit. **Overwritten in place.** |
| `/var/log/gbs-audit/quick/timeline.log`         | One-line state digest per quick run. Tail to watch state evolve. |
| `/boot/efi/audit-*.tar.gz`                      | Copies on EFI partition for USB-pull. Last 3 retained. |

## Install

**On the W767 directly:**
```
sudo ./install-on-fedora.sh
```

**From the dev host (over ssh):**
```
W767_HOST=root@<ip> ./install-on-fedora.sh
```

Re-run any time to refresh the scripts.

## Pull results back

```
W767_HOST=root@<ip> ./pull-from-target.sh
# → ./pulled/<TS>/full/latest/   ← full audit
# → ./pulled/<TS>/quick/latest/  ← latest quick audit
# → ./pulled/<TS>/quick/timeline.log
```

## On-demand run

```
ssh root@<ip> systemctl start gbs-audit-full.service        # full
ssh root@<ip> systemctl start gbs-audit-quick.service       # quick
```

## What `gbs-audit-full` covers

- System identity (`uname`, DT model, cmdline, EFI vars)
- Kernel + journal (full dmesg, errors, journal failures)
- Hardware enum (`lspci`, `lsusb`, `lsblk`, IOMMU groups, SoC topology)
- Driver bind state (bound, unbound, deferred-probe, modaliases)
- Display + GPU + backlight (every DRM connector + bl_power dance — minus
  destructive backlight ramp; that's left to a separate live test)
- Power/regulators/clocks/wakeup-sources
- **Wi-Fi deep dive** (ath10k debugfs, qmi services, tqftpserv presence,
  wifi platform device tree)
- **Audio deep dive** (asound cards, snd modules, pcm devices)
- Remoteproc state (ADSP/CDSP/MPSS firmware paths + state)
- USB / HID / SPMI / battery / thermal
- **ACPI Samsung devices** (`SAM0xxx` family — for EC reverse-engineering)
- Suspend state + suspend_stats

## Why systemd, not a kernel module

Most diagnostic data lives in userspace tools (`journalctl`, `lspci`,
`upower`, `nmcli`) and `/sys`/`/proc`/`/sys/kernel/debug`. A kernel module
would need to re-implement all of that in-kernel. systemd `oneshot` services
are the right shape: file-system writes are persistent, retry semantics are
free, and no kernel-side risk of crashing the system mid-audit.

(If you ever want a panic-time dump, sysrq-`c` already calls our crash
notifier; we can add a kdump capture step later.)

## Bytes budget

- One full audit: ~2 MB unpacked, ~300 KB tarball
- Quick audit: ~50 KB, overwritten in place
- 5 retained full audits + 7d of timeline → ~2 MB / week
