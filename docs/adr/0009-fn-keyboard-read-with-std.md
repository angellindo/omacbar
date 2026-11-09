# The Fn keyboard is read with std: capabilities from /proc text, events as raw records

`touchbard` follows the held Fn and modifier keys through a `ModifierSource`. The evdev adapter
needs two things from the kernel: which input device reports `KEY_FN` (research 0002: the Apple SPI
keyboard, found by capability, never by event number) and that device's key events. We get both
without a new dependency and without `unsafe` (R7):

- **Which device:** the text of `/proc/bus/input/devices`. Each device lists its name, its handlers
  (`eventN`) and its key capability bitmap (`B: KEY=`, 64-bit hex words, most significant first).
  Picking the keyboard is a pure function over that text, so it is tested on synthetic text and no
  test ever reads the machine's devices.
- **Its events:** `/dev/input/eventN` opened with `std::fs::File` and read as 24-byte
  `struct input_event` records (x86_64). Parsing and the held-key state are generic over
  `std::io::Read`, so tests feed synthetic records from memory, a pipe or a file.

Reading the keyboard is opt-in (`touchbard --keys auto|PATH`): without the flag the daemon opens no
input device, which keeps every agent-run test of the binary off `/proc` and `/dev`
(hardware is the human's).

Consequences, accepted for the POC: without the `EVIOCGKEY` ioctl the daemon cannot read which keys
are already down when it starts (a held key is seen at its first autorepeat, which the SPI keyboard
sends) nor resynchronise after `SYN_DROPPED`; without `EVIOCGRAB` the reader is passive, so held
modifiers still reach the compositor (the modifier leak, TB-11) and the keyboard keeps working if
the daemon dies.

## Considered options

- **`evdev` crate.** Ioctls, capability queries and initial key state in a safe API, but several
  new packages (`nix`, `bitvec`, ...) not in `Cargo.lock` or the local registry, for one bitmap
  test and one record format.
- **`input-linux` crate.** Same trade, fewer users.
- **`rustix::ioctl` for `EVIOCGBIT`/`EVIOCGKEY`.** `rustix` is already locked (ADR-0007), but its
  ioctl entry points are `unsafe`: R7 would need a scoped allow per call for what the text and the
  record stream already give us.
- **Read the keyboard by default.** Every existing `touchbard` test would open the machine's
  keyboard. The service unit can pass `--keys auto` when it exists.
