//! The Touch Bar daemon (`touchbard`): renders the shown layer only when something changes and
//! presents each frame with its damage through a display backend (ADR-0002, ADR-0006).

mod display;
pub mod fake;

pub use display::{Display, DisplayError, SimDisplay};
