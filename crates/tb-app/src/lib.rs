//! The configurator (`touchbar-customizer`) as a library, so tests drive the real view headless
//! through `rune::Runtime`: open a config file, select, reorder, edit and save its buttons.

mod app;
mod panel;
mod preview;

pub use app::{State, WINDOW, on_key, spec, view};
