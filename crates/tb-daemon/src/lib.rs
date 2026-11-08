//! The Touch Bar daemon (`touchbard`): renders the shown layer only when something changes and
//! presents each frame with its damage through a display backend (ADR-0002, ADR-0006).

mod daemon;
mod damage;
mod display;
pub mod fake;
mod render;
mod touch;

pub use daemon::{Daemon, DaemonError, Event};
pub use display::{Display, DisplayError, SimDisplay};
pub use render::SceneRenderer;
pub use touch::Touch;
