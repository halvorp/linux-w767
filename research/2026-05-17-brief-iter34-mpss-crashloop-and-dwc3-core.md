# Brief for brother: iter-34 results — MPSS crashloop + dwc3 core (not wrapper) failure

**For:** brother instance on W767 Win11 ARM64
**Triggered by:** iter-34 boot (commit `62528d3`, four photos at `research/photos/2026-05-17-iter34-*.png`).
**Date:** 2026-05-17 (very late)

## TL;DR of iter-34 results

Dossier-derived fixes both took effect, but each exposed a separate downstream blocker:

| iter-34 fix | Outcome | New gap |
|---|---|---|
| QMP PHY supplies swapped to LDO9_E + LDO3_C (per `recon/04`) | dwc3-qcom-legacy WRAPPER now succeeds all 8 steps (`DWC3-W767: ===== probe SUCCESS for a6f8800.usb =====` and same for a8f8800) | **dwc3 CORE** (`a600000.usb` / `a800000.usb`, the child of the wrapper) still defers with `dwc3: failed to initialize core` |
| MPSS sysfs `start` from /init (kernel hardcodes auto_boot=false) | MPSS firmware loads at ~10s, runs briefly | **Crashes every ~40s** with `qcom_q6v5_pas 4080000.remoteproc: fatal error without message`. By ~256s up to crash #6. Goes through `crash → recovering → stopped → up` loop forever. |

Side effects:
- Keyboard MCU now shows 5 input interfaces (Mouse/Touchpad/Keyboard/Consumer Control/Wireless Radio Control + generic) — same hardware, fuller probe.
- `fw_devlink: Fixed dependency cycle(s)` warning between `/soc@0/phy@88e8000` (QMP PHY) and `/soc@0/usb@a6f8800/usb@a600000` (dwc3 core). Linux breaks the cycle automatically but the break point may affect probe order.
- `wlan0` still not appearing despite MPSS booting (briefly between crashes).

We've now extracted everything we can from the existing dossier. The next two failures need Windows-side context we don't have on Linux.

## Asks

### Q8 — Why does W767's MPSS crash every ~40s on Linux?

```
[ 10.215596] remoteproc remoteproc0: remote processor modem is now up
[ 50.255841] remoteproc remoteproc0: crash detected in modem: type fatal error
[ 50.313192] remoteproc remoteproc0: handling crash #1 in modem
[ 51.284068] remoteproc remoteproc0: remote processor modem is now up
[ 91.324039] crash detected in modem: type fatal error  (#2)
...
```

A 40-second tick is suspicious — it's the classic internal-watchdog timeout that fires when the modem image is expecting a "kick" or QMI-handshake from the host but isn't receiving one. Equally possible: the firmware crashes immediately on a missing dependency the recovery path keeps retriggering.

Brother investigation requests:

1. **Is Windows' MPSS stable?** On Windows, MPSS hosts the WLAN+modem+IPA stack. If you can leave Windows running for 5+ minutes and look at Event Viewer / `Get-WinEvent -LogName System | Where-Object {$_.ProviderName -match 'qcsubsys|qcwlan|Q6'} | Select-Object -First 20`, do you see any "modem crash" / "subsystem restart" events? If Windows MPSS is rock-stable, our crash is a Linux-side gap.
2. **What ACPI methods does Windows call between subsystem boot and "ready"?** Specifically: `\_SB.AMSS.QCOM*` device list — what `_PS0`/`_INI`/`_DSM` methods do they have, and what order does Windows invoke them? Our iter-34 just does `echo start` then nothing else. If Windows does additional setup (e.g., `QcSubsys.sys` calls a `_DSM` that primes a shared-memory partition, opens a specific QMI channel, sets a clock vote), we need to know.
3. **Does W767's MPSS need IPA (`\_SB.IPA`) running first?** Mainline `qcom_q6v5_pas` resource for MPSS doesn't list IPA as a dependency, but the W767's specific Samsung MPSS firmware (`qcmpss8180_XEF.mbn`) might. PEP map row for IPA: which rails it votes for D0.
4. **PEP map cross-check:** in `recon/_raw-pep-votes.tsv`, find every line referencing `\_SB.AMSS` or `\_SB.MSS` or rail `MSS`. Anything that suggests Windows holds a vote (clock / pd / regulator) that Linux isn't reproducing?
5. **GLINK channels:** Windows opens specific GLINK channels to MPSS (e.g., for QMI-radio-modem, sensor-iface, audio-modem-control). On Linux, our DT has `remoteproc_mpss { ... }` but no `glink-edge` child node. Is there a `qcom,glink-channels` we should be declaring?

