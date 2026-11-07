//! Touch hit-testing along the bar's x axis.

use crate::layout::Placed;
use crate::model::ButtonId;
use crate::spacing::SpacingRules;

/// The button under a touch at `x`, per the touch slop row of `docs/design/button-standard.md`.
///
/// A touch inside a button hits it. A touch in the gap between two buttons of the same group hits
/// the nearer one when within `touch_slop()` (ties go left). Bar edges and group gaps are dead, so
/// a touch there hits nothing. `placed` must be `place` output: ordered by x, groups contiguous.
pub fn hit<'a>(placed: &'a [Placed], x: u16, rules: &SpacingRules) -> Option<&'a ButtonId> {
    let next = placed.iter().position(|p| x < p.rect.x || p.rect.contains_x(x))?;
    let right = &placed[next];
    if right.rect.contains_x(x) {
        return Some(&right.id);
    }
    let left = &placed[next.checked_sub(1)?];
    if left.group != right.group {
        return None;
    }
    let (left_d, right_d) = (left.rect.distance_x(x), right.rect.distance_x(x));
    let (d, nearer) = if left_d <= right_d { (left_d, left) } else { (right_d, right) };
    (d <= rules.touch_slop()).then_some(&nearer.id)
}

#[cfg(test)]
mod tests;
