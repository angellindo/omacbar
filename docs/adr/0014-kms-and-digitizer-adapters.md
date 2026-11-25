# Linux Touch Bar ioctls use scoped rustix boundaries

The Touch Bar adapters in `tb-daemon` use `rustix` for Linux device ioctls. Their unsafe code is
limited to the device boundary. The DRM adapter confines ioctls and dumb-buffer mappings to
`drm/card.rs`, with kernel layouts asserted in `drm/uapi.rs`. The digitizer confines evdev
ioctls to `digitizer/device.rs`, with size assertions for Linux `input_id` and `input_absinfo`.
Discovery, calibration, event parsing and touch handling are safe Rust.

Digitizer discovery takes supplied input-device text. The real open path compares the opened
character device's major and minor and the T1 identity with sysfs before reading evdev metadata
on that same descriptor. It grabs the descriptor only after the reported identity, name and axis
range match a verified profile. Closing the descriptor releases the grab. TB-45 owns recording
the first verified profile. The T1 hardware sources are
[research 0001](../research/0001-t1-touch-bar-hardware.md) and
[research 0010](../research/0010-ewweberlin-touchbar-control-path.md).

The Linux UAPI defines the evdev metadata and grab opcodes. The evdev handler treats the grab
argument as a Boolean and can return an ABS entry even if that axis is absent, so the metadata
reader checks the capability bitmap before reading ranges. The adapter reads the name and
capabilities without querying a device's unique identifier. [Linux v6.15 evdev implementation](https://github.com/torvalds/linux/blob/v6.15/drivers/input/evdev.c#L968-L1147)

Tests exercise discovery, calibration and MT-B reports with synthetic inputs and the descriptor
boundary with an ordinary file. Verifying the real ioctl success path and orientation requires
the T1 hardware.

Device runs block SIGHUP, SIGINT and SIGTERM before GPU initialization or worker creation.
A dedicated `sigwait` thread receives those signals, so no signal handler touches daemon state.
Closing a shutdown pipe wakes the daemon's file-descriptor readers and inotify watches so they
can release their descriptors. The `stop_signals.rs` boundary uses libc for masking, waiting
and waking the signal thread. Only a tapped command's child restores the caller's original
signal mask before exec.
