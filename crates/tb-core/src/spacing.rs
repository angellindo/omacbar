//! Spacing rules: the numbers behind `docs/design/button-standard.md`. Change them there first.

/// Pixel spacing applied by the layout engine. All values sit on the 4 px grid.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SpacingRules {
    /// Minimum empty space between the bar edge and the first/last button.
    pub edge_padding: u16,
    /// Space between two buttons in the same group.
    pub gap: u16,
    /// Space between two groups.
    pub group_gap: u16,
    /// Space above and below every button.
    pub vertical_inset: u16,
    /// Narrowest button a finger can hit reliably.
    pub min_button_width: u16,
    /// Horizontal padding between a button edge and its label.
    pub label_padding: u16,
}

impl SpacingRules {
    pub const STANDARD: SpacingRules = SpacingRules {
        edge_padding: 88,
        gap: 12,
        group_gap: 88,
        vertical_inset: 8,
        min_button_width: 64,
        label_padding: 12,
    };

    /// Height of every button on a bar of `bar_height` px; 0 when the insets take it all.
    pub const fn button_height(&self, bar_height: u16) -> u16 {
        // Subtracting twice instead of `2 * vertical_inset` cannot overflow.
        bar_height.saturating_sub(self.vertical_inset).saturating_sub(self.vertical_inset)
    }

    /// Distance from a button within which a touch in a gap still counts as that button.
    pub const fn touch_slop(&self) -> u16 {
        self.gap / 2
    }
}

impl Default for SpacingRules {
    fn default() -> Self {
        Self::STANDARD
    }
}
