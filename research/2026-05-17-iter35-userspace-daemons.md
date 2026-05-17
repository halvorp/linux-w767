# iter-35: rmtfs + pd-mapper + tqftpserv userspace + fw_devlink=permissive

**Date:** 2026-05-17 (very late)
**Status:** built, awaiting boot
**Based on:** brother's Q8/Q9 reply (commit `0d017d2`) + iter-34 evidence + docs/00-hardware-combined.md

## Two fixes in one boot

### Fix 1 — MPSS crash loop: ship userspace daemons MPSS firmware needs

iter-34's crash cycle (every ~40s, fatal error without message) was caused by `\_SB.AMSS._DEP` including `\_SB.RFS0` (QCOM0417, Windows driver `qcremotefs8180.sys`, "Remote File System"). MPSS firmware boots, immediately issues `RFSA_RPC_OPEN` to read its `/persist` partition, gets no response within ~30s, panics. ADSP works because its `_DEP` lacks `\_SB.RFS0`.

Linux equivalent of `qcremotefs8180.sys` is **`rmtfs`** from `github.com/linux-msm/rmtfs`. Plus three siblings the modem stack needs:

| Daemon | Job | Path in initramfs |
|---|---|---|
| `qrtr-ns` (in-kernel) | QRTR name service / QMI registry | `net/qrtr/ns.c` — built-in via `CONFIG_QRTR=y`. **No userspace binary needed.** |
| `rmtfs` | Backs modem's `/persist` requests via host files | `/usr/bin/rmtfs` |
| `pd-mapper` | Reads `.jsn` blobs and registers each protection-domain service. **WLAN QMI service registration depends on this** (modemuw.jsn → wlan_pd) | `/usr/bin/pd-mapper` |
| `tqftpserv` | TFTP-like file server for late-stage MPSS image loads | `/usr/bin/tqftpserv` |

**Source pragma:** Cross-compile from source would have meant building libqrtr (qrtr meson project) + chasing libudev/liblzma/libzstd aarch64 cross-compile. Instead pulled pre-built Alpine aarch64 binaries from `dl-cdn.alpinelinux.org/alpine/edge/{community,testing,main}/aarch64/`. Each daemon links against musl + libqrtr + (udev | lzma | zstd). Total staged: 3 daemons (~205 KB) + 4 libs (~1 MB) + musl loader (~720 KB) = ~2 MB added to initramfs.

Init script changes:
```sh
mkdir -p /var/lib/rmtfs
export LD_LIBRARY_PATH=/usr/lib:/lib
/usr/bin/pd-mapper  >/dev/kmsg 2>&1 &
/usr/bin/rmtfs -P -s -r  >/dev/kmsg 2>&1 &
/usr/bin/tqftpserv  >/dev/kmsg 2>&1 &
# ... then existing MPSS sysfs-start poller fires
```

Daemons start BEFORE the MPSS auto-start poll. Once MPSS comes up, RFSA QMI calls land on rmtfs, pd-mapper publishes the WLAN service on QRTR, ath10k_snoc finds wlanfw and brings up `wlan0`.

### Fix 2 — dwc3 core: fw_devlink=permissive cmdline

iter-34 dmesg showed:
```
fw_devlink: Fixed dependency cycle(s) between
  /soc@0/phy@88e8000 and /soc@0/usb@a6f8800/usb@a600000
```

QMP USB3 PHY and dwc3 core are in a circular OF graph. fw_devlink breaks the cycle but the break might leave dwc3 core probing before PHY is ready → -EPROBE_DEFER → "failed to initialize core" with no retry.

Same exact bug as early sc8280xp X13s (fixed in kernel commit `b0c7c2f` ~v6.6). Our kernel is 7.0.0 (should have the fix) but cycle persists anyway.

Cheap experiment per brother's Q9 reply: add `fw_devlink=permissive` to BLS cmdline. This makes fw_devlink log cycles but not enforce them — devices probe on demand with -EPROBE_DEFER retry. If this unblocks dwc3 core, the cycle is the root cause and iter-36 plans a DTS refactor.

BLS cmdline change:
```
options  console=tty0 loglevel=8 consoleblank=0 nomodeset rdinit=/init
         earlycon=efifb keep_bootcon net.ifnames=0 panic=10
         fw_devlink=permissive          <-- NEW
         clk_ignore_unused pd_ignore_unused arm64.nopauth efi=noruntime
         iommu.passthrough=0 iommu.strict=0 pcie_aspm.policy=powersupersave
```

## Expected outcomes

| Per-frame display | Means |
|---|---|
| `remoteproc0 (modem): running` stable for >5 min (vs 40s crash loop) | rmtfs answered RFSA → MPSS happy |
| `wlan0` appears in NET | pd-mapper registered WLAN service → ath10k_snoc bound |
| Three USB buses or `/dev/sda` on either USB-C port | fw_devlink=permissive let dwc3 core probe |
| `ps` at shell shows pd-mapper/rmtfs/tqftpserv running | Daemons up |
| MPSS still crashes | Either rmtfs flags wrong, or another dep we missed |
| dwc3 still failing | Cycle wasn't the cause; iter-36 instruments dwc3_core_init |

## What's NOT in this iter

- No kernel changes (kernel unchanged from iter-34)
- No DTS changes (still iter-34's PHY supply swap)
- No firmware changes (all .jsn + .mbn already staged)
- pmic-glink connector graph stays bare (Branch C from iter-33)

## Files

- Daemons: `w767-os/initramfs/layout-iter35/usr/bin/{rmtfs,pd-mapper,tqftpserv}` + libs at `/usr/lib/` + musl loader at `/lib/`
- Init: `w767-os/initramfs/layout-iter35/init` (daemon startup + existing MPSS kick + ESP retry + refresh loop)
- Image: `/tmp/w767-iter35.img` (local only, 784 MB)
- Source attributions: Alpine edge community + testing aarch64 packages (rmtfs, pd-mapper, tqftpserv, qrtr-libs)
- Brother's source brief: `research/2026-05-17-claude-q8-q9-mpss-and-dwc3-core.md` (commit `0d017d2`)
