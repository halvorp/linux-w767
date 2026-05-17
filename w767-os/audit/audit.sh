#!/bin/bash
# Galaxy Book S comprehensive diagnostic audit.
# Runs on the device, writes everything to /boot/efi/audit-TIMESTAMP/
# Safe, non-destructive (one backlight-dance test at the end, restores state).

set +e
set -u

TS=$(date +%Y%m%d-%H%M%S)
OUT="/boot/efi/audit-$TS"
mkdir -p "$OUT"
cd "$OUT" || { echo "can't cd $OUT"; exit 1; }
echo "==> Writing audit to $OUT"

# Helper: run a command, capture stdout+stderr to FILE, don't abort on error
cap() {
  local name="$1"; shift
  {
    printf '>>> %s\n' "$*"
    "$@" 2>&1
    printf '[exit=%s]\n' "$?"
  } > "$name" 2>&1 || true
  printf '  %-32s done\n' "$name"
}

# Helper: run a shell snippet (for pipelines, loops, etc.)
capsh() {
  local name="$1"; shift
  {
    printf '>>> %s\n' "$*"
    bash -c "$*" 2>&1
    printf '[exit=%s]\n' "$?"
  } > "$name" 2>&1 || true
  printf '  %-32s done\n' "$name"
}

# ---- System / boot identity --------------------------------------------------
cap   00-uname.txt                   uname -a
cap   01-os-release.txt              cat /etc/os-release
cap   02-cmdline.txt                 cat /proc/cmdline
cap   03-dt-model.txt                cat /sys/firmware/devicetree/base/model
capsh 04-dt-compatible.txt           "tr -d '\0' </sys/firmware/devicetree/base/compatible | tr ',' '\n'"
cap   05-efi-vars.txt                bash -c 'ls /sys/firmware/efi/efivars/ 2>/dev/null | head -50'
cap   06-uptime.txt                  uptime
cap   07-date.txt                    date -u

# ---- Kernel / journal --------------------------------------------------------
cap   10-dmesg.txt                   dmesg -T
capsh 11-dmesg-errors.txt            "dmesg | grep -iE 'err|fail|warn|BUG|Oops|cannot|unable|timed? out|deferred probe' | head -300"
capsh 12-journal-kernel.txt          "journalctl -k -b 0 --no-pager"
capsh 13-journal-all.txt             "journalctl -b 0 --no-pager"
capsh 14-journal-failed.txt          "journalctl -b 0 --no-pager | grep -iE 'failed|error|warning' | head -200"

# ---- Hardware enumeration ----------------------------------------------------
cap   20-lspci.txt                   lspci -vvv
cap   21-lsusb-tree.txt              lsusb -tv
cap   22-lsusb-devices.txt           lsusb -vv
cap   23-lsblk.txt                   lsblk -f -o NAME,SIZE,FSTYPE,LABEL,UUID,MOUNTPOINTS
capsh 24-iommu-groups.txt            "for d in /sys/kernel/iommu_groups/*/devices/*; do echo \$d; done | sort"
capsh 25-soc-topology.txt            "find /sys/devices/platform/soc@0 -maxdepth 2 -type d 2>/dev/null | sort"

# ---- Drivers / modules / probe state ----------------------------------------
cap   30-lsmod.txt                   lsmod
capsh 31-drivers-bound.txt           "find /sys/bus/*/drivers/*/ -maxdepth 2 -type l 2>/dev/null | sort"
capsh 32-devices-nodriver.txt        'for d in /sys/bus/*/devices/*; do if [ -e "$d/uevent" ] && [ ! -e "$d/driver" ]; then echo "UNBOUND: $d"; cat "$d/uevent" | sed "s/^/  /" 2>/dev/null; echo; fi; done | head -300'
capsh 33-deferred-probe.txt          "dmesg | grep -iE 'deferred probe|EPROBE_DEFER|-517' | head -100"
capsh 34-modalias.txt                "for d in /sys/bus/platform/devices/*; do if [ -e \$d/modalias ]; then echo \"\$d: \$(cat \$d/modalias)\"; fi; done"

