//! Headless rendering to pixels: the same path the daemon will use to feed the Touch Bar.

use std::io::BufWriter;
use std::path::Path;

use rune::render::{Gpu, Offscreen, Renderer};
use rune::{Env, Runtime};
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

/// Renders `scene` offscreen at 1:1 and reads the pixels back. Needs a GPU adapter
/// (any Vulkan driver, including lavapipe in CI); no window or display.
pub fn render_frame(scene: BarScene) -> Frame {
    let (width, height) = (u32::from(scene.size.width), u32::from(scene.size.height));
    let viewport = (f32::from(scene.size.width), f32::from(scene.size.height));
    let gpu = Gpu::headless();
    let mut renderer = Renderer::new(&gpu.device, rune::render::OFFSCREEN_FORMAT);
    let target = Offscreen::new(&gpu.device, width, height);
    let mut rt = Runtime::new(scene, bar_view, viewport, Env::new(true, 1.0));
    rt.frame(0.0);
    renderer.prepare(&gpu.device, &gpu.queue, &mut rt.text.atlas, &rt.instances, (width, height));
    // wgpu is Rune's dependency, not ours: we cannot name `CommandEncoderDescriptor` without
    // pinning a second wgpu version, so take the default through inference.
    #[expect(clippy::default_trait_access)]
    let mut encoder = gpu.device.create_command_encoder(&Default::default());
    renderer.draw(&mut encoder, &target.view, [0.0; 4]);
    target.copy(&mut encoder);
    renderer.submit(&gpu.queue, encoder);
    Frame { width, height, rgba: target.read(&gpu.device) }
}
