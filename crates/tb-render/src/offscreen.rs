//! Headless rendering to pixels: the same path the daemon will use to feed the Touch Bar.

use std::io::BufWriter;
use std::path::Path;

use rune::render::{Gpu, OFFSCREEN_FORMAT, Offscreen, Renderer as Pipeline};
use rune::text::TextSystem;
use rune::{Env, Runtime};
use tb_core::BarSize;
use thiserror::Error;

use crate::scene::BarScene;
use crate::view::bar_view;

/// A rendered bar: tightly packed RGBA8 (sRGB), row-major, `width * height * 4` bytes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Frame {
    pub width: u32,
    pub height: u32,
    pub rgba: Vec<u8>,
}

impl Frame {
    /// The RGBA value at `(x, y)`.
    pub fn pixel(&self, x: u32, y: u32) -> [u8; 4] {
        let i = ((y * self.width + x) * 4) as usize;
        [self.rgba[i], self.rgba[i + 1], self.rgba[i + 2], self.rgba[i + 3]]
    }

    pub fn write_png(&self, path: &Path) -> Result<(), FrameError> {
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir)?;
        }
        let file = BufWriter::new(std::fs::File::create(path)?);
        let mut encoder = png::Encoder::new(file, self.width, self.height);
        encoder.set_color(png::ColorType::Rgba);
        encoder.set_depth(png::BitDepth::Eight);
        encoder.write_header()?.write_image_data(&self.rgba)?;
        Ok(())
    }
}

#[derive(Debug, Error)]
pub enum FrameError {
    #[error(transparent)]
    Io(#[from] std::io::Error),
    #[error(transparent)]
    Png(#[from] png::EncodingError),
}

#[derive(Debug, Error)]
pub enum RenderError {
    #[error("cannot start the GPU: {0}")]
    Gpu(String),
}

/// Draws scene after scene with one GPU device, Rune pipeline, offscreen target and text system,
/// so a frame costs a draw and a readback, not a device and a pipeline (`render_frame`).
pub struct Renderer {
    gpu: Gpu,
    pipeline: Pipeline,
    target: Offscreen,
    /// The CPU side of the glyph atlas `pipeline` uploads from; lent to each scene's `Runtime`.
    text: TextSystem,
}

impl std::fmt::Debug for Renderer {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Renderer")
            .field("adapter", &self.gpu.adapter.get_info().name)
            .field("target", &(self.target.width, self.target.height))
            .finish_non_exhaustive()
    }
}

impl Renderer {
    /// Creates the GPU device, Rune's pipeline and an offscreen target. Err without a usable
    /// adapter or device (lavapipe is fine). Create it once and render every frame with it.
    pub fn new() -> Result<Renderer, RenderError> {
        let gpu = Gpu::try_new(Gpu::instance(), None).map_err(RenderError::Gpu)?;
        Ok(Self::on(gpu, BarSize::MACBOOK_PRO))
    }

    /// The target starts at `size`, the size of the first scene expected.
    fn on(gpu: Gpu, size: BarSize) -> Renderer {
        let pipeline = Pipeline::new(&gpu.device, OFFSCREEN_FORMAT);
        let target = Offscreen::new(&gpu.device, u32::from(size.width), u32::from(size.height));
        Renderer { gpu, pipeline, target, text: TextSystem::default() }
    }

    /// Draws `scene` at 1:1 and reads it back: the same bytes as `render_frame(scene.clone())`.
    /// A scene of another size replaces the target and keeps the device.
    pub fn render(&mut self, scene: &BarScene) -> Frame {
        self.draw(scene.clone())
    }

    fn draw(&mut self, scene: BarScene) -> Frame {
        let (width, height) = (u32::from(scene.size.width), u32::from(scene.size.height));
        if (self.target.width, self.target.height) != (width, height) {
            self.target = Offscreen::new(&self.gpu.device, width, height);
        }
        let viewport = (f32::from(scene.size.width), f32::from(scene.size.height));
        let text = std::mem::take(&mut self.text);
        let mut rt = Runtime::with_text(scene, bar_view, viewport, Env::new(true, 1.0), text);
        rt.frame(0.0);
        let (device, queue) = (&self.gpu.device, &self.gpu.queue);
        self.pipeline.prepare(device, queue, &mut rt.text.atlas, &rt.instances, (width, height));
        // wgpu is Rune's dependency, not ours: we cannot name `CommandEncoderDescriptor` without
        // pinning a second wgpu version, so take the default through inference.
        #[expect(clippy::default_trait_access)]
        let mut encoder = device.create_command_encoder(&Default::default());
        self.pipeline.draw(&mut encoder, &self.target.view, [0.0; 4]);
        self.target.copy(&mut encoder);
        self.pipeline.submit(queue, encoder);
        self.text = std::mem::take(&mut rt.text);
        Frame { width, height, rgba: self.target.read(device) }
    }
}

/// Renders `scene` offscreen at 1:1 and reads the pixels back. Needs a GPU adapter
/// (any Vulkan driver, including lavapipe in CI); no window or display. Panics without one;
/// a caller that draws more than one frame keeps a [`Renderer`] instead.
pub fn render_frame(scene: BarScene) -> Frame {
    Renderer::on(Gpu::headless(), scene.size).draw(scene)
}
