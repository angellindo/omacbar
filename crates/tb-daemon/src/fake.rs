//! Fakes at the daemon's device seams, for tests here and in later tickets (testing standard rule 2).

use tb_core::Rect;
use tb_render::Frame;

use crate::display::{Display, DisplayError};

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
