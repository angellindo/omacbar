# 0001 · T1 Touch Bar hardware: resolution, display path, limits

Status: resolved (TB-1) · Machine: `MacBookPro14,3` (15" 2017, Apple T1), Omarchy, kernel 7.2.15

## Question

Can a Linux userspace program draw arbitrary pixels on this Touch Bar, at what resolution, and
what does it cost?

## Findings

### The bar is a 2170 x 60 px panel

- Physical panel: **2170 x 60 px**, landscape (1085 x 30 pt at Apple's 2x). The display interface
  reports it as a **60 x 2170 framebuffer** (portrait), so frames must be **rotated 90°** before
  they are sent. The ESC area is part of the same panel. [barkeep]
- Pixel format on the wire: 3 bytes per pixel (RGB). One frame is 2170 x 60 x 3 = 390,600 bytes. [barkeep]
- Brightness: HID feature reports on the iBridge (not the panel), 0-100 mapped by the driver. [barkeep]
- Physical size / DPI: **not yet measured** (the display's `GINF` reply carries width and height in
  inches; read it in TB-9). Until then, design in pixels only.

### The iBridge has three USB configurations; only config 2 has the display

Read from `/sys/bus/usb/devices/1-3/descriptors` on this machine (`05ac:8600 iBridge`):

| Config | Interfaces | What it gives |
|---|---|---|
| **1 (active today)** | UVC camera x2, HID keyboard, HID (ALS/backlight) | Firmware-drawn key mode only. No pixels. |
| **2** | UVC x2, HID digitizer (EP 0x83), **class 0x10 display (bulk OUT 0x02, IN 0x85)**, CDC-NCM, HID backlight, vendor 0xff/f9 (Secure Enclave, Touch ID) | **Custom pixels + multitouch.** |
| 3 | UVC x2, HID keyboard, HID, CDC-NCM | Key mode + network |

Today the machine runs `apple-ib-drv-dkms` (`apple_ibridge`, `apple_touchbar`, `fnmode=2`): config 1,
the T1 firmware draws a fixed layout (ESC / F-keys / special keys) and the host only picks which.
**No custom graphics are possible in config 1.** [cschaba, live descriptors]

### Two community paths to config 2

| Path | Display device | Touch | Status |
|---|---|---|---|
| **Seborider/mbp-t1-touchbar** | patched mainline `appletbdrm` (adds `05ac:8600` + T1 dialect: 16-byte reply preamble, acks one frame behind) → a real **DRM device** | patched `hid-multitouch` (`MT_CLS_APPLE_TOUCHBAR`) → evdev | Tested on MacBookPro13,2 + Omarchy + Hyprland; needs Linux ≥ 6.15 |
| **aeonsolus/barkeep** | own module → **`/dev/dfr0` char device** (write one raw frame) | hidraw → uinput | Tested on MacBookPro13,2; suspend hangs (unresolved) |

**macOS itself prefers config 2** for this device (`AppleUSBiBridge.kext`:
`kUSBPreferredConfiguration = 2`, display at config 2 interface 3; see research 0006), so pixel mode
is the T1's normal operating mode, not an unsupported one.

**How config 2 is reached (corrected in TB-46).** A kernel enumeration callback, a USB device
driver's `choose_configuration`, would pick the configuration when the kernel enumerates the
device. No such driver exists for `05ac:8600` here, so the kernel's generic rule picks config 1
([`usb_choose_configuration`, stable 497791a0](https://git.kernel.org/pub/scm/linux/kernel/git/stable/linux.git/tree/drivers/usb/core/generic.c?id=497791a0a96b866d90add64041c47acdff49cae6#n52))
and the packaged udev rule writes `bConfigurationValue=1` again. Seborider at the pinned revision
`3a00d8dceb2cfeb75c3e6b9f501ead095c1d9924` selects nothing at enumeration: its bring-up script
[`system/touchbar-t1-setup`](https://github.com/Seborider/mbp-t1-touchbar/blob/3a00d8dceb2cfeb75c3e6b9f501ead095c1d9924/system/touchbar-t1-setup),
run by a oneshot `touchbar-t1.service`, loads the modules, registers the digitizer's HID id
dynamically through `new_id`, and only then writes `2` to the iBridge's sysfs
`bConfigurationValue`, waits for interface 2.3's driver and rebinds the digitizer. Its
`docs/touchid.md` §4 says an enumeration-time selector closes the `new_id` window and touch stops
working. barkeep's path is not inferred from Seborider's and is not researched here. A userspace
configuration write does not stick while `apple_ibridge` is loaded, because it forces config 1
back at HID probe, so the key-mode driver must be unloaded or replaced first.

**Compile-only evidence (TB-46, 2026-11-22).** Against `7.2.15-1.1-omarchy-mbp143ab`, stock
v7.2.15 `appletbdrm.c` and `hid-appletb-bl.c` patched with Seborider's two T1 patches compile and
modpost out of tree with zero warnings; Seborider's shipped `kernel/appletbdrm.c` (the file its
installer hands to DKMS) fails on the removed `drm_atomic_state` plane-helper signatures. An
isolated module build proves nothing about loading, binding, a frame or a return to key mode.
Seat/owner isolation must hold before the T1 DRM card can appear, and real device paths stay
human-only. Restoring service files, booting a known-good kernel with its module tree, and
recovering the T1 firmware are three different recoveries. Physical return to config 1 after a
pixel-mode trial is unverified. Pinned inputs, hashes and the exact command:
[hardware runbook](../hardware-runbook.md#driver-compile-experiment-compile-only).

### Hard prerequisites and risks

- The T1 firmware (`EFI/APPLE/EMBEDDEDOS/combined.memboot`, device-specific) must be intact; without
  it the device enumerates as `05ac:1281` (recovery) and the bar, camera and ALS all die. **Back up
  the ESP first.** [cschaba, Seborider]
- `FDRData` in the ESP is needed for any future Touch ID work and cannot be regenerated. Back it up.
- Hyprland must never claim the Touch Bar DRM card (do not put it in `AQ_DRM_DEVICES`; seat rules
  keep it off the main seat). [Seborider]
- Kernel updates rebuild DKMS modules; a patched out-of-tree driver is a maintenance cost.
- Suspend/resume with the display driver loaded is not proven on this model.
- **This machine has lost its T1 firmware before** (ESP without `EMBEDDEDOS`, device at `05ac:1281`)
  and needed a flash-writing recovery. The restored set is backed up only on the internal SSD
  (research 0006). An external copy is a hard gate for TB-9.
- A local `macbook-touchbar-local.service` controls the bar in key mode and must be retired first.

## Decision input

The POC is viable **only** through config 2. The app must not care which path is used: it renders
RGBA frames and hands them to a `Display` backend (DRM dumb buffer, `/dev/dfr0`, or a PNG/window
simulator). See ADR-0002.

## Sources

- Live: `/sys/bus/usb/devices/1-3/descriptors`, `lsmod`, `/sys/module/apple_touchbar/parameters`
- [aeonsolus/barkeep](https://github.com/aeonsolus/barkeep) (protocol, 2170x60, rotation, `/dev/dfr0`)
- [Seborider/mbp-t1-touchbar](https://github.com/Seborider/mbp-t1-touchbar) (appletbdrm + hid-multitouch patches, config 2 has everything)
- [cschaba/macbookpro14-3-linux](https://github.com/cschaba/macbookpro14-3-linux) (this exact model in key mode, firmware warnings)
- [RagingEnby/omarchy-t1-touchbar](https://github.com/RagingEnby/omarchy-t1-touchbar), [nohzafk/omarchy-macbookpro-t1](https://github.com/nohzafk/omarchy-macbookpro-t1)
- [AsahiLinux/tiny-dfr](https://github.com/AsahiLinux/tiny-dfr) (reference daemon: DRM dumb buffer, libinput touch, uinput keys)
