# The display backend is a seam; the app never assumes how pixels reach the bar

Two community kernel paths exist for T1 pixel mode (patched `appletbdrm` → DRM device;
`barkeep` → `/dev/dfr0`), and neither is upstream. The daemon presents frames through a `Display`
trait with three implementations: DRM dumb buffer (preferred: it is the mainline driver's
interface and what tiny-dfr uses), raw char device, and the simulator. Rotation and pixel-format
conversion (RGBA 2170x60 → device format, 60x2170) live in the backend, not the renderer. This
keeps hardware enablement (a risky, human-owned step) off the critical path of every other ticket.
