# 0012 · Rune: the pointer leaving the window while a press is held

Status: TB-51 · research lane · headless only (no window opened, no Hyprland run, no hardware) ·
2026-11-24. Answers the block of TB-44 fix cycle 1 (simulator window, app at `905dad2`).

## Question

When the pointer leaves the simulator window while a press on `sim-screen` is held, and the host
reports only `CursorLeft` (no outside coordinates), how can TB-44 cancel the touch, clear the
highlight at the leave, and keep the release (in place, or after coming back) from running the
button, with the smallest reversible change, and what can it consume today?

What answers it: the host path and public API at the pin and at an identified upstream revision; a
supported mechanism if one exists, else an opt-in change tried in a disposable copy; the four
TB-44 regressions run on each candidate, with a control that does not opt in.

Sources, by SHA (line numbers are at that SHA):

- `P` = Rune pin `f73f72cdc563534e4f7de9f5a79bed007ac42384` (PR #185, committed 2026-11-07), the
  rev in TB-44's `Cargo.toml`/`Cargo.lock`. Paths under `crates/rune/src/`.
- `H` = Rune `dev` at `22ebd7e7f5ceb189e1829f066b5ba0fe31ed5598` (PR #2021, merged
  2026-11-24T20:43Z): 1523 commits ahead of `P`, 0 behind
  (`gh api repos/anomalyco/rune/compare/f73f72c…22ebd7e`). `dev` is the default branch; it had
  moved on to `c931651436` (PR #2041, which touches `pointer.rs` and `hover.rs` again) by
  2026-11-24T23:08Z, so `H` is the revision tested here, not "latest".
- `A` = TB-44 `905dad2`, `crates/tb-daemon/src/sim_window.rs`; `T` = the four regressions
  `window_leave_in_capture.rs` from TB-44 fix cycle 1 (sha256 `479e9888…726b`, used unchanged).
- `Y` = Hyprland `v0.56.2` (`efb50993…`, the build `hyprctl version` reports on this machine),
  `src/managers/input/InputManager.cpp`.

## Findings

### 1. At the pin no public mechanism sees the leave (confirms TB-44's block)

| Claim | Source | Confidence |
|---|---|---|
| The desktop host turns `CursorLeft` into `rt.pointer_leave()` + a redraw, nothing else | `P` `window/events.rs:479-482` | high |
| The host releases the left button at `rt.pointer()`, and only if it is `Some` | `P` `window/events.rs:487-511` (494, 509) | high |
| `pointer_leave` with a capture only sets the private `left_in_capture`; no `Drag`, no hover change, `pointer` kept | `P` `runtime/input/pointer.rs:338-348` | high |
| A release after it queues `Drag` `Phase::Up` uncancelled at the last inside position, then lands the deferred leave | `P` `pointer.rs:287-293`, `295-336` (320), `160-166` | high |
| A move back inside clears `left_in_capture` and the press goes on as if it never left | `P` `pointer.rs:17` | high |
| `on_hover_change` holds a pressed element inside until release ("A held press keeps the pointer inside until it releases") | `P` `el/layout.rs:353-358`; `runtime/input/hover.rs:100-109` | high |
| `Drag::cancelled` is set only by `cancel_drag` (Escape, focus loss, `cancel_pointer`, which is `pub(crate)`) | `P` `el.rs:80,90`; `pointer.rs:94-127`; `runtime/input/keys.rs:270`; `window/lifecycle.rs:256,297` | high |
| `App` exposes no window-event hook: `on_window`/`on_ready` hand a `WindowHandle` (raw handle, placement), `on_frame_drawn` a read-only `&Runtime` | `P` `app.rs:396-438`; `window/handle.rs:21-116` | high |
| Rune pins leave-during-press as *not* a cancel ("the deferred leave lands on release") | `P` `runtime/tests/pointer_drag.rs:360-366` | high |

So, at `P`, TB-44 cannot meet the four regressions without changing Rune.

### 2. Upstream `dev` already has a supported, opt-in path (no Rune change)

| Claim | Source | Confidence |
|---|---|---|
| The host path is unchanged: `CursorLeft` → `pointer_leave()`; release at `rt.pointer()`, else a sampled OS pointer | `H` `window/host/events.rs:555-558`, `663-675` | high |
| `pointer_leave` with a capture now marks the pointer left and refreshes hover (capture and position kept) | `H` `runtime/input/pointer.rs:545-556` | high |
| New opt-in `El::hover_tracks_bounds()`: a held press that leaves the element's bounds **or receives a host pointer-leave** fires `on_hover_change(false)`; coming back fires `true`; capture is not ended | `H` `el/layout.rs:849-882`; `runtime/input/hover.rs:164-173`; PR #2021 (fixes #329) | high |
| Without the opt-in, the old retention stays (default `false`) | `H` `runtime/input/hover.rs:164-173`; PR #2021 | high |
| PR #2021 was checked headless and on wasm only ("No live desktop window was opened") | PR #2021 body, "Not checked" | high |
| The release after a leave still queues an *uncancelled* `Drag` `Phase::Up`, so the app must ignore it itself | `H` `pointer.rs:472-523` | high |
| `Runtime::cancel_pointer` and `Tx::cancel_pointer` are public since PR #1889 (`ab4da9c8be`) | `H` `runtime/input/api.rs:63`; `el/tx/press.rs:92` | high |

TB-44 at `H` needs only an app change: `sim-screen` opts into `hover_tracks_bounds` and, on
`on_hover_change(false)` while a press is down, sends `Touch::Cancel` and forgets the press (the
existing `(Up, None) => {}` arm then ignores the release). Diff (`A` → variant, verbatim):

```diff
@@ impl SimWindow {
+    /// The pointer left the screen (or the window) while a press is held: cancel it now; the rest
+    /// of that press, the release included, sends nothing.
+    fn left_screen(&mut self) {
+        if self.pressed_x.take().is_some() {
+            self.send(Touch::Cancel);
+        }
+    }
@@ pub fn sim_window_view(…)
             .pixels(&w.screen)
+            .hover_tracks_bounds()
+            .on_hover_change(|s: &mut SimWindow, inside: bool, _tx: &mut Tx| if !inside { s.left_screen() })
             .on_drag(|s: &mut SimWindow, d: &mut Drag, tx: &mut Tx| s.on_drag(d, tx)),
```

`Tx::cancel_pointer` in that callback would also end Rune's capture, but it clears
`rt.pointer()` (`cancel_pointer` → `pointer_leave` without capture, `H` `pointer.rs:130-142`,
`545-549`), so a regression that releases at `rt.pointer().expect(…)` (`T:186`) would panic: not
used, not run.

### 3. The pin with an opt-in Rune change (prototype in a disposable copy)

When no supported path exists on the rev TB-44 builds, the smallest opt-in change reuses Rune's
own cancel (`cancel_drag`). It is 22 lines in Rune plus one Rune unit test, and one line in TB-44
(`.cancel_drag_on_leave()` on `sim-screen`; `SimWindow::on_drag` already maps `drag.cancelled()`
to `Touch::Cancel`, `A:70-72`). Diff against `P` (product code only):

```diff
--- a/crates/rune/src/el/handler_set.rs
@@ pub(crate) struct Handlers {
     pub drag_threshold: Option<f32>,
+    /// `El::cancel_drag_on_leave`: the pointer leaving the window cancels this element's held drag.
+    pub cancel_drag_on_leave: bool,
@@ impl Empty for Handlers {
         drag_threshold: None,
+        cancel_drag_on_leave: false,
--- a/crates/rune/src/el/handlers.rs
@@ impl El {
+    /// The pointer leaving the window while this element's [`El::on_drag`] holds a press ends the
+    /// gesture there, as a cancel: the handler hears `Phase::Up` with [`Drag::cancelled`] at the
+    /// leave, and the release that follows does nothing, also when the pointer came back first.
+    /// Without it a leave mid-press keeps the press, as AppKit does, and the release ends it.
+    pub fn cancel_drag_on_leave(mut self) -> Self {
+        self.handlers.cancel_drag_on_leave = true;
+        self
+    }
--- a/crates/rune/src/runtime/commit/memo_reuse.rs
@@ fn same_handlers(   (the exhaustive `Handlers` pattern; E0027 without it)
         drag_threshold,
+        cancel_drag_on_leave,
@@
         && a.drag_threshold == *drag_threshold
+        && a.cancel_drag_on_leave == *cancel_drag_on_leave
--- a/crates/rune/src/runtime/input/pointer.rs
@@ pub fn pointer_leave(&mut self) {
         self.wheel_lost();
+        // An element that asked for it (`El::cancel_drag_on_leave`) loses its held press here; the
+        // leave itself still lands on the release, at the last position the host saw.
+        if self.capture_button == crate::PointerButton::Primary
+            && self.capture.as_ref().and_then(|(p, _)| self.live_index(p)).is_some_and(|i| self.nodes[i].handlers.cancel_drag_on_leave)
+            && self.cancel_drag()
+        {
+            self.left_in_capture = true;
+            return;
+        }
```

Keeping `left_in_capture` (instead of clearing the pointer at once) leaves `rt.pointer()` at the
last inside position, so the host's release still arrives (`P` `window/events.rs:494`), finds no
capture and does nothing, and the deferred leave lands then (`P` `pointer.rs:160-166`, `287-293`).

### 4. Results: the four TB-44 sequences, each candidate, with controls

All runs headless (`rune::Runtime<SimWindow>` + `forward_touches` + `Daemon`, as `T` drives them),
on disposable copies of `A`, run `.scratch/TB-51/proto/runs/20261124T210554Z` (§ Reproduce).
Columns: **S1** highlight cleared at the leave (`T:202`) · **S2** leave then release runs nothing
(`T:221`) · **S3** leave, back over the same button, release runs nothing (`T:234`) · **S4** the
next click runs exactly once (`T:250`); each for F5 (key records) and a command button (marker).
"Kept" = the other `tb-daemon` window/touch suites: `window_press_release` 13, `cancelled_touch`
16, `sim_window_touches` 14, `sim_window_screen` 14.

| Run | Rune | `sim-screen` | S1 | S2 | S3 | S4 | Kept |
|---|---|---|---|---|---|---|---|
| TB-44 red-1 (reference) | `P` | unchanged | FAIL | FAIL | FAIL | FAIL | — |
| A1 control | `P` + opt-in patch | unchanged (no opt-in) | FAIL | FAIL | FAIL | FAIL | 57/57 |
| A2 | `P` + opt-in patch | `.cancel_drag_on_leave()` | pass | pass | pass | pass | 57/57 |
| B1 control | `H` | unchanged | FAIL | FAIL | FAIL | FAIL | 57/57 |
| B2 | `H` | `hover_tracks_bounds` + `on_hover_change` (§2) | pass | pass | pass | pass | 57/57 |

- A1 and B1 fail with the same assertions as TB-44's red-1 (`[[], [f5]]` vs `[[], [f5], []]`;
  `F5 EV_KEY 63 1/0` written; F5 twice in S4): a consumer that does not opt in keeps today's gesture
  meaning, at `P`+patch and at `H`.
- Rune-level checks through the public API (`rune_cancel_drag_on_leave.rs`, A1 and A2, 3/3): with
  the option, `Up` cancelled at the leave, `pressing()` false, the release (also after coming back)
  adds nothing, the next press is a fresh uncancelled gesture; without it, no `Up` at the leave,
  `pressing()` true, the release ends it uncancelled and `pointer()` is `None` after it (Rune's own
  `pointer_drag.rs:360-366` behaviour).
- C1, Rune's own unit tests (`runtime::tests::pointer_drag`, `pointer_hit`, `hover_tests`) on the
  patched pin: not run. Cargo refuses (`package rune cannot be tested because it requires
  dev-dependencies and is not a member of the workspace`), and Rune's own workspace does not
  resolve offline (`dioxus-devtools` missing from the cache). The Rune unit test the patch adds
  (`cancel_drag_on_leave_ends_a_held_drag_cancelled_at_the_leave`) is therefore unverified; its
  behaviour is covered by the public-API checks above.
- `H` compiles TB-44 unchanged: `cargo check --workspace --all-targets` exit 0 (B3), no warning
  in a `tb-*` crate. Not run at `H`: the rest of the test suite, clippy, `scripts/check.sh`,
  render snapshots (unverified).
- Limits: one run each, one CPU, thermal pauses 0-65 per step (logged with ns timestamps); the
  sequences use `T`'s waits (presents ≤5 s, quiet 1 s). Headless only: what the real host sends is
  § 5.

### 5. What Hyprland actually sends mid-press (HIL, not run)

| Claim | Source | Confidence |
|---|---|---|
| While a pointer button is held (no drag-and-drop, no hard input), Hyprland keeps pointer focus on the surface it was on and sends motion in that surface's coordinates, so outside coordinates arrive as `CursorMoved`, which TB-44 already cancels (`!inside`, `A:63,70`) | `Y` `InputManager.cpp:410-445`, `765-785` | medium (source read, no live run) |
| A `wl_pointer.leave` (winit `CursorLeft`) with no outside motion first is therefore not the normal Hyprland path; it may still come from a surface unmap, a DnD, `m_hardInput`, or a workspace/focus change during the press | `Y` `InputManager.cpp:412-413` (the conditions) | low (inferred, unverified) |
| The four regressions model the `CursorLeft`-only path; whether it occurs on this machine is HIL | TB-44 `blocked.md`; this doc | — |

## Decision input for TB-44

| | Option A: bump Rune to `H` + app-side opt-in (§2) | Option B: keep `P`, opt-in Rune patch (§3) | Option C: keep `P`, no change |
|---|---|---|---|
| Exists today | yes, public API on `dev` `22ebd7e` | prototype only (local copy) | — |
| Consumable from a clean checkout today | yes, once `Cargo.toml` pins `22ebd7e` and repeats Rune's `[patch.crates-io]` (below) | no: needs a published rev (upstream PR or a fork) | — |
| External authorization | none (reads a public rev; publishes nothing) | yes: publishing a fork or an upstream PR is the human's call | none |
| TB-44 code change | ~10 lines in `sim_window.rs` | 1 line in `sim_window.rs` | none |
| Meets the four sequences headless | yes (B2) | yes (A2) | no |
| Other apps' behaviour | unchanged (opt-in, default `false`) | unchanged (opt-in, default `false`) | — |
| Cost | the Rune bump: `rev` → `22ebd7e`; 8 `[patch.crates-io]` entries Rune asks apps to repeat (`H` root `Cargo.toml:110-173`: `wgpu-hal`, `naga`, `winit`, `sctk-adwaita`, `parley_engine`, `accesskit_windows`, `accesskit_atspi_common`, `accesskit_macos`, of which the three `accesskit_*` were unused on Linux; without them resolution fails on `parley_engine` feature `line-dictionaries`); `Cargo.lock` +289/−34; full suite, clippy and snapshots at `H` still to run | a Rune fork or upstream PR, its rev in `Cargo.lock`, and keeping the patch alive across bumps | — |
| Rollback | revert the TB-44 commit and the bump commit (`rev` back to `f73f72c`, drop `[patch.crates-io]`, restore `Cargo.lock`) | revert the rev and the one line | — |

**Decision for TB-44: Option A, split in two.** TB-44 cannot meet the leave regressions on the
current pin without a Rune change; upstream `dev` already solves it with a supported opt-in, and the
whole TB-44 workspace compiles against it unchanged. Per `Cargo.toml` ("Bump in its own ticket")
and ADR-0001, the bump is its own ticket; TB-44 then adds the §2 change on top. Until the bump
lands, TB-44 is blocked on it (not on research). Option B is the fallback only if the bump fails
its own gates; it needs the human's authorization to publish a fork or upstream PR.

In the bump ticket, as Rune asks of apps, the `[patch.crates-io]` entries must point to the same
rev. Untested here: the git form (`naga = { git = "https://github.com/anomalyco/rune", rev =
"22ebd7e…" }`, Cargo finding the package in `vendor/`). The prototype used local paths.

## Follow-ups (proposed tickets)

1. **Rune bump to `dev` `22ebd7e`** (own ticket, blocks the TB-44 fix): `rev` +
   `[patch.crates-io]` (git form) + `Cargo.lock`; run `scripts/check.sh` (full suite, clippy,
   snapshots); keep the four `T` regressions red until 2.
2. **TB-44 fix cycle**: apply the §2 change on top of 1; `T` turns green. Blocked by 1.
3. **HIL on Hyprland** (hardware/human): press on the simulator window, drag out of it, release
   outside / come back; confirm what winit gets (`CursorMoved` outside vs `CursorLeft` only) and
   that no key is emitted. Blocked by 2.

## Reproduce

Everything is in `.scratch/TB-51/proto/` (`README.md`, `patches/`, `runs/20261124T210554Z/`
with `run.sh`, logs, return codes, `receipt.txt`, `thermal-events.log`). Without that directory:

1. Clone Rune; `git checkout f73f72cdc563534e4f7de9f5a79bed007ac42384`; apply the §3 diff (and the
   Rune unit test in `runtime/tests/pointer_drag.rs`, in `patches/rune-cancel-drag-on-leave.patch`).
2. `git archive 905dad2` of this repo into a fresh dir; append
   `[patch."https://github.com/anomalyco/rune"] rune = { path = "<rune>/crates/rune" }` to its
   `Cargo.toml`; copy `T` (TB-44 `.scratch/TB-44/fix-1/window_leave_in_capture.rs`) into
   `crates/tb-daemon/tests/`.
3. `cargo test -p tb-daemon --test window_leave_in_capture --test window_press_release --test
   cancelled_touch --test sim_window_touches --test sim_window_screen` → 4 FAIL (control); add
   `.cancel_drag_on_leave()` before `.on_drag(` in `sim_window_view` → all pass.
4. For `H`: a second archive of `905dad2`; `[patch."https://github.com/anomalyco/rune"]` to a Rune
   checkout at `22ebd7e`, plus `[patch.crates-io]` with the 8 crates above as paths into its
   `vendor/`; `cargo fetch`; same tests (control 4 FAIL); apply the §2 diff → all pass;
   `cargo check --workspace --all-targets`.

Each heavy step ran through the shared thermal lock (`thermal-command.py` of TB-46
`validate-thermal/20261124T173620Z`: start ≤65 °C, pause ≥74 °C, resume ≤64 °C, one CPU, nice 19,
one job, one test thread, no duty cycle), with a private `HOME`, XDG dirs, `CARGO_HOME` (reflink
copy) and target, offline after one `cargo fetch` for `H`.

## Sources

- Rune pin: <https://github.com/anomalyco/rune/tree/f73f72cdc563534e4f7de9f5a79bed007ac42384>
- Rune `dev` tested: <https://github.com/anomalyco/rune/tree/22ebd7e7f5ceb189e1829f066b5ba0fe31ed5598>
- PR #2021 `feat(pointer): add Cx::press_cursor, Tx::press_point, and El::hover_tracks_bounds`
  (fixes #329 "Opt-in pointer-inside tracking during a held press (element bounds and window
  leave)"): <https://github.com/anomalyco/rune/pull/2021>
- PR #1889 `feat(input): expose … Runtime::cancel_pointer, and Tx::cancel_pointer`:
  <https://github.com/anomalyco/rune/pull/1889>
- Hyprland `v0.56.2`: <https://github.com/hyprwm/Hyprland/blob/efb50993780079460b0cbed1363e2166a2de1d9f/src/managers/input/InputManager.cpp>
- TB-44 block and red run: TB-44 `.scratch/TB-44/fix-1/blocked.md`, `red-1.log` (app `905dad2`).
