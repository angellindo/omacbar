//! Fakes at the daemon's device seams, for tests here and in later tickets (testing standard rule 2).

use tb_core::{Rect, Rgba};
use tb_render::{BarScene, Frame};

use crate::display::{Display, DisplayError};
use crate::render::SceneRenderer;

/// One present as the daemon sent it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Presented {
    pub frame: Frame,
    pub damage: Vec<Rect>,
}

/// A display that records every present in order and never fails.
#[derive(Debug, Default)]
pub struct RecordingDisplay {
    presents: Vec<Presented>,
}

impl RecordingDisplay {
    pub fn new() -> Self {
        Self::default()
    }

    /// Every present so far, oldest first.
    pub fn presents(&self) -> &[Presented] {
        &self.presents
    }
}

impl Display for RecordingDisplay {
    fn present(&mut self, frame: &Frame, damage: &[Rect]) -> Result<(), DisplayError> {
        self.presents.push(Presented { frame: frame.clone(), damage: damage.to_vec() });
        Ok(())
    }
}

/// A CPU renderer for tests: the background everywhere, then each button rect filled with its
/// `fill` as-is (no blending, text, border or radius).
#[derive(Debug, Clone, Copy, Default)]
pub struct FlatRenderer;

impl SceneRenderer for FlatRenderer {
    fn render(&mut self, scene: &BarScene) -> Frame {
        let (width, height) = (usize::from(scene.size.width), usize::from(scene.size.height));
        let rgba = |c: Rgba| [c.r, c.g, c.b, c.a];
        let mut frame = Frame {
            width: u32::from(scene.size.width),
            height: u32::from(scene.size.height),
            rgba: rgba(scene.background).repeat(width * height),
        };
        for button in &scene.buttons {
            let r = button.rect;
            let fill = rgba(button.fill).repeat(usize::from(r.width));
            for y in usize::from(r.y)..(usize::from(r.y) + usize::from(r.height)).min(height) {
                let start = (y * width + usize::from(r.x)) * 4;
                let end = (start + fill.len()).min((y + 1) * width * 4);
                frame.rgba[start..end].copy_from_slice(&fill[..end - start]);
            }
        }
        frame
    }
}
