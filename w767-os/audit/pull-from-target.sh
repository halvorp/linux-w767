#!/bin/bash
# Pull the latest gbs-audit results from the W767 to the dev host so Claude (or
# any tooling) can read fresh ground truth without touching the device.
#
# Usage:
#     W767_HOST=root@<ip> ./pull-from-target.sh
#     # → writes ./pulled/<TS>/{full,quick}/

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
W767_HOST="${W767_HOST:-}"
[ -n "$W767_HOST" ] || { echo "set W767_HOST=user@host"; exit 1; }

TS=$(date -u +%Y%m%d-%H%M%SZ)
DEST="$SCRIPT_DIR/pulled/$TS"
mkdir -p "$DEST"

echo "==> pulling /var/log/gbs-audit/ → $DEST"
rsync -av --no-perms --no-owner --no-group \
  "$W767_HOST:/var/log/gbs-audit/" "$DEST/"

# Also stash a 'latest' symlink for convenience
ln -sfn "$TS" "$SCRIPT_DIR/pulled/latest"

echo "==> done. Open $DEST/quick/latest/ or $DEST/full/latest/"
echo "==> Convenience: $SCRIPT_DIR/pulled/latest -> $TS"
