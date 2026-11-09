//! Touch Bar rendering: a Rune view of one layer, drawn at 1:1 device pixels.
//!
//! - [`scene`] resolves a layer + theme into what to draw (pure, testable without a GPU).
//! - [`view`] is the Rune view function for a [`BarScene`].
//! - [`offscreen`] renders a scene headless into an RGBA [`Frame`] (GPU, no window).

pub mod offscreen;
pub mod scene;
pub mod view;

pub use offscreen::{Frame, render_frame};
pub use scene::{BarScene, SceneButton, build_scene};
pub use view::{bar_view, label_style, label_width};
