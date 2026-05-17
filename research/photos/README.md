# Boot-screen photos — primary diagnostic data

Phone photos captured from the GBS display during boot tests. These are
the ground-truth source data for the corresponding research notes.

## iter-21 (2026-05-17) — our minimal kernel, first successful boot to shell

- **`2026-05-17-iter21-frame-A.png`** — top of screen, banner + "Useful next commands" list visible
- **`2026-05-17-iter21-frame-B.png`** — bottom of screen, dmesg flood of `deferred probe pending` cascade showing the chain blocked by `/psci/power-domain-cpu-cluster0` (the iter-22 PSCI cpuidle fix target)

Related doc: `../2026-05-17-claude-iter21-boot-result.md`

## iter-25 (2026-05-17) — Fedora 7.0.0-62 aarch64 kernel + our DTS + minimal initramfs

The /init refresh loop kept the screen alive on Fedora's kernel (it didn't
on our minimal kernel — see iter-25 doc for why).

- **`2026-05-17-iter25-frame10-uptime30s.png`** — frame 10 at 30s uptime: shows i2c-hid bound to touchpad at 0-0049, USB/HID empty, filtered dmesg dominated by sync_state-pending messages
- **`2026-05-17-iter25-frame20-uptime61s.png`** — frame 20 at 61s uptime: identical content to frame 10 — kernel reached steady state with no further matching dmesg activity after ~19s. Brother's high-priority debug targets: usb_mp (a4f8800.usb) absent from sync_state lines, usb_sec (a8f8800.usb) present.

Related docs:
  - `../2026-05-17-update-iter25-findings.md` (interprets these photos)
  - `../2026-05-17-brief-deep-usb-display-debug.md` (the asks that iter-25 partially answered)
