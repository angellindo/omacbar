# TB-47 slice 3: QA spec preparation (the `touchbard` entry path)

Base: `5ce810b` (slice 2 PASS on this commit, seal `08bb9abe`; slice 1 final `6306198`, seal
`6a62d8c5`, re-checked green on `5ce810b`). Frozen: `crates/tb-daemon/src/main/tests.rs`,
`crates/tb-daemon/src/main/hardware_tests.rs` (its child module) and
`crates/tb-daemon/tests/touchbard_hardware.rs`. 15 required cases, 11 preserve cases. The unsealed
draft (`qa-draft/slice-3`, 22 Nov) was the starting point; it was rewritten against the real
slice 1/2 API and the orchestrator's decisions on it.

## Seam the plan did not spell out (pinned here)

The plan names `run_daemon` generic over `SceneRenderer + Display`, `Message::Stop`, and
`main/tests.rs` as the observation point, but no function that reaches the device run with fakes.
The binary cannot: its paths are `/sys`, `/proc/bus/input/devices` and `/dev`. So the contract pins
one bin-private seam, inside the plan's own files (`main.rs` or the `devices.rs` bin module), and
nothing in the library beyond what the plan already lists (`block_stop_signals`, `StopSignals`,
`Daemon::display_mut` is allowed but not required by the tests):

```rust
// reachable from src/main/tests.rs through `use super::*` (main.rs imports it if it lives elsewhere)
struct Devices<K, T> {
    sysfs: PathBuf,                                                    // `/sys` for real
    input_devices: PathBuf,                                            // `/proc/bus/input/devices` for real
    open_card: Box<dyn FnOnce(&Path) -> io::Result<(K, NodeId)>>,      // CardKms::open + node_id
    open_touch: Box<dyn FnOnce(&Path) -> Result<T, String>>,           // open_digitizer(/sys, .., VERIFIED_DIGITIZERS)
}

fn run_hardware<R, K, T>(args: Args, home: Option<&Path>, renderer: R, devices: Devices<K, T>,
                         stop: Option<StopSignals>) -> Result<(), String>
where R: SceneRenderer, K: Kms, T: TouchSource + Send + 'static;

// tb_daemon (library), ADR-0014 unsafe module stop_signals.rs
pub fn block_stop_signals() -> io::Result<StopSignals>;   // blocks SIGHUP, SIGINT, SIGTERM on the calling thread
pub struct StopSignals { .. }                              // Send; waits with sigwait; remembers the caller's mask
```

`Devices` is built with a struct literal of exactly these four fields. `Args` keeps `sim_dir`,
`keys`, `emit`, `window` and may gain fields (the parse tests take every other field from
`parse_args` of no arguments). Not frozen: field names for the new flags, `Message` and its
variants, `StopSignal`, `Devices::real`, which module holds the seam, error wording beyond the
two words below, and how the caller's mask reaches the command.

`main` for a device run: `block_stop_signals()` first, before `GpuRenderer::new` or any thread,
then `run_hardware(.., Devices::real(), Some(stop))`. Without `--drm`/`--touch`: today's path,
no blocking, no `/sys` or `/proc` read. Review checks this; no offline test can (gap 1).

## Rules the tests pin

- **Flags** (QA-301): `--drm auto|PATH`, `--touch auto|PATH`, each once, with a value, in any
  order with `--keys`/`--emit`; the four forms parse to distinct args. Usage error (exit 2):
  `--drm` with `--sim` or `--sim-window`, `--touch` with `--sim-window`. `--touch` with `--sim` is
  allowed (touches drive the simulator). The path given is observed through the run (QA-303/306:
  the opener is asked for exactly that path), not through `Debug`.
- **Lexical first** (QA-302, QA-315): a path that is not lexically `/dev/dri/cardN` or
  `/dev/input/eventN` (outside `/dev`, or through `..`) is refused before sysfs or the input
  devices text is read, before any open, before any frame; the error names the path. Binary: exit
  1 (not 2), the ordinary file is unchanged, no `--sim` frame, no `out/` in the working dir.
- **Card identity** (QA-303/304): explicit desktop GPU, explicit T2, auto with none, auto with two
  → no card and no digitizer opened. Auto and the explicit T1 path open `/dev/dri/card1` only. An
  opened node whose minor, major or char-device bit disagrees with sysfs is closed with no other
  call (`close` only), before master; no digitizer opened.
- **Startup failures** (QA-305/306): master busy, a foreign frame shown, another orientation →
  no SETCRTC, the digitizer not opened. A digitizer that is no candidate, the same-id keyboard,
  auto with two, or an opener refusal → no SETCRTC, the card never opened (`close`) or started and
  released in reverse (`set_master, create_dumb, map_dumb, add_fb2, rm_fb, unmap, destroy_dumb,
  drop_master, close`); the opener is asked only for a chosen candidate, and its reason is in the
  error. Auto opens the one candidate (`/dev/input/event7`).
