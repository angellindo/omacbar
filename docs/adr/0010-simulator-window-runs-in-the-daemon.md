# The simulator window is a Rune app inside `touchbard`, fed in memory by `SimDisplay`

Amends ADR-0006 ("`tb-daemon` does not depend on `rune` directly"). `touchbard --sim-window` shows
the simulator's screen live in a desktop window and turns mouse presses on it into touches (TB-32).
The window is a Rune `App` in `tb-daemon` (module `sim_window`), so `tb-daemon` now depends on
`rune` directly. Rune is already linked into the binary through `tb-render`, so the build does not
grow; without `--sim-window` no window code runs and no display is needed.

How it is wired:

- **Frames, in memory.** `SimDisplay::on_present` takes a function that receives the whole screen
  after each non-empty present that succeeded. The window passes one that calls
  `rune::Pixels::set`, Rune's streaming image path ("call it from any thread", no decode, no
  mips). The window never reads the PNG files, which `SimDisplay` keeps writing.
- **Touches, as a `TouchSource`.** The window's screen element turns a primary press into
  `Touch::Down { x }` at the matching bar x and its release into `Touch::Up`, sent on a channel
  whose receiving end (`WindowTouches`) is a `TouchSource`. `touchbard` forwards it exactly as it
  forwards stdin lines, so both paths keep one contract. Closing the window closes the source.
- **Threads.** winit needs the process's main thread and `GpuRenderer` is not `Send` (Rune's text
  system holds `Rc`s), so with `--sim-window` the window runs on the main thread and the daemon
  (renderer, `Daemon`, its loop) is built and run on a spawned thread. Without the flag the daemon
  runs on the main thread as before.

## Considered options

- **Window view in `tb-render`.** `tb-render` draws and reads back pixels and takes no input
  (R1); the window owns pointer input and the app's lifetime.
- **A separate crate or binary for the window.** It would depend on `tb-daemon`, which ADR-0006
  keeps a leaf, and the ticket asks for a `touchbard` flag.
- **A cargo feature that gates `rune` in `tb-daemon`.** An unrequested flag for no build saving:
  `tb-render` links Rune anyway.
- **The window polls the PNG files.** Disk round trips and a race with a half-written file, for
  pixels `SimDisplay` already holds in memory.
