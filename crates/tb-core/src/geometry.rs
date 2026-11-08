//! Physical bar geometry. All values are device pixels: the bar is drawn at 1:1, never scaled.

/// Size of a Touch Bar panel in device pixels, landscape (width along the keyboard).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BarSize {
    pub width: u16,
    pub height: u16,
}

impl BarSize {
    /// T1 and T2 `MacBook` Pro panels: 2170 x 60 px (1085 x 30 pt at 2x).
    pub const MACBOOK_PRO: BarSize = BarSize { width: 2170, height: 60 };
}

/// An axis-aligned rectangle in device pixels, origin at the bar's left edge, top.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Rect {
    pub x: u16,
    pub y: u16,
    pub width: u16,
    pub height: u16,
}

impl Rect {
    /// Right edge (exclusive), saturating at `u16::MAX` for a rect reaching past the last column.
    pub const fn right(&self) -> u16 {
        self.x.saturating_add(self.width)
    }

    /// Whether column `x` lies inside the rect, judged on its true extent (no overflow).
    pub const fn contains_x(&self, x: u16) -> bool {
        x >= self.x && x - self.x < self.width
    }

    /// Horizontal distance from `x` to this rect (0 when inside), saturating at `u16::MAX`.
    pub const fn distance_x(&self, x: u16) -> u16 {
        if x < self.x {
            self.x - x
        } else if self.contains_x(x) {
            0
        } else {
            // `x - self.x >= width` here, so neither subtraction underflows.
            (x - self.x - self.width).saturating_add(1)
        }
    }
}
