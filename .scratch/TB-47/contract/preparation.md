# TB-47 slice 1: QA spec preparation

Base: `c1f76d1` (TB-43 tip). Frozen: `crates/tb-daemon/tests/drm_display.rs` (31 cases),
`crates/tb-daemon/tests/drm_target.rs` (13 cases), plus 2 `sim_display` preserve cases. The public
interface the tests pin is the slice-1 draft `interface.md` (names and field shapes binding; the
`Kms` trait shape is free).

## Changes from the unsealed draft

- **Orientation spelling cited.** Linux 7d0a66e4 `drivers/gpu/drm/drm_connector.c` L1149–1154 maps
  `DRM_MODE_PANEL_ORIENTATION_RIGHT_UP` to "Right Side Up". The physical orientation stays TB-10.
- **What success proves.** The file header now says that a SETCRTC or DIRTYFB returning success
  only confirms the frame was *submitted*. The driver's plane update discards the USB result
  (appletbdrm.c at stable 497791a0, L474–505; TB-46 receipt), so a lit frame is HIL evidence.
- **Overflowing pitch case fixed.** `with_dumb_pitch(u32::MAX)` now also sets
  `with_dumb_size(240 * 2170)`. Without it, a fake that maps `pitch × 2170` bytes would try to
  allocate about 9 TB. The refusal (`pitch × height` overflows, or exceeds the mapping) is
  unchanged.
- **Compacted.** The unbound and absent explicit-card cases are now one test, and so are the two
  real-adapter cases (a missing node is never created; an ordinary file fails at VERSION). The IDs
  were renumbered to QA-101…QA-144.
- **doc_markdown.** `NotACardPath` is backticked in a doc comment.

## Viability (before sealing)

The test files were compiled and run in a throwaway copy of `c1f76d1` in the QA agent's
scratchpad, against a quick reference `drm`/`target`/`card`/`RecordingKms` implementation (about
600 lines, not committed). All 31 + 13 contract tests passed, and so did all 11 `sim_display`
tests. This included the real `rustix` VERSION ioctl on an ordinary file returning ENOTTY, and
`fstat` reporting a non-character node. The contract can be satisfied, and no case is vacuous by
construction. rustfmt `--check` passes.

## Implementer notes

- `KmsCall` must keep at least one read variant (e.g. `Version`, `Metadata`), or mark the enum
  `#[non_exhaustive]`. Otherwise the `_ => None` arm in `changes()` is unreachable and clippy
  `-D warnings` fails.
- `fail_at(step, _)` fails the first occurrence of that step only. `Metadata` means the first
  metadata read.
- Validate pitch and size before ADDFB2, using checked u32/u64 arithmetic. Validating before
  MAP_DUMB is allowed.
- The fake's `pixels()` stays readable after destroy, and any out-of-bounds write must panic.

## Left to later slices or review (unchanged from the draft)

- The composition open → `node_id` → `confirm_touch_bar_node` → `start` (confirmation of the
  character device, major:minor, USB IDs and driver before master) is wired and asserted in
  slice 3.
- The uapi struct-size `const` asserts live in `card.rs` and are checked at review.
- That serials are never read cannot be observed: review only.
- Nothing here is HIL evidence (TB-10).
