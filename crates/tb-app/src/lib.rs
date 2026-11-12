//! The configurator (`touchbar-customizer`) as a library, so tests drive the real view headless
//! through `rune::Runtime`: open a config file, select, reorder, edit and save its buttons.

mod app;
mod diagnostics;
mod drafts;
mod panel;
mod preview;
mod saving;
mod shell;
mod workspace;

pub use app::{State, WINDOW, on_key, spec, view};
