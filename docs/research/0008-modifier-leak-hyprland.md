# 0008 · Modifier leak on Hyprland: keys emitted while a modifier is held

Status: predicted from source, waiting for the human's run (TB-11) · Machine: `MacBookPro14,3`, kernel
`7.2.15-1-omarchy`, Hyprland 0.56.2 (commit `efb5099`, Lua config), libinput 1.32.0, libxkbcommon
1.13.2 · Investigated read-only on 2026-11-09. Every prediction below is **unverified** until the
human posts the results of § HIL run.

## Question

When the daemon emits a key (F5) from a uinput keyboard while the user physically holds Ctrl (the
`fn-ctrl` layer), what does the focused Wayland client receive on Hyprland, and does releasing Ctrl
on the virtual device or sending the key with `send_shortcut` make the key exact? Answering it needs:
how Hyprland combines modifier state across keyboards, what the kernel and libinput do with a
virtual key release, what `send_shortcut` sends, and a live run on this machine.

Sources: `H` = Hyprland source at tag `v0.56.2` (`efb50993…`, the build `hyprctl version` reports),
paths under `src/`; `K` = Linux `v6.18` `drivers/input/`; `L` = libinput `1.32.0` `src/`;
`U` = systemd `v258` `src/udev/udev-builtin-input_id.c`; `W` = wev `1.1.0` `wev.c` (the Arch
package version). Live commands are quoted inline.

## Findings

### 1. Hyprland gives every client one merged modifier state

| Claim | Source | Confidence |
|---|---|---|
| Each keyboard (the Fn keyboard, a uinput keyboard) is a separate Hyprland keyboard with its own xkb state; a key updates only its own keyboard's state | `H` `devices/Keyboard.cpp:31-42` (`updateXkbStateWithKey` per keyboard) | high |
| On any keyboard's modifier change, Hyprland sends clients that keyboard's mods **ORed with the mods of every other keyboard** (`shareModsFromAllKBs`) | `H` `managers/input/InputManager.cpp:1685-1722` (merge at 1709, send at 1719), `1915-1932` | high |
| A key from any keyboard is sent with `setKeyboard` + `sendKeyboardKey` only; switching the active keyboard sends no modifiers event, and a keymap event only if the keymap text differs | `H` `managers/SeatManager.cpp:161-180`, `255-270`; `protocols/core/Seat.cpp:366-385` | high |
| So the client applies the last modifiers it was sent: physical Ctrl, then the virtual F5 arrives as Ctrl+F5 | follows from the three rows above | high |
| A key press already held on another keyboard is not resent, and a key release is dropped while any other keyboard still holds that key (`shareKeyFromAllKBs`, `m_pressed`) | `H` `InputManager.cpp:1641-1678`, `1891-1913` | high |
| Hyprland's own binds match against the merged mods (`getModsFromAllKBs` = `m_lastMods`) | `H` `managers/KeybindManager.cpp:380`; `InputManager.cpp:1711`, `1887-1889` | high |
| `share_states` (per-device opt-out) exists only for Wayland virtual-keyboard-protocol devices, and even then the other keyboards' mods are still ORed into that device's events | `H` `devices/VirtualKeyboard.cpp:49-51`; `config/legacy/ConfigManager.cpp:559` ("only for virtualkeyboards"); `InputManager.cpp:1915-1932` | high |
| New keyboards are not held back by Hyprland's permission system on this machine | `H` `InputManager.cpp:1244-1268`; live `hyprctl getoption ecosystem:enforce_permissions` → `bool: false` | high |

The leak reaches Hyprland binds too, with the live config: `hyprctl binds -j` lists
`XF86MonBrightnessUp` with no modifier as "Brightness up" and with Ctrl (modmask 4) as "Keyboard
brightness up". A `fn-ctrl` button that emits Brightness Up would therefore change the keyboard
backlight, not the screen.

### 2. A virtual Ctrl release cannot undo a physical Ctrl

