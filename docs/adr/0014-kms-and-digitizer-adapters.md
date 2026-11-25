# Opt-in Linux Touch Bar display and digitizer adapters

The Touch Bar adapters in `tb-daemon` use `rustix` for Linux device ioctls. Their unsafe code is
limited to the device boundary. The DRM adapter confines ioctls and dumb-buffer mappings
to `drm/card.rs`, uses the `rustix` `mm` feature for mappings and asserts kernel layouts in
`drm/uapi.rs`. The digitizer confines evdev
ioctls to `digitizer/device.rs`, with size assertions for Linux `input_id` and `input_absinfo`.
Discovery, calibration, event parsing and touch handling are safe Rust.

## Display ownership and restore

Before opening a card, the daemon checks its sysfs driver binding and T1 iBridge USB ancestry.
Both explicit and automatic selection use those checks. After open it rechecks the ancestry
and matches the character device's major and minor to sysfs, then confirms the DRM driver name
on the descriptor. Display ownership requires DRM master. A busy card is refused before
modesetting.

The daemon uses legacy KMS for a single connector, CRTC and primary plane. It reads the plane's
formats and the connector's mode and orientation, converts the RGBA frame into a dumb buffer
using the returned pitch, and sends the first frame with `SETCRTC`. Later presents use
`DIRTYFB` for the rotated damage. The driver already routes these through
`drm_atomic_helper_set_config` and `drm_gem_fb_create_with_dirty`, respectively. Userspace needs
no atomic property requests for this path.
[appletbdrm driver source](https://github.com/gregkh/linux/blob/497791a0a96b866d90add64041c47acdff49cae6/drivers/gpu/drm/tiny/appletbdrm.c)

A foreign framebuffer's lifetime belongs to its owner. The daemon refuses an active prior
presentation before any modeset, including a bound connector or a CRTC or plane with active
state. After its own modeset, restore attempts to disable the CRTC to return to the original
inactive KMS state. It then releases the framebuffer, mapping, buffer and master, and reports
restore failures. An exit before the first modeset only needs resource release.

The T1 candidate retains the driver's update and disable callbacks, which discard the USB flush
and clear results. A successful present means the frame was submitted to the driver. Restoring
the inactive KMS state also gives no evidence of what's visible. TB-10 needs a human to observe
the lit frame and the later return of the firmware key row.
[Driver callbacks](https://github.com/gregkh/linux/blob/497791a0a96b866d90add64041c47acdff49cae6/drivers/gpu/drm/tiny/appletbdrm.c#L474-L505)
· [Pinned T1 patch](https://github.com/Seborider/mbp-t1-touchbar/blob/3a00d8dceb2cfeb75c3e6b9f501ead095c1d9924/kernel/patches/appletbdrm-t1.patch)

## Digitizer ownership

Digitizer discovery takes supplied input-device text. The real open path compares the opened
character device's major and minor and the T1 identity with sysfs before reading evdev metadata
on that same descriptor. It grabs the descriptor only after the reported identity, name and axis
range match a verified profile. Closing the descriptor releases the grab. TB-45 owns recording
the first verified profile. Until then, `VERIFIED_DIGITIZERS` is empty and even a matching
T1 candidate is refused before the grab. Its reported axis bounds alone cannot establish the
axis along the bar or its direction. See [research 0001](../research/0001-t1-touch-bar-hardware.md) and
[research 0010](../research/0010-ewweberlin-touchbar-control-path.md) for the T1 hardware sources.

The Linux UAPI defines the evdev metadata and grab opcodes. The evdev handler treats the grab
argument as a Boolean and can return an ABS entry even if that axis is absent, so the metadata
reader checks the capability bitmap before reading ranges. The adapter reads the name and
capabilities without querying a device's unique identifier. [Linux v6.15 evdev implementation](https://github.com/torvalds/linux/blob/v6.15/drivers/input/evdev.c#L968-L1147)

After taking the grab the daemon changes the clock to monotonic on that descriptor
(`EVIOCSCLOCKID`). Linux replaces queued events, if any, with a `SYN_DROPPED` marker. A reset
failure closes the descriptor and releases the grab. The daemon then wraps the file in its
interruptible reader. The live stream can begin on an implicit slot because evdev emits
selectors when the slot changes. After a dropped report the adapter cancels the current touch,
discards through the next `SYN_REPORT` and resumes on an implicit slot. A new tracking ID and a
fresh position are required to follow a contact. A sample before its ID counts only if a prior
lift established that the slot was idle and the sample belongs to the same report as the ID.
When a later slot change makes the implicit contact ambiguous, the adapter cancels it.
Raw replay requires an explicit selector at startup and after lost sync.
[Linux v6.15 queue reset](https://github.com/torvalds/linux/blob/v6.15/drivers/input/evdev.c#L162-L198)
· [Linux v6.15 slot selection](https://github.com/torvalds/linux/blob/v6.15/drivers/input/input.c#L144-L190)

Tests exercise discovery, calibration and MT-B reports with synthetic inputs and the descriptor
boundary with an ordinary file. Verifying the real ioctl success path and orientation requires
the T1 hardware.

## Interruption

Device runs block SIGHUP, SIGINT and SIGTERM before GPU initialization or worker creation.
A dedicated `sigwait` thread receives those signals, so no signal handler touches daemon state.
Closing a shutdown pipe wakes the daemon's file-descriptor readers and inotify watches so they
can release their descriptors. The `stop_signals.rs` boundary uses libc for masking, waiting
and waking the signal thread. Only a tapped command's child restores the caller's original
signal mask before exec.

## Considered options

- **tiny-dfr's card probe.** The reference opens candidate card nodes and tries to acquire master
  before testing the display geometry. Sysfs identity first keeps unrelated cards out of device
  discovery. [tiny-dfr display backend](https://github.com/AsahiLinux/tiny-dfr/blob/a79f2f43e6c6a9b855968e54eb243c20642ebc89/src/display.rs#L62-L187)
- **Atomic KMS.** Atomic commits would add property lookup and request assembly. A single buffer
  and an inactive starting state already have the needed modeset, damage and restore operations
  through legacy KMS.
- **The `drm` and `evdev` crates.** These wrappers would add dependencies for the device calls.
  The existing `rustix` and `libc` packages cover the selected Linux interfaces, with scoped
  unsafe code and the kernel argument layouts checked in the adapters.
- **CLI or config calibration.** An operator-supplied axis and range could authorize a grab using
  a guessed orientation or another model's mapping. Require an exact profile backed by TB-45
  observations.
- **A signal handler writing to a self-pipe.** The `sigwait` thread can post a stop message on
  the daemon's channel. The shutdown pipe wakes readers during cleanup, outside signal handling.

The [HIL checklist](../hil-checklist.md#opt-in-linux-adapters-tb-47) describes the operator's
preconditions and the hardware evidence required for TB-10 and TB-45.
