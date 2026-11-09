//! Touch Bar domain model. Pure logic only: no IO, no GPU, no device access.
//!
//! Vocabulary lives in `GLOSSARY.md`. Pixel rules live in `docs/design/button-standard.md`.

pub mod chord;
pub mod config;
pub mod edit;
pub mod geometry;
pub mod hit;
pub mod key;
pub mod layout;
pub mod model;
pub mod presets;
pub mod reorder;
pub mod spacing;
pub mod theme;

pub use chord::{Chord, Modifiers};
pub use geometry::{BarSize, Rect};
pub use key::Key;
pub use layout::{LayoutError, Placed, place};
pub use model::{Action, Button, ButtonId, ButtonStyle, Group, Layer, Layout, Width};
pub use spacing::SpacingRules;
pub use theme::{BarTheme, Rgba};