| Claim | Source | Confidence |
|---|---|---|
| The kernel keeps a key bitmap per input device and passes a key event only if it changes that bit: a release of a key the uinput device never pressed is dropped before any reader sees it | `K` `input.c:235-247` (`input_get_disposition`, `test_bit(code, dev->key) != !!value`) | high |
| libinput counts key presses per device and forwards a release only when that device's count reaches 0; every forwarded key is posted (the seat-wide count is only informative) | `L` `evdev-fallback.c:34-50`; `libinput.c:2462-2485`, `2291-2311` | high |
| A virtual Ctrl press + release does change the uinput keyboard's own mods, but Hyprland ORs in the Fn keyboard's held Ctrl (§1), so clients keep seeing Ctrl; the virtual Ctrl key events themselves are swallowed by the held-key dedupe | `H` `InputManager.cpp:1641-1678`, `1915-1932` | high |
| A uinput device that advertises only Ctrl and F5 still becomes a keyboard: udev tags any key-capable device `ID_INPUT_KEY`, and libinput treats `ID_INPUT_KEY` as a keyboard | `U` lines 364-371; `L` `evdev.c:82-83` | medium (seen live in step 0 of the run) |

### 3. `send_shortcut` sets the client's mods explicitly

| Claim | Source | Confidence |
|---|---|---|
| With the Lua config (this machine: `~/.config/hypr/hyprland.lua`), `hyprctl dispatch X` evaluates `return hl.dispatch(X)`; the dispatcher is `hl.dsp.send_shortcut({ mods = "...", key = "...", window = "..."? })` | `H` `debug/HyprCtl.cpp:1126-1141`; `config/lua/bindings/LuaBindingsDispatchers.cpp:402-454`, `1404` | high |
| It resolves the key name through the active keyboard's keymap, then sends the focused surface: modifiers = exactly `mods`, key press, key release, then modifiers = 0 (depressed, latched **and locked**) | `H` `config/shared/actions/ConfigActions.cpp:1530-1598` (lines 1554, 1576) | high |
| It goes straight to the seat: Hyprland binds and the input method (fcitx5 runs here: `pgrep -a fcitx5`) never see the key | same; contrast `InputManager.cpp:1638-1678` | high |
| Until the next physical modifier change, the client believes no modifier and no lock (NumLock, Caps Lock) is active, even though Ctrl is still held: a physical Ctrl+C in that window arrives as plain `c` | `ConfigActions.cpp:1576`; resync on the next mod change via `InputManager.cpp:1685-1722` | medium-high |
| With the legacy hyprlang config the form would be `hyprctl dispatch sendshortcut ,F5,` (exactly three fields) | `H` `config/legacy/DispatcherTranslator.cpp:531-588` | high (not used here) |

### 4. Fn is not a modifier

`xkbcli compile-keymap --layout us --options omarchy:shift_caps_compose` (the rules
`hyprctl devices` reports for every keyboard) maps `KEY_FN` (keycode 472) to `XF86Fn`, and
`XF86Fn` is in no `modifier_map` (Shift, Lock, Control, Mod1-Mod5 only). So holding Fn alone adds
no modifier bit, and a key emitted on the `fn` layer arrives exact (high). Whether the client also
gets an `XF86Fn` key press depends on whether `applespi` reports Fn as a key event while
`fnmode=2` (unverified; seen in step 4 of the run).

## Predictions for the Acceptance experiments

wev prints keyboard events as `key: <xkb keycode>; state: 1 (pressed)` and
`depressed: <hex>: <names>` / `latched` / `locked` (`W` lines 383-430). F5 is keycode 71, Left Ctrl
37, Control is bit `00000004`, NumLock (locked, on here) is `00000010: Mod2`.

