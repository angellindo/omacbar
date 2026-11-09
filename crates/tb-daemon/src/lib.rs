//! The Touch Bar daemon (`touchbard`): renders the shown layer only when something changes and
//! presents each frame with its damage through a display backend (ADR-0002, ADR-0006).

mod daemon;
mod damage;
mod display;
pub mod fake;
mod fn_keyboard;
mod keys;
mod render;
mod sim_window;
mod touch;

pub use daemon::{Daemon, DaemonError, Event};
pub use display::{Display, DisplayError, SimDisplay};
pub use fn_keyboard::{FnKeyboard, find_fn_keyboard};
pub use keys::{EvdevModifiers, KeyError, ModifierSource, forward_modifiers};
pub use render::{GpuRenderer, SceneRenderer};
pub use sim_window::{SIM_WINDOW, SimWindow, WindowTouches, sim_window_app, sim_window_view};
pub use touch::{LineTouches, Touch, TouchError, TouchSource, forward_touches};
