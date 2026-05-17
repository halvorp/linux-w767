// SPDX-License-Identifier: GPL-2.0
/*
 * hid-samsung-w767 — diagnostic HID driver for the Samsung Galaxy Book S
 *                     internal USB-composite keyboard (VID 04E8, PID A055).
 *
 * Starting point for Phase 3. At this stage we only *log* probe events so we
 * can confirm the device is being claimed and see its report descriptor. Once
 * we have evidence of what's actually wrong (wrong key mappings, missing
 * Fn-layer, broken consumer controls, etc.), swap the probe/raw_event hooks
 * for the real fixes.
 *
 * Intentionally separate from drivers/hid/hid-samsung.c so we can iterate
 * in-place on the device (rmmod/insmod this module, don't touch the in-tree
 * one) without bouncing the whole kernel.
 */

#include <linux/module.h>
#include <linux/kernel.h>
#include <linux/hid.h>

#define USB_VENDOR_ID_SAMSUNG     0x04E8
#define USB_PRODUCT_ID_W767_KBD   0xA055

static int w767_hid_probe(struct hid_device *hdev, const struct hid_device_id *id)
{
	int ret;

	hid_info(hdev, "w767: probe vid=%04x pid=%04x (%s)\n",
		 id->vendor, id->product, hdev->name);

	ret = hid_parse(hdev);
	if (ret) {
		hid_err(hdev, "w767: hid_parse failed: %d\n", ret);
		return ret;
	}

	ret = hid_hw_start(hdev, HID_CONNECT_DEFAULT);
	if (ret) {
		hid_err(hdev, "w767: hid_hw_start failed: %d\n", ret);
		return ret;
	}

	hid_info(hdev, "w767: attached (report descriptor size=%u)\n",
		 hdev->dev_rsize);
	return 0;
}

static void w767_hid_remove(struct hid_device *hdev)
{
	hid_info(hdev, "w767: detaching\n");
	hid_hw_stop(hdev);
}

static int w767_raw_event(struct hid_device *hdev, struct hid_report *report,
			  u8 *data, int size)
{
	/* Log first N bytes at debug level so we can watch for pattern changes
	 * without flooding dmesg. Enable via:
	 *   echo 'module hid_samsung_w767 +p' > /sys/kernel/debug/dynamic_debug/control
	 */
	hid_dbg(hdev, "w767: raw_event len=%d %*phN\n",
		size, min(size, 16), data);
	return 0;
}

static const struct hid_device_id w767_hid_table[] = {
	{ HID_USB_DEVICE(USB_VENDOR_ID_SAMSUNG, USB_PRODUCT_ID_W767_KBD) },
	{ }
};
MODULE_DEVICE_TABLE(hid, w767_hid_table);

static struct hid_driver w767_hid_driver = {
	.name       = "hid-samsung-w767",
	.id_table   = w767_hid_table,
	.probe      = w767_hid_probe,
	.remove     = w767_hid_remove,
	.raw_event  = w767_raw_event,
};
module_hid_driver(w767_hid_driver);

MODULE_LICENSE("GPL");
MODULE_DESCRIPTION("Samsung Galaxy Book S (W767) HID composite KB/TP diagnostic driver");