| # | Experiment | Prediction | Confidence |
|---|---|---|---|
| 0 | `f5`, nothing held (control) | `key: 71` pressed/released, no modifiers event, depressed stays `00000000` | high |
| 1 | `f5` while physical Left Ctrl is held | `key: 37` + `depressed: 00000004: Control`, then `key: 71` pressed/released with **no modifiers event in between**: the client sees Ctrl+F5 | high (0.9) |
| 2a | `ctrl-up-f5` (virtual Ctrl release, never pressed) while Ctrl held | Same as 1: the release is dropped by the kernel | high (0.9) |
| 2b | `ctrl-down-up-f5` (virtual Ctrl press then release) while Ctrl held | Same as 1: no extra `key: 37` lines, no modifiers event; F5 with Control | high (0.85) |
| 3 | `hl.dsp.send_shortcut({ mods = "", key = "F5" })` while Ctrl held | `depressed: 00000000` and `locked: 00000000`, `key: 71` pressed/released, `depressed: 00000000` again: **exact F5**. A following physical `c` (Ctrl still held) arrives as `key: 54` with no Control; releasing Ctrl brings back `locked: 00000010: Mod2` | high (0.85) for exact F5; medium-high for the desync |
| 4 | `f5` while physical Fn is held (not Ctrl) | No modifiers event, F5 exact; maybe a `key: 472` (`XF86Fn`) line before it | high for exact; low-medium for the 472 line |

## HIL run (human only)

Agents never run these: they create an input device and send input. Every step is reversible; the
whole run takes about ten minutes. Use two terminals, A and B, in the repo root.

**Setup**

```bash
sudo pacman -S --needed wev        # observer; remove at the end with: sudo pacman -Rns wev
# The emitter is .scratch/TB-11/uinput-emit.py (the same text is in the appendix below).
```

Terminal A: `wev -f wl_keyboard` (a wev window opens; its log prints in terminal A). Keep wev
small so terminal B stays visible.

**0 · Control run, and check the device is seen**

Terminal B: `sudo python3 -I .scratch/TB-11/uinput-emit.py f5`, then click the wev window and hold
nothing. While it counts down, in a third shell (optional, read-only):
`hyprctl devices | grep -A1 tb11` (expect `tb11-emitter` under Keyboards). Copy terminal A's lines.

**1 · F5 while physical Ctrl is held**

Terminal B: `sudo python3 -I .scratch/TB-11/uinput-emit.py f5`; click wev; press and **hold Left
Ctrl** until `sent` prints in B; release. Copy A's lines.

**2a / 2b · Virtual Ctrl release first**

Repeat step 1 with `ctrl-up-f5`, then with `ctrl-down-up-f5`. Copy A's lines for each.

**3 · send_shortcut**

Terminal B: `sleep 5; hyprctl dispatch 'hl.dsp.send_shortcut({ mods = "", key = "F5" })'`;
click wev; hold Left Ctrl; after B prints `ok`, keep holding Ctrl and press `c` once; release Ctrl.
Copy A's lines and B's output.

**4 · Fn layer check**

Repeat step 1 holding **Fn** instead of Ctrl. Copy A's lines.

**Stuck-modifier recovery (any time)**

1. The emitter always ends within about 6 s and destroys its device; Ctrl+C in terminal B does the
   same. If the process dies, the kernel destroys the device and releases its keys (`K`
   `misc/uinput.c:754-758` → `input.c:706-719`).
2. Check nothing is left: `grep -c tb11-emitter /proc/bus/input/devices` → `0`, and
   `hyprctl devices | grep -c tb11` → `0`. If not: `sudo pkill -f uinput-emit.py`.
3. If an app still acts as if Ctrl (or no Caps Lock/NumLock) were in effect: press and release
   Ctrl, Shift, Option and Command once each on the Fn keyboard. Hyprland resends the merged
   modifiers on every modifier change (`H` `InputManager.cpp:1685-1722`).
4. Last resort: log out of the session and back in.

**Teardown:** `sudo pacman -Rns wev` if it was installed for this run. Nothing else is left: no
udev rule, group change or file outside the repo.

**What to post** (one `[RESEARCH]` comment per step): the step number, the wev lines from the
moment the chord was pressed until it was released, and whether that matched the prediction table.

## Decision input: options for TB-18

