//! Touches on the bar. Only x matters: the bar is one row of buttons.

/// One touch report, in bar pixels.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Touch {
    /// A finger is down at column `x`.
    Down { x: u16 },
    /// No finger on the bar.
    Up,
}
