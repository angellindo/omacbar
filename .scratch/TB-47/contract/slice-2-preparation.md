# TB-47 slice 2: QA spec preparation (digitizer)

Base: `6415d13` (slice-1 PASS `6306198`, seal `6a62d8c5`, plus main/TB-44 `5b0f2de` with the real
`Touch::Cancel` unit variant, which this slice consumes and does not redefine). Frozen:
`crates/tb-daemon/tests/digitizer_touches.rs` (15 required cases), plus 5 preserve cases in
`held_modifiers` and `fn_keyboard`. The unsealed draft (`qa-draft/slice-2`, 22 Nov) was the
starting point; the orchestrator's resolution on it is applied here.

## Public interface the frozen file pins

All re-exported from `tb_daemon`. Names and field shapes are binding; everything else (the
`Calibration` internals, `DigitizerError` variants and wording beyond what the refusal must name,
the `TargetError` variants for a refused node, the record reader, the ioctl module) is the
implementer's choice.

```rust
pub fn find_digitizer(devices: &str) -> Result<PathBuf, TargetError>;              // auto
pub fn check_digitizer(devices: &str, node: &Path) -> Result<PathBuf, TargetError>; // explicit
pub fn confirm_digitizer_node(sysfs: &Path, node: &Path, id: NodeId) -> Result<PathBuf, TargetError>;
// TargetError and NodeId are slice 1's; tests match NotFound and Ambiguous(Vec<PathBuf>) only.

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct AbsAxis { pub code: u16, pub min: i32, pub max: i32 }
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct DigitizerMeta { pub bus: u16, pub vendor: u16, pub product: u16, pub name: String,
                           pub axes: Vec<AbsAxis> /* more fields allowed */ }
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct DigitizerProfile { pub bus: u16, pub vendor: u16, pub product: u16,
                              pub name: &'static str, pub axis: AbsAxis, pub reversed: bool }
pub const VERIFIED_DIGITIZERS: &[DigitizerProfile] = &[];   // const or static slice, empty

pub struct Calibration { .. }
pub fn calibrate(meta: &DigitizerMeta, profiles: &[DigitizerProfile], width: u16)
    -> Result<Calibration, DigitizerError>;                  // DigitizerError: Debug + Display

pub struct EvdevTouches<R: Read> { .. }
impl<R: Read> EvdevTouches<R> { pub fn new(reader: R, calibration: Calibration) -> Self; }
impl<R: Read> TouchSource for EvdevTouches<R> { .. }

pub struct DigitizerNode { .. }                 // the real evdev boundary (unsafe lives here)
impl DigitizerNode {
    pub fn open(path: &Path) -> io::Result<Self>;          // O_RDONLY|O_CLOEXEC, no O_CREAT, nothing else
    pub fn node_id(&self) -> io::Result<NodeId>;            // fstat
    pub fn metadata(&self) -> io::Result<DigitizerMeta>;    // EVIOCGID, EVIOCGNAME, EVIOCGABS
}
pub fn open_digitizer(sysfs: &Path, node: &Path, profiles: &[DigitizerProfile])
    -> Result<EvdevTouches<File>, DigitizerError>;
```

`open_digitizer` composes, in this order: lexical `/dev/input/eventN` check, `DigitizerNode::open`,
`node_id`, `confirm_digitizer_node`, `metadata`, `calibrate`, then `EVIOCGRAB`. Nothing is grabbed
before the opened node is confirmed and calibrated.

## Rules the tests pin

- **Discovery** (`/proc/bus/input/devices` text): `Bus=0003 Vendor=05ac Product=8600`, `EV_ABS`,
  ABS bits `MT_SLOT`, `MT_TRACKING_ID`, `MT_POSITION_X`, `MT_POSITION_Y`, and an `eventN` handler.
  The same-id keyboard, `05ac:8302`, a device without slots or tracking ids, and another bus or
  vendor are never chosen. `auto`: zero is `NotFound`, two are `Ambiguous` (both listed).
  Explicit: lexically `/dev/input/eventN` and a candidate's handler.
- **Node binding** (fake sysfs, kernel shape `class/input/eventN` → `…/inputM/eventN` with `dev`,
  `device` → `inputM` with `id/{bustype,vendor,product}` as `%04x`): Ok only if the opened node is a
  character device whose major:minor equals sysfs `dev`, and sysfs ids are `0003`/`05ac`/`8600`.
  Refused: another minor (renumbered), another major, not a character device, the T2 node, another
  bus/vendor, a node sysfs does not list, a path outside `/dev/input`.