| Option | Exact on Hyprland? | Cost / risk | Fits ADR-0009? |
|---|---|---|---|
| A · Modifier layers use `command` actions; `key` actions only on `default` and `fn` | Yes, by construction (§1, §4) | None new; a `key` on a `fn-*` layer would leak, so config check should warn | yes |
| B · Release the modifier on the virtual device before the key | No (§2: dropped by the kernel or ORed back by Hyprland) | Wasted code | yes |
| C · Send the key with Hyprland `send_shortcut` (IPC) | Yes for the focused client, with any explicit mods (§3) | Hyprland-only; bypasses Hyprland binds and the IME; client mods read 0 until the next physical modifier change; daemon needs `HYPRLAND_INSTANCE_SIGNATURE`. Available today with no code as a `command`: `hyprctl dispatch 'hl.dsp.send_shortcut({ mods = "", key = "F5" })'` | yes (no device grab) |
| D · Grab the Fn keyboard (`EVIOCGRAB`) and re-emit it through uinput, masking modifiers around bar keys | Yes, including Hyprland binds (only one keyboard holds Ctrl) | Reverses ADR-0009 (passive reader, no ioctl/`unsafe`, new crate); every keystroke goes through the daemon; a hung daemon keeps the grab and the Fn keyboard goes dead (the grab ends only when the fd closes, `K` `evdev.c:437`, `input.c:114-128`) | no |
| E · Hold the emitted key until the chord is released | Yes (merged mods are empty by then) | Fires late, after the bar has already switched back to the default layer; surprising | yes |
| F · Wayland virtual-keyboard protocol (wtype style) instead of uinput | No (same merge, §1 `share_states` row) | Needs a Wayland connection in the daemon | yes |

**Provisional recommendation (to confirm with the run):** A for the MVP, with C as the documented
recipe for an exact key on a modifier layer, written as a `command` action. TB-18 then emits
uinput `key` actions on `default` and `fn` only (they must stay real key events: the live config
binds plain `F9` to push-to-talk dictation and plain brightness/volume keys, which `send_shortcut`
would bypass). A native `send_shortcut` adapter can come post-MVP if a user needs `key` actions on
modifier layers. Reject B, D, E and F. If step 3 does not deliver an exact F5, the recipe in C is
dropped and A stands alone; if step 1 shows plain F5 (prediction wrong), the scope row can be lifted
for Hyprland.

## Follow-ups

- **Config check warns on a `key` action in a `fn-*` layer** (the modifier leak), naming the
  `send_shortcut` recipe. Blocked by TB-11. Relates to TB-18.
- **Native `send_shortcut` adapter for `key` actions on modifier layers** (post-MVP, only if the
  run confirms step 3). Blocked by TB-11 and TB-18.

## Appendix: `.scratch/TB-11/uinput-emit.py`

