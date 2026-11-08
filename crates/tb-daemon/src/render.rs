//! Turning a scene into pixels. The daemon renders through this trait so tests can use a CPU fake.

use tb_render::{BarScene, Frame};

/// Draws a whole bar scene into a frame of the scene's size.
pub trait SceneRenderer {
    fn render(&mut self, scene: &BarScene) -> Frame;
}