# ---- Display / GPU / backlight — THE FOCUS ----------------------------------
cap   40-drm-list.txt                ls -laR /sys/class/drm/
capsh 41-drm-details.txt             'for d in /sys/class/drm/card*-*; do [ -d "$d" ] || continue; echo "=== $d ==="; for f in status enabled modes edid subconnector connector_id; do if [ -e "$d/$f" ]; then printf "  %s: " "$f"; if [ "$f" = "edid" ]; then od -An -tx1 <"$d/$f" | head -4; else cat "$d/$f"; fi; fi; done; echo; done'
cap   42-backlight-list.txt          ls -laR /sys/class/backlight/
capsh 43-backlight-all.txt           'for d in /sys/class/backlight/*; do [ -d "$d" ] || continue; echo "=== $d ==="; for f in brightness max_brightness actual_brightness bl_power type scale; do if [ -e "$d/$f" ]; then printf "  %s: " "$f"; cat "$d/$f" 2>&1; fi; done; echo; done'
cap   44-graphics-fb.txt             bash -c 'for d in /sys/class/graphics/fb*; do [ -e "$d" ] || continue; echo "=== $d ==="; for f in name modes virtual_size bits_per_pixel; do [ -e "$d/$f" ] && printf "%s=%s\n" "$f" "$(cat $d/$f)"; done; echo; done'
cap   45-simpledrm.txt               bash -c 'find /sys/bus/platform/devices/simple-framebuffer* -maxdepth 3 2>/dev/null | head -50'
cap   46-drm-debug.txt               bash -c 'find /sys/kernel/debug/dri -maxdepth 4 2>/dev/null | sort'
capsh 47-drm-debug-all.txt           'for f in /sys/kernel/debug/dri/*/{name,state,clients,framebuffer,encoders,connectors,modeset}; do [ -e "$f" ] && { echo "=== $f ==="; cat "$f"; echo; }; done'

# ---- Power / regulators / clocks --------------------------------------------
capsh 50-regulator-summary.txt       "cat /sys/kernel/debug/regulator/regulator_summary 2>&1 | head -300"
capsh 51-clk-summary.txt             "cat /sys/kernel/debug/clk/clk_summary 2>&1 | head -400"
capsh 52-genpd-summary.txt           "cat /sys/kernel/debug/pm_genpd/pm_genpd_summary 2>&1 | head -200"
capsh 53-power-supply.txt            'for d in /sys/class/power_supply/*; do [ -d "$d" ] || continue; echo "=== $d ==="; cat "$d/uevent" 2>&1; echo; done'
capsh 54-wakeup-sources.txt          "cat /sys/kernel/debug/wakeup_sources 2>&1 | head -100"

# ---- Network / wireless ------------------------------------------------------
cap   60-ip-addr.txt                 ip -br addr
cap   61-ip-link.txt                 ip -br link
cap   62-ip-route.txt                ip route
cap   63-rfkill.txt                  rfkill list
cap   64-iw-dev.txt                  iw dev
cap   65-nmcli-dev.txt               nmcli -t dev status
cap   66-nmcli-con.txt               nmcli -t con show
capsh 67-wifi-modules.txt            "lsmod | grep -iE 'ath10k|cfg80211|mac80211|wcn|wireless'"
capsh 68-wifi-dmesg.txt              "dmesg | grep -iE 'ath10k|wcn|wlan|qmi|pd_mapper|snoc' | head -100"

# ---- Remoteproc / firmware --------------------------------------------------
capsh 70-remoteproc-state.txt        'for d in /sys/class/remoteproc/*; do [ -d "$d" ] || continue; echo "=== $d ==="; for f in state firmware name recovery; do [ -e "$d/$f" ] && printf "  %s=%s\n" "$f" "$(cat $d/$f 2>&1)"; done; echo; done'
capsh 71-remoteproc-debug.txt        'find /sys/kernel/debug/remoteproc -maxdepth 3 -type f 2>/dev/null | while read f; do echo "=== $f ==="; cat "$f" 2>&1; echo; done'
capsh 72-firmware-tree.txt           "find /usr/lib/firmware -type d 2>/dev/null | sort"
capsh 73-firmware-samsung.txt        "ls -la /usr/lib/firmware/qcom/samsung/w767/ 2>&1"
capsh 74-firmware-lenovo.txt         "ls -la /usr/lib/firmware/qcom/sc8180x/LENOVO/82AK/ 2>&1"
capsh 75-firmware-dmesg.txt          "dmesg | grep -iE 'firmware|request_firmware|Direct firmware' | head -100"

# ---- USB topology details (keyboard / internal hub) -------------------------
capsh 80-usb-devices.txt             'for d in /sys/bus/usb/devices/*; do [ -d "$d" ] || continue; n=$(basename "$d"); echo "=== $n ==="; for f in idVendor idProduct product manufacturer serial bNumInterfaces speed bcdUSB; do [ -e "$d/$f" ] && printf "  %-16s %s\n" "$f" "$(cat $d/$f 2>&1)"; done; echo; done'
capsh 81-usb-input.txt               'for d in /sys/class/input/input*; do [ -d "$d" ] || continue; echo "=== $d ==="; for f in name phys id/vendor id/product id/bustype; do [ -e "$d/$f" ] && printf "  %-14s %s\n" "$f" "$(cat $d/$f 2>&1)"; done; echo; done'
capsh 82-hid-devices.txt             'for d in /sys/bus/hid/devices/*; do [ -d "$d" ] || continue; echo "=== $d ==="; cat "$d/uevent" 2>&1; echo; done'

