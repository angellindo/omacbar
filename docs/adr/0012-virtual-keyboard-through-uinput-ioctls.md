---
status: accepted (2026-11-09, TB-18)
---

# The virtual keyboard is made with four uinput ioctls through rustix, and only behind `--emit`

`touchbard` presses the keys of `key` actions on a virtual keyboard: a uinput device (research
0002, the tiny-dfr pattern). ADR-0009 read the Fn keyboard with std alone. The same approach cannot
create a uinput device. Writing events to `/dev/uinput` is plain `write(2)`, but the kernel accepts
them only after `UI_SET_EVBIT`, `UI_SET_KEYBIT` and `UI_DEV_CREATE`, and those are ioctls
(`linux/uinput.h`). The legacy setup path, which writes a `struct uinput_user_dev`, needs the same
three ioctls. std has no ioctl, so creating the device needs `unsafe` (R7).

The device is created with `rustix::ioctl`, and nothing else uses `unsafe`:

- **Create (unsafe, one module).** `tb-daemon`'s `uinput` module opens `/dev/uinput` with
  `std::fs::OpenOptions` and makes four ioctl calls through `rustix::ioctl`: `UI_SET_EVBIT`
  (`EV_KEY`), `UI_SET_KEYBIT` once per key in `tb_core::Key::all()`, `UI_DEV_SETUP` (name
  `touchbard`, `BUS_VIRTUAL`) and `UI_DEV_CREATE`. That module alone has a scoped
  `#[allow(unsafe_code)]`, and every block has a `// SAFETY:` comment naming the opcode, its
  argument type in `linux/uinput.h`, and why the argument outlives the call. `rustix` 1.x is
  already a workspace dependency and is in `Cargo.lock` (ADR-0007). `rustix::ioctl` needs no
  feature, so no package is added.
- **Destroy (no ioctl).** The device is destroyed when its file is closed: the kernel's uinput
  release destroys it and releases its held keys (research 0008, stuck-modifier recovery,
  `misc/uinput.c`). std opens files `O_CLOEXEC`, so a command the daemon spawns never keeps the
  device alive.
- **Keys (std).** A key action is written as 24-byte `struct input_event` records (`x86_64`, the
  layout `keys.rs` reads): each key pressed in order, then released in reverse, with a
  `SYN_REPORT` after each, all in one `write_all`. The record writer takes any `std::io::Write`.
  The same code therefore writes to the device, to a file (`--emit PATH`) and to memory in tests.
  Only the four ioctls go untested in CI; the human checks them by hand (HIL).

Creating the virtual keyboard is opt-in, like reading the Fn keyboard (ADR-0009):
`touchbard --emit uinput` creates it, and `--emit PATH` writes the records to PATH. Without the
flag the daemon opens no device, so no agent-run test touches `/dev/uinput`. When the device cannot
be created (on this machine `/dev/uinput` is `root:root 0600`, research 0002, and access is
TB-20), the reason is logged and key actions do nothing. Every tapped key action then logs that
its keys were not pressed, and commands and the bar keep working.

## Considered options

- **std only.** Impossible: the setup ioctls above have no `write(2)` form.
- **`evdev` crate (`VirtualDeviceBuilder`).** A safe API, but it brings several packages (`nix`,
  `bitvec`, ...) that are neither in `Cargo.lock` nor in the local registry, all for four calls.
  ADR-0009 rejected it for the same reason.
- **`uinput` / `input-linux` crates.** The same trade, with fewer users; `uinput` also links
  libudev.
- **`libc::ioctl`.** It is locked too, but it is variadic and untyped. `rustix`'s typed
  `Setter`/`IntegerSetter`/`NoArg` patterns state each opcode's argument type in one place.
- **An external tool (`ydotool`, `wtype`).** `ydotoold` needs uinput access itself, plus a socket
  and a second service. `wtype` uses the Wayland virtual-keyboard protocol, which leaks modifiers
  in the same way (research 0008 §1).
- **Hyprland `send_shortcut` for every key.** ADR-0011 rejects it: it bypasses Hyprland binds
  (plain `F9` dictation) and the input method.

## Consequences

- On a tap, the keys are pressed and released at once. Holding a button neither holds nor repeats
  its key, so `KeyEmitter::emit` has no separate press and release. A hold mode would change that
  interface.
- No delay is inserted between press and release, so the daemon loop never sleeps. Whether every
  client registers a zero-length press is unverified. The live checks in research 0008 spaced the
  events 50 ms apart. The HIL check of TB-18 settles it.
