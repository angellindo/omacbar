# 0004 · Functional / E2E testing options

Status: resolved (TB-4)

| Option | What it drives | Fit for this project | Verdict |
|---|---|---|---|
| **Rune headless** (`Runtime` in `cargo test`, `--snap`, `--tree`, `--script`) | The real view + runtime on a virtual clock; real GPU renderer offscreen | Exact: same code path as the bar; deterministic; no display; CI-able with lavapipe | **Primary functional layer** |
| **uinput/evdev fakes** | A virtual multitouch device and keyboard feeding the daemon's input layer | Exercises touch → hit → action without the panel | **Daemon functional layer** (TB-21) |
| Hardware-in-the-loop checklist | The real Touch Bar | Only way to verify panel output, rotation, brightness | **Manual, per hardware ticket** |
| [Vercel agent-desktop](https://github.com/vercel-labs/agent-desktop) | Desktop apps through the AT-SPI accessibility tree | Pointer input refused on Wayland, GNOME-focused, needs Rune's accessibility feature on; cannot see the Touch Bar at all | Not now; maybe a smoke test of the configurator later |
| [TesterArmy](https://tester.army) | Web and mobile apps (cloud agents, MCP/CLI) | No native Linux desktop or Touch Bar support | Not applicable |

Rules that follow (see `docs/standards/testing.md`): assert on the element tree first (keys,
rects, text), pixels second with a tolerance, and keep pixel goldens out of unit tests.

## Sources

- Rune `crates/rune/README.md` § Agent tooling, § Testing stories
- [flaviocopes.com/agent-desktop](https://flaviocopes.com/agent-desktop) (Linux = AT-SPI; Wayland pointer input refused)
- [docs.tester.army](https://docs.tester.army/) (web + mobile)
