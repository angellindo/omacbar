//! Turning a scene into pixels. The daemon renders through this trait so tests can use a CPU fake.

use tb_render::{BarScene, Frame};

/// Draws a whole bar scene into a frame of the scene's size.
pub trait SceneRenderer {
    fn render(&mut self, scene: &BarScene) -> Frame;
}

/// The real renderer: Rune offscreen on the GPU (`tb_render::render_frame`). Needs a Vulkan
/// adapter; lavapipe works headless.
#[derive(Debug, Default)]
pub struct GpuRenderer;

impl GpuRenderer {
    pub fn new() -> Self {
        Self
    }
}

impl SceneRenderer for GpuRenderer {
    fn render(&mut self, scene: &BarScene) -> Frame {
        tb_render::render_frame(scene.clone())
    }
}
