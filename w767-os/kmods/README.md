# Out-of-tree kernel modules

Drivers we're iterating on out-of-tree before upstreaming. Each module lives in its own subdirectory and builds via the standard `M=$PWD modules` kbuild flow against `../linux/`.

## Planned

| Dir                  | Target driver                                              | Phase 3 priority |
| -------------------- | ---------------------------------------------------------- | ---------------- |
| `hid-samsung-w767/`  | Samsung USB-composite KB quirks (04E8:A055)                | 1 — safety net   |
| `ec-sam0604/`        | Samsung EMEC EC platform driver (battery/thermal/lid)      | 4                |

## Iteration pattern

```bash
cd w767-os/kmods/hid-samsung-w767
make -C ../../../linux M=$PWD ARCH=arm64 CROSS_COMPILE=aarch64-linux-gnu- modules

# Push + reload via the control daemon
W767_HOST=root@<ip>
rsync -a hid_samsung_w767.ko $W767_HOST:/lib/modules/$(ssh $W767_HOST uname -r)/extra/
ssh $W767_HOST 'depmod -a && w767_ctl_cli rmmod hid_samsung_w767 || true && w767_ctl_cli modprobe hid_samsung_w767'
ssh $W767_HOST 'w767_ctl_cli dmesg-tail 60'
```

(`w767-os/scripts/iterate-driver.sh` wraps this for in-tree drivers too.)
