# Configurator B: layer → group → button

The portable reference for TB-40 is **touchbar-customizer-migrado.figcraft**, frame **576**
“B — Flujo por capa”, and notes frame **907** “Notas — Variante B · completadas”.
Coordinates below are logical points in its **1200 × 760** frame, with the document's x=1940
removed. The attachment is not required to reproduce these bounds or interactions. Variant C,
images and widgets are outside this implementation.

## Geometry

| Source node / region | Bounds (x, y, width, height) | Implementation resolution |
|---|---|---|
| 576: B frame | 0, 0, 1200, 760 | Native logical canvas, independently scrollable columns |
| 581: sidebar divider | 208, 0, 1, 643 | Sidebar is 208 points wide |
| 759: edit-panel divider | 880, 0, 1, 643 | Workspace 672 points, edit panel 320 points |
| 652: workspace content | 232, 0, 624, 643 | 24-point horizontal inset; authored 628 is not the whole column |
| 606: selected layer example | 20, 175, 168, 34 | Six actual canonical layer rows start y 107, repeat every 34 points |
| 758: edit-panel content | 901, 23, 279, variable | Authored 308 leaves 12 points; resolved outer region is 320 |
| Label input | 901, 102, 279, 32 | Real text input, same position for every selected button |
| First group / first cards | heading y 102, cards y 122 | 20 points from heading to card; cards 53 high |
| Subsequent group headings | y 189 /276 /363 in single-row groups | 14 points after a 53-point card, then 20-point heading |
| Two-card group | x 232 /547, widths 309 | 6-point gap; width describes list cards, not device buttons |
| Three-card group | x 232 /442 /652, widths 204 | Long groups wrap at three cards with 8-point row gaps |
| Add group example | 232,450,624,38 | Real content pushes this row down naturally |
| 847: footer | 0,643,1200,117 | Stays below independently scrolling body columns |
| 856: preview canvas | 57,682,1085,30 | Exactly 2170 × 60 at 50%; outer frame 56,681,1087,32 |
| Preview action description | 24,726,1152,18 | Selected action text; never executes an action |

The source's abbreviated width/action/colour chips become usable text inputs. Extra numeric,
policy and diagnostic rows keep their section order and scroll in the edit panel. Sidebar save
and diagnostics scroll independently. Long labels truncate in cards and in the shared bar view;
fields retain their complete text.

At 1200 × 760 the sidebar/workspace/edit panel occupy 208/672/320 points above a 117-point footer.
Narrow windows scroll the native 1200-point-wide document horizontally using Rune's public
scroll APIs. Short windows bound each body region to the available height above the 117-point
footer, with a 200-point minimum body viewport; the outer document scrolls vertically below that
minimum. Each body region owns its content overflow, so nested wheel handling cannot trap a
control below a short window. No control or bar is scaled down. Required functional sizes are
1200 × 760, 1200 × 500, 900 × 600 and 640 × 400. The native 50%
preview remains 1085 × 30 and is reachable in document order.

## Theme and distinct states

| Source colour / meaning | Theme role |
|---|---|
| #1a1b26 background | `BarTheme.background` |
| Flat surfaces / footer | `button_fill` over background |
| #a9b1d6 primary ink | `button_text` |
| #3b4261 separators | `button_border` |
| #7aa2f7 selection | `accent`, outer 2-point ring |
| #f7768e validation | resolved `red`, inner 1-point ring and textual `!` |
| Optional red absent | `button_text` with the same textual issue marker |

The configurator uses system monospace, square panels, flat fills and no shadows/gradients.
Selection and validation rings have separate geometry, keys and colours, so both remain visible.
They decorate only cards and the configurator preview: they never modify `SceneButton`, physical
pressed state, daemon rendering, or shared `bar_button`. The active Omarchy theme is loaded at
startup. Token colour overrides follow the theme; valid hex overrides show their frozen-theme
notice. There is no added live theme watcher.

