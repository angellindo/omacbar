//! Turning a scene into pixels. The daemon renders through this trait so tests can use a CPU fake.

use tb_render::{BarScene, Frame, RenderError, Renderer};

/// Draws a whole bar scene into a frame of the scene's size.
pub trait SceneRenderer {
    fn render(&mut self, scene: &BarScene) -> Frame;
}

/// The real renderer: Rune offscreen on the GPU, with one kept `tb_render::Renderer` (one GPU
/// device) for its life. Needs a Vulkan adapter; lavapipe works headless.
#[derive(Debug)]
pub struct GpuRenderer {
    renderer: Renderer,
}

impl GpuRenderer {
    /// Starts the GPU device and pipeline; Err without a usable adapter. Create it once.
    pub fn new() -> Result<Self, RenderError> {
        Ok(Self { renderer: Renderer::new()? })
    }
}

impl SceneRenderer for GpuRenderer {
    fn render(&mut self, scene: &BarScene) -> Frame {
        self.renderer.render(scene)
    }
}
