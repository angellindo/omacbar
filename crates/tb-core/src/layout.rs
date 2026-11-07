//! The layout engine: turns a layer's buttons into pixel rects on the bar.
//!
//! Algorithm (see `docs/design/button-standard.md` § Width):
//! 1. Usable width = bar width - 2 x edge padding.
//! 2. Subtract gaps (`gap` inside a group, `group_gap` between groups) and every fixed (`Px`) width.
//! 3. Split what is left between `Auto`/`Span` buttons by shares; leftover pixels go one each to
//!    the leftmost shared buttons, so a layer with any shared button fills the bar exactly.
//! 4. Reject any button narrower than `min_button_width`.

use thiserror::Error;

use crate::geometry::{BarSize, Rect};
use crate::model::{ButtonId, Layer};
use crate::spacing::SpacingRules;

/// A button with its computed rect.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Placed {
    pub id: ButtonId,
    pub rect: Rect,
    pub group: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum LayoutError {
    #[error("buttons need {needed} px but the bar has {available} px")]
    Overflow { needed: u32, available: u32 },
    #[error("button `{id}` would be {width} px wide; the minimum is {min} px")]
    TooNarrow { id: ButtonId, width: u32, min: u16 },
}

/// Places every button of `layer`. Empty groups are ignored.
pub fn place(layer: &Layer, size: BarSize, rules: &SpacingRules) -> Result<Vec<Placed>, LayoutError> {
    let groups: Vec<_> = layer.groups.iter().enumerate().filter(|(_, g)| !g.buttons.is_empty()).collect();
    if groups.is_empty() {
        return Ok(Vec::new());
    }

    let available = u32::from(size.width.saturating_sub(2 * rules.edge_padding));
    let gaps: u32 = groups
        .iter()
        .map(|(_, g)| count(g.buttons.len() - 1).saturating_mul(u32::from(rules.gap)))
        .fold(0, u32::saturating_add)
        .saturating_add(count(groups.len() - 1).saturating_mul(u32::from(rules.group_gap)));
    let buttons = || groups.iter().flat_map(|(_, g)| g.buttons.iter());
    let fixed: u32 = buttons()
        .map(|b| match b.width {
            crate::model::Width::Px(px) => u32::from(px),
            _ => 0,
        })
        .sum();
    let shares: u32 = buttons().map(|b| b.width.shares()).sum();

    let needed =
        gaps.saturating_add(fixed).saturating_add(shares.saturating_mul(u32::from(rules.min_button_width)));
    if needed > available {
        return Err(LayoutError::Overflow { needed, available });
    }

    let free = available - gaps - fixed;
    let (per_share, mut leftover) = free.checked_div(shares).map_or((0, 0), |q| (q, free % shares));

    let height = rules.button_height(size.height);
    let mut x = u32::from(rules.edge_padding);
    let mut placed = Vec::new();
    for (pos, (group_index, group)) in groups.iter().enumerate() {
        if pos > 0 {
            x += u32::from(rules.group_gap);
        }
        for (i, button) in group.buttons.iter().enumerate() {
            if i > 0 {
                x += u32::from(rules.gap);
            }
            let width = match button.width {
                crate::model::Width::Px(px) => u32::from(px),
                w => {
                    let bonus = u32::from(leftover > 0);
                    leftover -= bonus;
                    per_share * w.shares() + bonus
                }
            };
            if width < u32::from(rules.min_button_width) {
                return Err(LayoutError::TooNarrow {
                    id: button.id.clone(),
                    width,
                    min: rules.min_button_width,
                });
            }
            placed.push(Placed {
                id: button.id.clone(),
                rect: Rect { x: px(x), y: rules.vertical_inset, width: px(width), height },
                group: *group_index,
            });
            x += width;
        }
    }
    Ok(placed)
}

/// Counts never get near `u32::MAX`; saturating keeps the overflow check honest if they did.
fn count(n: usize) -> u32 {
    u32::try_from(n).unwrap_or(u32::MAX)
}

/// Every value here is bounded by the bar width (u16), checked by the overflow test above.
fn px(v: u32) -> u16 {
    u16::try_from(v).unwrap_or(u16::MAX)
}

#[cfg(test)]
mod tests;
