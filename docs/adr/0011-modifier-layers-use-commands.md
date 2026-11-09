---
status: accepted (2026-11-09, TB-11; research 0008)
---

# Modifier layers use command actions; emitted keys stay on the default and fn layers

On a `fn-*` layer the user is still holding the modifier when they touch the bar. Hyprland sends
every client one modifier state, the OR of all keyboards' modifiers, so a key the daemon emits from
its uinput keyboard arrives combined with the held modifier (Ctrl+F5 instead of F5), and Hyprland's
own binds match it too (on this config, Ctrl+Brightness Up is "keyboard brightness up"). Releasing
the modifier on the virtual device cannot help: the kernel drops a release the device never pressed,
and Hyprland ORs the physical modifier back in (research 0008 §1-2). So `touchbard` emits uinput
`key` actions only on the `default` and `fn` layers (Fn is not an xkb modifier); `fn-*` layers use
`command` actions, and the config check warns about a `key` action there. An exact key on a
modifier layer is written as a command:
`hyprctl dispatch 'hl.dsp.send_shortcut({ mods = "", key = "F5" })'`, which sets the client's
modifiers explicitly (research 0008 §3).

The live run on this machine confirmed it (research 0008 § HIL results). Step 1: F5 under a held Ctrl
arrived as Ctrl+F5. Steps 2a/2b: a virtual Ctrl release, or press and release, changed nothing.
Step 3: `send_shortcut` delivered an exact F5, but left the client reading no modifier and no NumLock
while Ctrl was still held, so a following Ctrl+C arrived as plain `c`. Step 4: under Fn, F5 arrived
plain.

## Considered options

- **Virtual release / re-press of the modifier.** Ineffective (above).
- **Native `send_shortcut` adapter for `key` actions on modifier layers.** Exact for the focused
  client, but Hyprland-only, bypasses Hyprland binds and the input method, and leaves the client
  believing no modifier or lock is active until the next physical modifier change. Possible
  post-MVP; the command recipe covers it now with no code.
- **Grab the Fn keyboard and re-emit it.** Exact everywhere, but reverses ADR-0009 (passive reader,
  no ioctl), routes every keystroke through the daemon, and a hung daemon holding the grab leaves the
  internal keyboard dead.
- **Hold the key until the chord is released.** Exact, but fires after the bar has switched back
  to the default layer.
- **Wayland virtual-keyboard protocol instead of uinput.** Same merge, same leak.

## Consequences

Default-layer F-keys stay real key events, so Hyprland binds on them keep working (plain `F9` is
push-to-talk dictation here), which `send_shortcut` would bypass.
