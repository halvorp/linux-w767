# Out-of-tree build of hid-samsung-w767.
#
# Usage (from this directory):
#   make                           # default KDIR = ../../../linux
#   make KDIR=/path/to/linux
#
# Deploys via scripts/iterate-driver.sh for the live rmmod/modprobe dance.

KDIR ?= $(CURDIR)/../../../linux
ARCH ?= arm64
CROSS_COMPILE ?= aarch64-linux-gnu-

obj-m := hid_samsung_w767.o

all:
	$(MAKE) -C $(KDIR) M=$(CURDIR) ARCH=$(ARCH) CROSS_COMPILE=$(CROSS_COMPILE) modules

clean:
	$(MAKE) -C $(KDIR) M=$(CURDIR) clean