- **Calibration**: Ok only for a profile with the same bus, vendor, product and name, with
  `MT_SLOT`, `MT_TRACKING_ID` and the profile's axis reported with exactly its min and max, and a
  range spanning at least one unit (empty or inverted is refused, never divided by). With the empty
  table the message names the observed vendor, product, name and the axis min/max.
- **Replay (MT-B, emitted at `SYN_REPORT`)**: one followed contact, the first new tracking id
  while none is followed; `Down{x}` once its slot has a position on the profile's axis; `Down`
  again only when the column changes; `-1` gives `Up`; other contacts are never promoted;
  `x = (v − min)·(W−1)/(max − min)`, mirrored when `reversed`.
- **Sync** (orchestrator resolution): while in sync a slot keeps its last position across contacts
  (the kernel does not resend unchanged values). At the start and after `SYN_DROPPED` the current
  slot and every cached position are unknown: records before a slot is named are ignored, and a
  new contact lands only with a position received after the loss. A `SYN_REPORT` alone proves
  nothing; the stale contact (no new tracking id) is never followed again.
- **Cancel, contact forgotten**: `SYN_DROPPED` (only when something is followed), a position
  outside `[min, max]`, a new tracking id in the followed slot without `-1` first (the new contact
  is then followed). End of stream mid-touch: `Cancel` then `Ok(None)` (including a lift whose
  report never came); a cut record: `Cancel` then `Err`; a read error: `Cancel` then
  `Err(TouchError::Io)` keeping ENODEV. No contact down: no `Cancel`.
- **Through the daemon**: 10 scripted streams through `EvdevTouches → forward_touches → Daemon`
  with `RecordingEmitter`/`RecordingSpawner`: each tap fires its key or command once; cancel,
  lost-sync, out-of-range, end/unplug and a stream joined mid-touch fire nothing and leave the last
  frame equal to the first.

## What each case proves, and what it does not

| Case | Proves (offline) | Does not prove (HIL, TB-45) |
|---|---|---|
| QA-201 | The shipped table is empty and refuses; the refusal carries the evidence TB-45 needs | The T1's real id, name, ranges |
| QA-202, QA-203 | Exact identity and range match, required MT axes, no division by an empty range | That the T1 reports MT-B at all |
| QA-204, QA-205, QA-206 | Mapping, dedupe, edges, reversal, axis selection, single contact | Which axis runs along the T1 bar, its direction, the physical left end |
| QA-207, QA-208, QA-209 | Fail-closed cancel and resync rules on synthetic byte streams | Real `SYN_DROPPED` frequency, unplug/suspend behaviour |
| QA-210 | The real `Daemon` runs each action at most once and clears the highlight | Real key injection or command execution (recording fakes) |
| QA-211–QA-213 | Discovery over synthetic `/proc` text | The T1's real `/proc` block |
| QA-214 | Node binding logic over a fake sysfs tree | The real sysfs shape on the T1 kernel |
| QA-215 | The real `open`/`fstat`/`EVIOCGID` code runs on an ordinary file (ENOTTY), no write, no create | Any success path: identity read, `EVIOCGRAB`, reads from a real node |

Review-only (not observable offline): the order confirm → calibrate → grab inside
`open_digitizer`; that no T2 numbers or serial reads appear in `src/`; struct-size asserts of
`input_id` (8) and `input_absinfo` (24) in the ioctl module; `O_NOFOLLOW`. Slice 3 wires
`open_digitizer` into `touchbard --touch` and asserts the refusals through the binary.

## Viability (before sealing)

See `.scratch/TB-47/qa-spec-s2/20261125T040753Z/README.md`: the frozen file was compiled and run
against a throwaway reference implementation (not committed, kept only in that folder) in a copy
of `6415d13`, with clippy `-D warnings` on all `tb-daemon` targets.

## Implementer notes

- `DigitizerProfile` must stay const-constructible (`&'static str` name).
- The empty `VERIFIED_DIGITIZERS` means production `open_digitizer` always refuses after reading
  the identity; that is the intended evidence path for TB-45, not a defect.
- The sysfs `id/*` attributes are lowercase `%04x` (`drivers/input/input.c`,
  `INPUT_DEV_ID_ATTR`); `dev` is `major:minor`.
- Use `TargetError` variants of your choice for a refused or mismatched node; tests only use
  `is_err()` there.