```python
#!/usr/bin/env python3
"""TB-11 throwaway uinput emitter (human-run only; agents never run this).

Usage: sudo python3 -I uinput-emit.py f5|ctrl-up-f5|ctrl-down-up-f5
"""
import fcntl, os, struct, sys, time

# linux/uinput.h: _IOW('U', 100, int), _IOW('U', 101, int), _IOW('U', 3, struct uinput_setup /* 92 B */)
UI_SET_EVBIT, UI_SET_KEYBIT, UI_DEV_SETUP = 0x40045564, 0x40045565, 0x405C5503
UI_DEV_CREATE, UI_DEV_DESTROY = 0x5501, 0x5502  # _IO('U', 1), _IO('U', 2)
EV_SYN, EV_KEY, SYN_REPORT = 0x00, 0x01, 0  # linux/input-event-codes.h
KEY_LEFTCTRL, KEY_F5 = 29, 63
BUS_VIRTUAL = 0x06

MODES = {
    "f5": [(KEY_F5, 1), (KEY_F5, 0)],
    "ctrl-up-f5": [(KEY_LEFTCTRL, 0), (KEY_F5, 1), (KEY_F5, 0)],
    "ctrl-down-up-f5": [(KEY_LEFTCTRL, 1), (KEY_LEFTCTRL, 0), (KEY_F5, 1), (KEY_F5, 0)],
}

def send(fd, code, value):
    # struct input_event on x86_64: timeval (2 x 8 B, filled by the kernel), u16 type, u16 code, s32 value
    os.write(fd, struct.pack("<qqHHi", 0, 0, EV_KEY, code, value))
    os.write(fd, struct.pack("<qqHHi", 0, 0, EV_SYN, SYN_REPORT, 0))

def main():
    if len(sys.argv) != 2 or sys.argv[1] not in MODES:
        sys.exit("usage: sudo python3 -I uinput-emit.py " + "|".join(MODES))
    steps = MODES[sys.argv[1]]
    fd = os.open("/dev/uinput", os.O_WRONLY)
    try:
        fcntl.ioctl(fd, UI_SET_EVBIT, EV_KEY)
        for code in (KEY_LEFTCTRL, KEY_F5):
            fcntl.ioctl(fd, UI_SET_KEYBIT, code)
        setup = struct.pack("<HHHH80sI", BUS_VIRTUAL, 0x0001, 0x0b11, 1, b"tb11-emitter", 0)
        fcntl.ioctl(fd, UI_DEV_SETUP, setup)
        fcntl.ioctl(fd, UI_DEV_CREATE)
        for n in range(5, 0, -1):
            print(f"{sys.argv[1]}: sending in {n} s - focus wev and hold the chord", flush=True)
            time.sleep(1)
        for code, value in steps:
            send(fd, code, value)
            time.sleep(0.05)
        print(f"{sys.argv[1]}: sent {steps}", flush=True)
        time.sleep(0.5)
    finally:
        try:
            fcntl.ioctl(fd, UI_DEV_DESTROY)
        finally:
            os.close(fd)

main()
```

Constants: `/usr/include/linux/uinput.h` (`UI_DEV_SETUP` = `_IOW('U', 3, struct uinput_setup)`,
`sizeof` = 8 + 80 + 4 = 92) and `/usr/include/linux/input-event-codes.h` (`KEY_LEFTCTRL` 29,
`KEY_F5` 63).

## Sources

- Live: `hyprctl version`, `hyprctl devices`, `hyprctl binds -j`,
  `hyprctl getoption ecosystem:enforce_permissions`, `ls ~/.config/hypr` (Lua config),
  `pgrep -a fcitx5`, `xkbcli compile-keymap --layout us --options omarchy:shift_caps_compose`,
  `pacman -Q hyprland libinput libxkbcommon`, `pacman -Si wev python-evdev` (neither installed).
- [hyprwm/Hyprland `v0.56.2`](https://github.com/hyprwm/Hyprland/tree/v0.56.2/src):
  `managers/input/InputManager.cpp`, `managers/SeatManager.cpp`, `managers/KeybindManager.cpp`,
  `protocols/core/Seat.cpp`, `devices/Keyboard.cpp`, `devices/IKeyboard.{hpp,cpp}`,
  `devices/VirtualKeyboard.cpp`, `config/shared/actions/ConfigActions.cpp`,
  `config/lua/bindings/LuaBindingsDispatchers.cpp`, `config/legacy/DispatcherTranslator.cpp`,
  `debug/HyprCtl.cpp`.
- [Linux `v6.18` `drivers/input/`](https://github.com/torvalds/linux/tree/v6.18/drivers/input):
  `input.c`, `evdev.c`, `misc/uinput.c`.
- [libinput `1.32.0`](https://gitlab.freedesktop.org/libinput/libinput/-/tree/1.32.0/src):
  `evdev.c`, `evdev-fallback.c`, `libinput.c`.
- [systemd `v258` `udev-builtin-input_id.c`](https://github.com/systemd/systemd/blob/v258/src/udev/udev-builtin-input_id.c).
- [wev `1.1.0`](https://git.sr.ht/~sircmpwn/wev/tree/1.1.0) `wev.c`, `README.md`.
- Research 0002 (Fn keyboard, uinput access), ADR-0009 (passive Fn keyboard reader).
