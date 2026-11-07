# Scope: POC and MVP

The question this project answers: **can a Rust + Rune app own the T1 Touch Bar on Omarchy,
with user-arranged, themed buttons per Fn + modifier layer?**

## Hardware limits that set the scope

| Limit | Consequence |
|---|---|
| 2170 x 60 px, one row, no hover | One line of buttons per layer. No tooltips, menus, popovers or scrolling. |
| Pixels only in USB config 2 with a patched kernel driver | Hardware enablement is a human, reversible-but-risky step (TB-9). Everything else must work without the bar (simulator). |
| T1 firmware draws key mode in config 1 | While the stack is off, the stock F-key bar keeps working. Never leave the user with a dead bar. |
| Fn is reported only by the SPI keyboard | Layers are chosen from that keyboard's state. |
| Held modifiers leak into emitted keys | Modifier layers default to `command` actions until TB-11 resolves it. |
| Touch ID / Secure Enclave | Out of scope, permanently. |
| OLED-like panel, always on | Static content; dim/idle timeout and optional pixel shift later. |
| Suspend/resume with config 2 unproven | MVP may stop the daemon on suspend; fix is post-MVP. |

## In scope (MVP, milestone M4)

1. Layers: `default` (F-keys, always present), `fn`, `fn-ctrl`, `fn-alt`, `fn-shift`, `fn-super`.
2. Buttons: id, label, width (`auto`, `span = n`, `px = n`), style override (fill, text, border as
   theme tokens or hex), action (`key`, `command`, `none`).
3. Layout engine with the spacing rules in `design/button-standard.md`; validation that reports
   every problem.
4. Omarchy theming: follow the active theme live; Omarchy-style flat, square buttons.
5. Daemon `touchbard`: render on change, present to the bar, touch → button → action, layer switch
   on Fn + modifier.
6. Configurator window: pick layer, add/edit/remove buttons, reorder (drag and keyboard), live
   preview at true proportions, save `~/.config/touchbar-customizer/config.toml`.
7. Simulator backend (PNG / window) so all of the above runs without the bar.

## Out of scope (for now)

Icons and images (labels only in MVP; icons are the first post-MVP feature), multi-modifier
chords (`fn+ctrl+shift`), per-app layers, sliders and other gesture controls, animations beyond
press feedback, live widgets (clock, battery), T2/Apple Silicon support (likely free later via
upstream `appletbdrm`), packaging for other distros.

## Viability exit criteria (POC, milestone M3)

- A Rune-rendered frame shows on the physical bar, correct orientation and colours.
- Tapping a button on the bar fires its action; holding Fn switches layers within 50 ms.
- Theme change in Omarchy re-renders the bar within 1 s.
- Removing the stack restores the stock key-mode bar.
