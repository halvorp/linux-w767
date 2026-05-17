obj-m += w767_audit.o
ccflags-y := -fno-stack-protector

KDIR := /home/peter/Documents/GalaxyBookS_Linux/target_headers/usr/src/kernels/7.0.0-62.fc45.aarch64
CROSS_COMPILE := aarch64-linux-gnu-
ARCH := arm64

all:
	make ARCH=$(ARCH) CROSS_COMPILE=$(CROSS_COMPILE) -C $(KDIR) M=$(PWD) modules

clean:
	make -C $(KDIR) M=$(PWD) clean
