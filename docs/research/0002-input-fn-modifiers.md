# 0002 · Input: touch, Fn and modifier detection, key emission

Status: resolved with one open risk (TB-2) · follow-up spike: TB-11

## Findings

- **Fn and modifiers are visible.** `Apple SPI Keyboard` (`applespi`, event5 today) advertises
  `KEY_FN`, `KEY_LEFTCTRL`, `KEY_LEFTALT`, `KEY_LEFTSHIFT`, `KEY_LEFTMETA` (read from
  `/proc/bus/input/devices` capability bits). The iBridge HID keyboard does **not** report Fn.
  Find the keyboard by capability (`KEY_FN`), not by event number. tiny-dfr looks for Fn on a
  device named "Touch Bar"; on T1 it must read the SPI keyboard instead.
- **Touch** in config 2 comes from the iBridge digitizer (`hid-multitouch` with the T1 ID patch;
  absolute X across the bar). One finger is enough for the MVP; the panel reports multitouch.
- **Key emission** goes through a uinput virtual keyboard (tiny-dfr pattern). `/dev/uinput` is
  `root:root 0600` here, and `/dev/input/event*` is `root:input 0660`: the daemon needs a udev rule
  or the `input` group plus uinput access (TB-20).
- **Open risk: modifier leak.** On a `Fn + Ctrl` layer the user is still physically holding Ctrl
  when they tap. A `Key` action emitted from uinput is seen by the compositor together with the
  held Ctrl (Ctrl+F5 instead of F5). Releasing Ctrl on the virtual device does not release it on
  the physical one. Options: make modifier layers default to `Command` actions (not keys), or
  have the daemon compensate. Needs a spike on Hyprland (TB-11) before key actions on modifier
  layers are promised.

## Sources

- Live: `/proc/bus/input/devices`, `ls -l /dev/uinput /dev/input/event5`
- [AsahiLinux/tiny-dfr](https://github.com/AsahiLinux/tiny-dfr) `src/main.rs` (Fn layer switch, uinput)
- [aeonsolus/barkeep](https://github.com/aeonsolus/barkeep) (SPI keyboard found by `KEY_FN`)
