//! The layout engine: turns a layer's buttons into pixel rects on the bar.
//!
//! Algorithm (see `docs/design/button-standard.md` § Width):
//! 1. Usable width = bar width - 2 x edge padding.
//! 2. Subtract gaps (`gap` inside a group, `group_gap` between groups) and every fixed (`Px`) width.
//!    Totals saturate, so a total past `u32` is an `Overflow` needing `u32::MAX` px.
//! 3. Reject the layer with `Overflow` when that leaves less than `min_button_width` per shared
//!    (`Auto`/`Span`) button: counted per button, not per share, so wide spans are not charged
//!    for a minimum they do not need.
//! 4. Give each shared button the whole pixels of its shares (`free x span / total shares`,
//!    rounded down). Each floor loses under 1 px, so the leftover is always fewer pixels than
//!    there are shared buttons: one each to the leftmost fills the bar exactly. With only fixed
//!    buttons, unused width stays empty on the right.
//! 5. Reject, by name, the first button whose allocated width is under `min_button_width`.

use thiserror::Error;

use crate::geometry::{BarSize, Rect};
use crate::model::{ButtonId, Group, Layer, Width};
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

    // Subtracting twice instead of `2 * edge_padding` cannot overflow.
    let available =
        u32::from(size.width.saturating_sub(rules.edge_padding).saturating_sub(rules.edge_padding));
    let group_gap = layer.group_gap.unwrap_or(rules.group_gap);
    let gaps = gaps_width(&groups, rules.gap, group_gap);
    let buttons = || groups.iter().flat_map(|(_, g)| g.buttons.iter());
    let fixed = buttons().map(|b| fixed_px(b.width)).fold(0, u32::saturating_add);
    let shares = buttons().map(|b| b.width.shares()).fold(0, u32::saturating_add);
    let shared_buttons = count(buttons().filter(|b| !matches!(b.width, Width::Px(_))).count());

    let needed = gaps
        .saturating_add(fixed)
        .saturating_add(shared_buttons.saturating_mul(u32::from(rules.min_button_width)));
    if needed > available {
        return Err(LayoutError::Overflow { needed, available });
    }

    let free = available - gaps - fixed;
    let floored: u32 = buttons().map(|b| share_of(free, b.width, shares)).sum();
    let mut leftover = free - floored;

    let height = rules.button_height(size.height);
    let mut x = u32::from(rules.edge_padding);
    let mut placed = Vec::new();
    for (pos, (group_index, group)) in groups.iter().enumerate() {
        if pos > 0 {
            x += u32::from(group_gap);
        }
        for (i, button) in group.buttons.iter().enumerate() {
            if i > 0 {
                x += u32::from(rules.gap);
            }
            let width = match button.width {
                Width::Px(_) => fixed_px(button.width),
                w => {
                    let bonus = u32::from(leftover > 0);
                    leftover -= bonus;
                    share_of(free, w, shares) + bonus
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

/// Total width of the gaps between the buttons of non-empty `groups` and between the groups.
fn gaps_width(groups: &[(usize, &Group)], gap: u16, group_gap: u16) -> u32 {
    groups
        .iter()
        .map(|(_, g)| count(g.buttons.len() - 1).saturating_mul(u32::from(gap)))
        .fold(0, u32::saturating_add)
        .saturating_add(count(groups.len() - 1).saturating_mul(u32::from(group_gap)))
}

/// Width of a `Px` button; 0 for shared (`Auto`/`Span`) buttons.
fn fixed_px(w: Width) -> u32 {
    match w {
        Width::Px(px) => u32::from(px),
        Width::Auto | Width::Span(_) => 0,
    }
}

/// Whole pixels of `free` owed to a shared button of width `w`: `floor(free x span / total)`.
/// Zero for `Px` buttons and when no button has a share (`total == 0`).
fn share_of(free: u32, w: Width, total: u32) -> u32 {
    let share = (u64::from(free) * u64::from(w.shares())).checked_div(u64::from(total)).unwrap_or(0);
    // At most `free`, since a button's span never exceeds the total.
    u32::try_from(share).unwrap_or(free)
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
