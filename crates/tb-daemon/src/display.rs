//! Display backends: where a presented frame shows up (ADR-0006). A present carries the whole frame
//! plus its damage; a backend shows only the damaged rects and keeps every other pixel as it was.

use std::path::PathBuf;

use tb_core::{BarSize, Rect};
use tb_render::Frame;
use tb_render::offscreen::FrameError;
use thiserror::Error;

/// A panel the daemon presents frames to.
pub trait Display {
    /// Shows the `damage` rects of `frame` on the panel; pixels outside them stay as they were.
    /// Empty damage: Ok, nothing shown.
    fn present(&mut self, frame: &Frame, damage: &[Rect]) -> Result<(), DisplayError>;
}

#[derive(Debug, Error)]
pub enum DisplayError {
    #[error("frame is {width}x{height} px, not the display's size")]
    FrameSize { width: u32, height: u32 },
    #[error("damage {rect:?} reaches past the bar; nothing was shown")]
    DamageOutsideBar { rect: Rect },
    #[error("cannot write the simulated screen: {0}")]
    Write(#[from] FrameError),
}

/// What `SimDisplay::on_present` calls with the screen.
type OnPresent = Box<dyn FnMut(&Frame) + Send>;

/// The simulator: a headless panel that writes its whole screen as `frame-NNNN.png` after each
/// non-empty present, and hands it to its `on_present` function (the simulator window).
pub struct SimDisplay {
    size: BarSize,
    screen: Frame,
    dir: PathBuf,
    written: u32,
    on_present: Option<OnPresent>,
}

impl std::fmt::Debug for SimDisplay {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SimDisplay")
            .field("size", &self.size)
            .field("dir", &self.dir)
            .field("written", &self.written)
            .field("on_present", &self.on_present.is_some())
            .finish_non_exhaustive()
    }
}

impl SimDisplay {
    /// Creates `dir`, removes earlier `frame-*.png` there; the screen starts opaque black.
    pub fn new(size: BarSize, dir: impl Into<PathBuf>) -> Result<Self, DisplayError> {
        let dir = dir.into();
        std::fs::create_dir_all(&dir).map_err(FrameError::from)?;
        for entry in std::fs::read_dir(&dir).map_err(FrameError::from)? {
            let path = entry.map_err(FrameError::from)?.path();
            let is_frame = path.file_name().and_then(|n| n.to_str()).is_some_and(|n| n.starts_with("frame-"))
                && path.extension().is_some_and(|e| e == "png");
            if is_frame {
                std::fs::remove_file(&path).map_err(FrameError::from)?;
            }
        }
        let (width, height) = (u32::from(size.width), u32::from(size.height));
        let pixels = usize::from(size.width) * usize::from(size.height);
        let screen = Frame { width, height, rgba: [0, 0, 0, 255].repeat(pixels) };
        Ok(Self { size, screen, dir, written: 0, on_present: None })
    }

    /// `f` sees the whole screen after each non-empty present that succeeded (after its PNG is
    /// written), in present order, from the presenting thread. An empty or refused present, or a
    /// failed write, does not call it. Replaces an earlier `f`.
    #[must_use]
    pub fn on_present(mut self, f: impl FnMut(&Frame) + Send + 'static) -> SimDisplay {
        self.on_present = Some(Box::new(f));
        self
    }

    /// What the simulated panel shows now (after every damage applied so far).
    pub fn screen(&self) -> &Frame {
        &self.screen
    }
}

impl Display for SimDisplay {
    fn present(&mut self, frame: &Frame, damage: &[Rect]) -> Result<(), DisplayError> {
        check(&self.screen, frame, damage)?;
        if damage.is_empty() {
            return Ok(());
        }
        // Staged: a failed write leaves the screen and the numbering as they were.
        let mut screen = self.screen.clone();
        for rect in damage {
            copy_rect(frame, &mut screen, self.size, *rect);
        }
        let number = self.written.saturating_add(1);
        screen.write_png(&self.dir.join(format!("frame-{number:04}.png")))?;
        (self.screen, self.written) = (screen, number);
        if let Some(f) = &mut self.on_present {
            f(&self.screen);
        }
        Ok(())
    }
}

/// Refuses a frame of another size, or a damage rect that does not fit on the screen.
fn check(screen: &Frame, frame: &Frame, damage: &[Rect]) -> Result<(), DisplayError> {
    if (frame.width, frame.height) != (screen.width, screen.height) {
        return Err(DisplayError::FrameSize { width: frame.width, height: frame.height });
    }
    let fits = |r: &Rect| {
        u32::from(r.x) + u32::from(r.width) <= screen.width
            && u32::from(r.y) + u32::from(r.height) <= screen.height
    };
    match damage.iter().find(|r| !fits(r)) {
        Some(&rect) => Err(DisplayError::DamageOutsideBar { rect }),
        None => Ok(()),
    }
}

/// Copies `rect` of `from` onto `to`; both frames are `size` and `rect` fits (see `check`).
fn copy_rect(from: &Frame, to: &mut Frame, size: BarSize, rect: Rect) {
    let row_bytes = usize::from(rect.width) * 4;
    let stride = usize::from(size.width) * 4;
    for y in usize::from(rect.y)..usize::from(rect.y) + usize::from(rect.height) {
        let start = y * stride + usize::from(rect.x) * 4;
        to.rgba[start..start + row_bytes].copy_from_slice(&from.rgba[start..start + row_bytes]);
    }
}
