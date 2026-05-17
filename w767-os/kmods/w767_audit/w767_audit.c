#include <linux/module.h>
#include <linux/kernel.h>
#include <linux/i2c.h>
#include <linux/acpi.h>
#include <linux/gpio/consumer.h>
#include <linux/delay.h>
#include <linux/slab.h>

#define DRIVER_NAME "w767_auditor"
#define TARGET_ADDR 0x2C

/* --- ACPI Interception --- */

static acpi_status samsung_acpi_handler(u32 function,
                                       acpi_physical_address address,
                                       u32 bit_width,
                                       u64 *value,
                                       void *handler_context,
                                       void *region_context)
{
    const char *op = (function == ACPI_READ) ? "READ" : "WRITE";
    const char *dev_name = (char *)handler_context;

    pr_info(DRIVER_NAME ": [ACPI] %s on %s: Offset=0x%llx, Width=%u, Val=0x%llx\n",
            op, dev_name, address, bit_width, *value);

    return AE_OK;
}

static void hook_acpi_device(const char *path, const char *label)
{
    acpi_handle handle;
    acpi_status status;
    
    status = acpi_get_handle(NULL, (acpi_string)path, &handle);
    if (ACPI_SUCCESS(status)) {
        /* Install for both SystemMemory and EmbeddedControl spaces */
        acpi_install_address_space_handler(handle, ACPI_ADR_SPACE_SYSTEM_MEMORY,
                                         &samsung_acpi_handler, NULL, (void *)label);
        acpi_install_address_space_handler(handle, ACPI_ADR_SPACE_EC,
                                         &samsung_acpi_handler, NULL, (void *)label);
        pr_info(DRIVER_NAME ": Hooked ACPI device %s (%s)\n", path, label);
    } else {
        pr_info(DRIVER_NAME ": ACPI device %s not found\n", path);
    }
}

/* --- I2C Auditing --- */

static void dump_i2c_device(struct i2c_adapter *adap, u8 addr)
{
    struct i2c_board_info info = {
        I2C_BOARD_INFO("audit_tmp", addr),
    };
    struct i2c_client *client = i2c_new_client_device(adap, &info);
    int i;

    if (IS_ERR(client)) return;

    pr_info(DRIVER_NAME ": --- Dumping 0x%02x on Bus %d ---\n", addr, adap->nr);
    for (i = 0; i < 256; i++) {
        s32 val = i2c_smbus_read_byte_data(client, i);
        if (val >= 0) {
            pr_info(DRIVER_NAME ": [0x%02x] = 0x%02x\n", i, val);
        }
    }
    i2c_unregister_device(client);
}

static void audit_i2c_bus(int bus_id)
{
    struct i2c_adapter *adap = i2c_get_adapter(bus_id);
    u8 addr;
    
    if (!adap) {
        pr_err(DRIVER_NAME ": I2C Bus %d not found\n", bus_id);
        return;
    }

    pr_info(DRIVER_NAME ": Scanning I2C Bus %d...\n", bus_id);
    for (addr = 0x03; addr < 0x78; addr++) {
        struct i2c_msg msg;
        u8 dummy;
        int ret;

        msg.addr = addr;
        msg.flags = I2C_M_RD;
        msg.len = 1;
        msg.buf = &dummy;

        ret = i2c_transfer(adap, &msg, 1);
        if (ret == 1) {
            pr_info(DRIVER_NAME ": Found device at 0x%02x on Bus %d\n", addr, bus_id);
            if (addr == TARGET_ADDR) {
                dump_i2c_device(adap, addr);
            }
        }
    }
    i2c_put_adapter(adap);
}

/* --- Module Lifecycle --- */

static int __init w767_audit_init(void)
{
    pr_info(DRIVER_NAME ": === Samsung W767 Hardware Audit Start ===\n");

    /* Common Samsung ACPI paths for ARM64 laptops */
    hook_acpi_device("\\_SB.SCAI", "SAM0101");
    hook_acpi_device("\\_SB.EMEC", "SAM0604");
    hook_acpi_device("\\_SB.GTPD", "Touchpad");

    /* Scan candidate I2C buses (referenced in ACPI/DTS) */
    audit_i2c_bus(17); /* Backlight controller bus */
    audit_i2c_bus(15); /* SSPN / 0x2C candidate */
    audit_i2c_bus(9);  /* Primary EC bus */
    audit_i2c_bus(1);  /* Touchscreen bus */

    return 0;
}

static void __exit w767_audit_exit(void)
{
    pr_info(DRIVER_NAME ": Audit module unloaded\n");
}

module_init(w767_audit_init);
module_exit(w767_audit_exit);

MODULE_LICENSE("GPL");
MODULE_AUTHOR("Gemini CLI");
MODULE_DESCRIPTION("Samsung Galaxy Book S (W767) Hardware Auditor");