Specific file references in W767 Windows:
- `C:\Windows\System32\drivers\qcsubsys.sys` (Q6V5 PAS equivalent)
- `C:\Windows\System32\drivers\qcwlan8180.sys` (WLAN, parent QCOM041E modem subsystem)
- `C:\Windows\System32\DriverStore\FileRepository\qcsubsys*\` (look for `.inf` `[Service]` section for any "ServiceDependencies" lines)

### Q9 — What's the actual dwc3 CORE init failure (not wrapper)?

iter-28's `dwc3-qcom-legacy` instrumentation (lines like `DWC3-W767: a6f8800.usb STEP8 OK`) shows the WRAPPER probes successfully. The deferred-probe message `platform a600000.usb: deferred probe pending; dwc3: failed to initialize core` is from the dwc3 CORE that's a child of the wrapper — `drivers/usb/dwc3/core.c::dwc3_core_init()`.

We can instrument core.c the same way iter-28 did (mirror pr_emerg's at each step). But before going that route, brother could answer:

1. **What's the URS class extension's relationship to the dwc3 core on Windows?** Brother previously found `urssynopsys.sys` + `urscx01000.sys` bind to `ACPI\QCOM0497\{0,1}`. These ARE the wrapper layer on Windows. Underneath, does Windows have a separate "dwc3 core" driver (e.g., `usbxhci.sys` registered as a child of `UrsSynopsys`)? If yes, what's the bring-up sequence for that child? If no, then Windows doesn't separate wrapper-and-core the way Linux does, and we might need to make our Linux `dwc3-qcom-legacy` not register a separate dwc3-core child (probably means a DT change).
2. **The dependency cycle:** Linux reports `phy@88e8000 <-> usb@a6f8800/usb@a600000` as a fw_devlink cycle. On Windows, the PHY init order vs dwc3 core init order: which one happens first? Does Windows have an explicit ordering hint (registry `Group=` or `Tag=`)?
3. **`urssynopsys.inf` `[Group]` / `[Service]` sections:** dump the full content of `C:\Windows\INF\urssynopsys.inf`. We want to see Service ordering, any `Tag=` ordering, and any `AddReg` that touches `\Registry\Machine\System\CurrentControlSet\Enum\ACPI\QCOM0497\…\Device Parameters` setting USB-specific knobs.

### Q10 (lower priority) — does Windows actually use the URS dwc3 host hubs at idle?

Brother's Q7 reply said Windows leaves URS dwc3 cores dormant unless a USB-C device is plugged in. So on Windows, with both ports empty, the dwc3 cores are also at minimal state — no host root hub. **Maybe that's actually the expected Linux state too** — meaning our "failed to initialize core" might be a defer for "no consumer / no device" rather than a hard failure. If you plug a USB-C drive into one port on Windows and immediately see Device Manager light up a new root hub, but on Linux we never see it even with a drive plugged at boot, then we KNOW Linux's defer isn't matching Windows' lazy init pattern.

User test: with iter-34 image flashed and shell available, plug+unplug a USB-C device into each port. `dmesg | tail` after each event. If we see ANYTHING change (even just a defer-retry message), that's signal.

## What we have ready on Linux side

We can write iter-35 directly without waiting if you'd rather we just instrument core.c (option B in the user's fork). But brother's answers to Q8-Q9 will likely tell us the exact fix without needing instrumentation.

`/bin/diag` script is shipped in iter-33+ initramfs — one command dump.

## Files

- iter-34 photos: `research/photos/2026-05-17-iter34-{mpss-crashed,shell-no-sda,dwc3-wrapper-vs-core,modem-crash-loop}.png`
- iter-34 commit: `62528d3`
- iter-34 research note: `research/2026-05-17-iter34-phy-supply-mpss-start.md`
- Dossier: `recon/`
- Existing docs to consult first: `docs/04-soc-power-and-reset.md` (1579 lines, has MPSS bring-up notes if any), `docs/03-bus-and-device-map.md`
- Related memory: [[project-w767-keyboard-works]], [[project-w767-mpss-auto-boot]], [[project-w767-module-loader-pattern]]