# ---- SPMI / PMIC / battmgr ---------------------------------------------------
capsh 90-spmi.txt                    'for d in /sys/bus/spmi/devices/*; do [ -d "$d" ] || continue; echo "=== $d ==="; cat "$d/uevent" 2>&1; echo; done'
capsh 91-pmic-glink.txt              'find /sys/devices/platform/pmic-glink -maxdepth 3 2>/dev/null | head -80'
capsh 92-battery.txt                 "upower -d 2>&1 | head -80"

# ---- Thermal -----------------------------------------------------------------
capsh 93-thermal.txt                 'for d in /sys/class/thermal/thermal_zone*; do [ -d "$d" ] || continue; printf "%-30s type=%s temp=%s\n" "$(basename $d)" "$(cat $d/type 2>&1)" "$(cat $d/temp 2>&1)"; done'

# ---- SoC info ----------------------------------------------------------------
capsh 94-socinfo.txt                 'for f in /sys/devices/soc0/* /sys/bus/soc/devices/soc0/*; do [ -f "$f" ] && printf "%-40s %s\n" "$f" "$(cat $f 2>&1)"; done'
capsh 95-iommu.txt                   'for d in /sys/class/iommu/*; do [ -d "$d" ] || continue; echo "=== $d ==="; find "$d" -maxdepth 2 -type f -exec bash -c "printf \"  %s \" \"\$1\"; cat \"\$1\" 2>&1" _ {} \;; done | head -100'

# ==============================================================================
# LIVE TESTS (interactive-ish — user observes screen while we poke things)
# ==============================================================================

# ---- Backlight dance ---------------------------------------------------------
if [ -d /sys/class/backlight/backlight ]; then
  {
    BL=/sys/class/backlight/backlight
    MAX=$(cat $BL/max_brightness 2>/dev/null)
    ORIG=$(cat $BL/brightness 2>/dev/null)
    ORIG_POWER=$(cat $BL/bl_power 2>/dev/null)
    printf "=== initial state ===\nbrightness=%s max=%s bl_power=%s\n\n" "$ORIG" "$MAX" "$ORIG_POWER"
    printf "=== test 1: ramp brightness 0 → max ===\n"
    for level in 0 $((MAX/8)) $((MAX/4)) $((MAX/2)) $((MAX*3/4)) "$MAX"; do
      printf "%s set brightness=%s (readback=%s, bl_power=%s)\n" "$(date +%T.%N)" "$level" "$(cat $BL/actual_brightness 2>/dev/null || cat $BL/brightness)" "$(cat $BL/bl_power)"
      echo "$level" > $BL/brightness 2>&1
      sleep 2
    done
    printf "\n=== test 2: toggle bl_power ===\n"
    for p in 0 1 4 0; do
      printf "%s set bl_power=%s (readback=%s)\n" "$(date +%T.%N)" "$p" "$(cat $BL/bl_power)"
      echo "$p" > $BL/bl_power 2>&1
      sleep 2
    done
    printf "\n=== restoring ===\n"
    echo "${ORIG_POWER:-0}" > $BL/bl_power 2>&1
    echo "${ORIG:-$MAX}"    > $BL/brightness 2>&1
    printf "end brightness=%s bl_power=%s\n" "$(cat $BL/brightness)" "$(cat $BL/bl_power)"
  } > LIVE-backlight-dance.txt 2>&1
  echo "  LIVE-backlight-dance.txt        done"
else
  echo "backlight device missing" > LIVE-backlight-dance.txt
fi

# ---- DRM connector enable dance ----------------------------------------------
{
  echo "=== /sys/class/drm/card0-*/status right now ==="
  for d in /sys/class/drm/card0-*; do [ -d "$d" ] || continue; printf "  %s status=%s enabled=%s\n" "$(basename $d)" "$(cat $d/status 2>&1)" "$(cat $d/enabled 2>&1)"; done
  echo
  echo "=== fbcon cursor blink (indirect check console is alive) ==="
  cat /sys/class/graphics/fbcon/cursor_blink 2>&1
} > LIVE-drm-state.txt 2>&1
echo "  LIVE-drm-state.txt              done"

# ---- Try unblanking via setterm/vbetool (may or may not exist) ----------------
{
  echo "=== setterm unblank ==="
  setterm -blank 0 -powersave off >/dev/null 2>&1 && echo "setterm done" || echo "setterm failed"
  echo
  echo "=== vbetool dpms on ==="
  vbetool dpms on >/dev/null 2>&1 && echo "vbetool done" || echo "vbetool not available"
} > LIVE-unblank-attempts.txt 2>&1

# ==============================================================================
# TARBALL
# ==============================================================================
sync
cd /boot/efi
tar czf "audit-$TS.tar.gz" "audit-$TS" 2>&1
sync
echo ""
echo "==> Audit tarball: /boot/efi/audit-$TS.tar.gz"
echo "==> Individual files under: /boot/efi/audit-$TS/"
ls -la "/boot/efi/audit-$TS.tar.gz"
echo ""
echo "Next:"
echo "  poweroff     (to cleanly shut down)"
echo "  then unplug the USB and bring it to the Fedora host"
