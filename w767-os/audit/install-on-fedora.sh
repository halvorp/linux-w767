#!/bin/bash
# Install the gbs-audit suite on the running iter-17 Fedora rootfs (or any
# systemd-based target). Idempotent — re-run any time to refresh.
#
# Local install (run on the W767 itself):
#     sudo ./install-on-fedora.sh
#
# Remote install (run on the dev host, pushes via ssh):
#     W767_HOST=root@<ip> ./install-on-fedora.sh
#
# Output paths on target:
#     /usr/local/bin/gbs-audit-full
#     /usr/local/bin/gbs-audit-quick
#     /etc/systemd/system/gbs-audit-full.service
#     /etc/systemd/system/gbs-audit-quick.service
#     /etc/systemd/system/gbs-audit-quick.timer
#     /var/log/gbs-audit/{full,quick}/                (created on first run)

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
W767_HOST="${W767_HOST:-}"

if [ -n "$W767_HOST" ]; then
  RUN() { ssh "$W767_HOST" "$@"; }
  COPY_BIN()  { scp "$1" "$W767_HOST:/usr/local/bin/$(basename "$1")"; }
  COPY_UNIT() { scp "$1" "$W767_HOST:/etc/systemd/system/$(basename "$1")"; }
else
  if [ "$(id -u)" -ne 0 ]; then
    echo "error: local install requires root (use sudo) or set W767_HOST=user@host"
    exit 1
  fi
  RUN() { eval "$@"; }
  COPY_BIN()  { install -m 0755 "$1" "/usr/local/bin/$(basename "$1")"; }
  COPY_UNIT() { install -m 0644 "$1" "/etc/systemd/system/$(basename "$1")"; }
fi

echo "==> installing gbs-audit binaries"
COPY_BIN  "$SCRIPT_DIR/gbs-audit-full"
COPY_BIN  "$SCRIPT_DIR/gbs-audit-quick"

echo "==> installing systemd units"
COPY_UNIT "$SCRIPT_DIR/gbs-audit-full.service"
COPY_UNIT "$SCRIPT_DIR/gbs-audit-quick.service"
COPY_UNIT "$SCRIPT_DIR/gbs-audit-quick.timer"

echo "==> chmod +x via target"
RUN "chmod +x /usr/local/bin/gbs-audit-full /usr/local/bin/gbs-audit-quick"

echo "==> reload systemd + enable units"
RUN "systemctl daemon-reload"
RUN "systemctl enable --now gbs-audit-full.service"
RUN "systemctl enable --now gbs-audit-quick.timer"

echo "==> status"
RUN "systemctl --no-pager status gbs-audit-full.service gbs-audit-quick.timer | head -40"

echo
echo "==> done. Next run paths on target:"
echo "    /var/log/gbs-audit/full/latest/                (full audit, on boot)"
echo "    /var/log/gbs-audit/quick/latest/               (refreshed every 5min)"
echo "    /var/log/gbs-audit/quick/timeline.log          (one-line digest per run)"
echo "    /boot/efi/audit-*.tar.gz                       (USB-pullable copies, last 3)"
echo
echo "To pull latest full audit back to dev host:"
echo "    rsync -av $W767_HOST:/var/log/gbs-audit/ ./gbs-audit-pull/"