The preview calls `build_scene` and `bar_button` at 0.5, preserving 88 px device edges, 12 px in-group
gaps, 88 px standard group gaps, per-layer gap overrides and 8 px vertical inset. Labels use shared
`label_width` at the theme font size; room is the actual placement width minus 24 device pixels.
Unavailable placements produce an explicit message rather than invented widths. The source's
263 px example is never a live width constant.

## Workflow and ambiguity resolutions

1. Always list `default`, `fn`, `fn-ctrl`, `fn-alt`, `fn-shift`, `fn-super`, with actual counts
   and `valid`/`error`/`empty` text. Visiting an absent optional layer does not create it. Its first
   add creates that layer only. Physical Fn/modifier events never change the editing selection.
2. Header counts are actual buttons/groups. Its width summary reports the actual minimum–maximum
   placement widths (including integer remainder), or empty/unavailable. Source counters, commands,
   timestamps and selected chord are example data. No `NUEVO` annotation is runtime UI.
3. Group `+ button` appends within its own current group, including a loaded empty group. Toolbar
   add preserves existing after-selection semantics. Add group inserts after the selected group,
   otherwise appends; it holds one new `none` button. First-free `button-N` ids remain stable.
   Remove clears selection and removes the emptied source group. Add/remove on a previously absent
   optional layer restores absence; originally present empty layers and explicit gaps survive.
4. Card drag and preview drag reorder within/across groups; Alt+Left/Right uses shared step rules.
   Selection follows the button id and selecting a card blurs fields. Cancel/self-drop does nothing.
   Groups have no invented persistent ids: displayed group numbers follow current indices.
5. Width supports `auto`, `span N`, `px N`, plus explicit mode/numeric controls. Suggested span 1–4
   and 64 px minimum are guidance, never silent clamps. Invalid width drafts remain per(chord,id),
   count as pending and block saving; another selection cannot hide them or save an older width.
6. Key and command buffers are independent per(chord,id). First switching to command starts empty,
   never with former key text. Switching through `none` remembers alternate buffers transiently;
   inactive buffers are neither serialized config nor dirty changes.
7. Ordinary modifier-layer buttons cannot newly select key. Existing legacy keyed buttons remain
   readable/saveable with shared nonblocking `KeyOnModifierLayer` warnings, and can change to
   command/none. The existing preset ESC (`esc`, `Esc`,116 px) is an explicit compatibility exception,
   also warned: it does not promise modifier isolation or pin/remove/reorder protection. The
   configurator does not add ESC to an empty layer. See ADR-0011 and research 0008.
8. Empty/whitespace-only commands are rejecting shared `ConfigIssue`s on every load/save path.
   `none` is the supported no-op. Unknown keys/colours, empty labels and layout issues retain shared
   validation; clipping/capacity/modifier warnings never block save. Global issues include every
   layer and draft, with a target chord/button and selected-button explanation.
9. Actual Save and focused Ctrl+S share one guard. Save has disabled accessibility semantics and
   no press handler while any rejecting issue or unavailable file blocks it. ConfigFile retains
   atomic temp/sync/rename writes; failed saves retain edits, baseline and last successful time.
10. Pending means a comparison against the last accepted/saved config, including layer presence,
    order, gaps, action/style/label/width and global theme/version. Reverting clears it. Startup
    rejected/unreadable files have no accepted baseline and show unsaved replacement state.
    Successful explicit saves alone sample an injected epoch clock and display HH:MM:SS UTC;
    no file mtime or fabricated startup timestamp is used.
11. File status says the daemon watches the saved config file and **simulator acknowledgement
    unavailable**. It makes no liveness, revision-observed or synchronization claim. No IPC is
    added; no button action executes in the configurator.

## Verification boundary

Runtime contracts observe literal geometry, taps, drag, keyboard, real scrolling, actual config
and file bytes. Shared core/file validation covers blank-command compatibility. Screenshots and
pixel inspection remain required after kernel-build completion. This reference alone does not
establish build success, functional correctness, simulator acknowledgement or physical hardware
support; the ticket's retained QA artifacts record those outcomes separately.