- **Exits restore once** (QA-307–311): EOF after a tap → `Ok`, F5 pressed once, one modeset, one
  disabling SETCRTC (`fb 0`, no connector, no mode) then `rm_fb, unmap, destroy_dumb, drop_master,
  close`. Lost sync or an out-of-range sample mid-touch → `Ok`, no action, the final buffer equals
  an untouched run's (nothing left pressed), one restore. A read error mid-touch, or a present
  failing with EIO → `Err`, one restore, no action. Unplug (present ENODEV + read ENODEV) → `Err`
  naming "gone", no disabling SETCRTC, buffer/mapping/master/card still released. RMFB failing
  in the restore → `Err` naming "restor", every later release step still attempted.
- **Touch without display** (QA-312): `--touch` + `--sim` drives `SimDisplay` (frames 1–3 for a
  tap) and never calls the card opener.
- **Stop signals** (QA-313): the test binary re-runs itself as a child on one `#[ignore]`d test,
  started by GNU `env --block-signal=HUP,INT,TERM`. The child runs `run_hardware` with
  `block_stop_signals()`, a digitizer whose reader blocks forever, and a card that prints `ready`
  after its first modeset. The parent waits for that line on the child's stdout, sends SIGHUP,
  SIGINT or SIGTERM to that pid only, and allows 10 s; the child checks `Ok`, one modeset, one
  restore and the release tail, then prints `done`. No sleep decides anything: the parent reads
  lines with deadlines and kills only its own child on timeout.
- **Command mask** (QA-314): the child reads its thread's `SigBlk`, calls `block_stop_signals`
  (then HUP, INT and TERM must be blocked), taps a command `exec grep SigBlk /proc/self/status >
  FIFO`, and reports both masks. Rows: nothing blocked (0), SIGUSR1 blocked (0x200), the stop
  signals already blocked (0x4003). The command must run with exactly the caller's mask: the
  daemon's own blocking is removed, earlier blocks are kept.

Preserve: the six existing parse tests (QA-320–325, only their `sim` helper changed; their
defaults are now asserted explicitly), the simulator still ending on SIGHUP/SIGINT/SIGTERM by
the default action (QA-326), and the default stdin/emit simulator runs (QA-327–330).

## What each case proves, and what it does not

| Case | Proves (offline) | Does not prove (HIL, TB-10/TB-45) |
|---|---|---|
| QA-301 | Flag grammar and conflicts | — |
| QA-302, QA-315 | Lexical refusal before any read, open or frame; exit 1; file untouched | — |
| QA-303, QA-304 | Sysfs identity before open, node binding before master, over a temp tree | The real T1 sysfs shape and minor |
| QA-305, QA-306 | Refusals leave no SETCRTC and release in reverse (recording card) | A real busy master, real foreign state |
| QA-307–QA-311 | One restore on every exit through the real `Daemon`/`DrmDisplay`/`EvdevTouches` | That a disable clears the physical panel, or a lit frame; a submitted KMS call is not a visible frame |
| QA-312 | `--touch` on the simulator | — |
| QA-313 | A process signal becomes one restore and `Ok`, with the reader still blocked | That `main` blocks before any thread (review); real panel state after the stop; SIGKILL |
| QA-314 | Commands get the caller's mask, not the daemon's | — |

Review-only: `main` calls `block_stop_signals()` before any thread in a device run and not
otherwise; `Command::new` stays in `ShSpawner` (no USB, service or privilege commands); no
`/sys`, `/proc` or `/dev` access without the flags; `Devices::real` wires `CardKms` and
`open_digitizer` with `VERIFIED_DIGITIZERS`.

## Gaps kept open on purpose

1. The real binary's signal wiring with a real signal is not exercised: a device run of the
   binary always refuses offline (lexical or sysfs), so `main`'s blocking order is review-only.
2. `--drm` without `--touch` (touches from stdin) is not driven in-process (the harness's stdin);
   the binary covers its lexical refusal only.
3. A touch read failure is pinned as `Err` (exit 1), like a bad stdin line today.
4. The child's stop row pre-blocks the signals with `env`; QA-314 separately proves that
   `block_stop_signals` blocks them on the calling thread.

## Viability (before sealing)

`.scratch/TB-47/qa-spec-s3/20261125T051234Z/README.md`: the frozen files were compiled and run
against a throwaway reference implementation (not committed, own target) in a copy of `5ce810b`
— 20/20 bin cases and 5/5 related integration cases green, clippy `-D warnings` clean on all
`tb-daemon` targets; five mutations of the reference each turned the intended case red. On the
pristine base with only the `mod hardware` line removed, the six parse cases are green and QA-301
fails on its first `--drm auto`.

## Implementer notes

- Check both paths lexically before choosing anything; then choose and start the card; then
  choose and open the digitizer.
- `std::process::Command` keeps the spawning thread's blocked mask on this host (a reference that
  spawned from the daemon thread leaked `0x4003` into the command). Spawn from a thread whose mask
  is the caller's, or set it in the child.
- `Drop` of `DrmDisplay` also restores; the run must call `restore` itself so a failure reaches the
  error (QA-311) and the restore happens before the run returns.
- A stop must not join the touch reader thread.
