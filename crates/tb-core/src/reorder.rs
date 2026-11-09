//! Reordering buttons within and across the groups of one layer.

use thiserror::Error;

use crate::geometry::BarSize;
use crate::layout::{LayoutError, place};
use crate::model::{ButtonId, Layer};
use crate::spacing::SpacingRules;

/// A button slot: group index, then index inside the group.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Slot {
    pub group: usize,
    pub index: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum ReorderError {
    #[error("no button at group {}, index {}", .0.group, .0.index)]
    NoSuchButton(Slot),
    #[error("cannot insert at group {}, index {}", .0.group, .0.index)]
    BadTarget(Slot),
}

/// Moves the button at `from` so it ends up at `to` (indices as seen after removal).
pub fn move_button(layer: &mut Layer, from: Slot, to: Slot) -> Result<(), ReorderError> {
    let source = layer.groups.get(from.group).filter(|g| from.index < g.buttons.len());
    if source.is_none() {
        return Err(ReorderError::NoSuchButton(from));
    }
    let target_len =
        layer.groups.get(to.group).map(|g| g.buttons.len()).ok_or(ReorderError::BadTarget(to))?;
    let target_len_after = if from.group == to.group { target_len - 1 } else { target_len };
    if to.index > target_len_after {
        return Err(ReorderError::BadTarget(to));
    }
    let button = layer.groups[from.group].buttons.remove(from.index);
    layer.groups[to.group].buttons.insert(to.index, button);
    Ok(())
}

/// Which way a keyboard reorder moves a button along the bar.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Toward {
    /// Toward the bar's left edge, the ESC side.
    Left,
    /// Toward the bar's right edge.
    Right,
}

/// Where button `id` sits in `layer` (the first match when ids repeat).
pub(crate) fn slot_of(layer: &Layer, id: &ButtonId) -> Option<Slot> {
    layer
        .groups
        .iter()
        .enumerate()
        .find_map(|(group, g)| g.buttons.iter().position(|b| &b.id == id).map(|index| Slot { group, index }))
}

/// Moves button `id` one step toward the bar's left (ESC side) or right edge: past its neighbour
/// in its group, or from the group's edge to the near end of the adjacent group. The layer is left
/// with no empty group. False (nothing changed) at the layer's ends or when `id` is not in `layer`.
pub fn step_button(layer: &mut Layer, id: &ButtonId, toward: Toward) -> bool {
    let Some(from) = slot_of(layer, id) else { return false };
    let len = layer.groups[from.group].buttons.len();
    let to = match toward {
        Toward::Left if from.index > 0 => Slot { index: from.index - 1, ..from },
        Toward::Left if from.group > 0 => {
            let group = from.group - 1;
            Slot { group, index: layer.groups[group].buttons.len() }
        }
        Toward::Right if from.index + 1 < len => Slot { index: from.index + 1, ..from },
        Toward::Right if from.group + 1 < layer.groups.len() => Slot { group: from.group + 1, index: 0 },
        Toward::Left | Toward::Right => return false,
    };
    if move_button(layer, from, to).is_err() {
        return false;
    }
    drop_if_empty(layer, from.group);
    true
}

/// Moves button `id` to where a drop at bar x `x` puts it: before the button under x (else the
/// nearest, ties left) when x is left of that button's centre, after it otherwise, in its group.
/// The layer is left with no empty group. `Ok(false)` when nothing changes (dropped on itself,
/// unknown id); `Err` when the layer does not fit the bar (no placements to drop on).
pub fn drop_button(
    layer: &mut Layer,
    id: &ButtonId,
    x: u16,
    size: BarSize,
    rules: &SpacingRules,
) -> Result<bool, LayoutError> {
    let placed = place(layer, size, rules)?;
    let Some(from) = slot_of(layer, id) else { return Ok(false) };
    let target = placed.iter().find(|p| p.rect.contains_x(x)).or_else(|| {
        // `min_by_key` keeps the first of equals: placements run left to right, so ties go left.
        placed.iter().min_by_key(|p| p.rect.distance_x(x))
    });
    let Some(target) = target.filter(|t| &t.id != id) else { return Ok(false) };
    let before_target = u32::from(x) * 2 < u32::from(target.rect.x) * 2 + u32::from(target.rect.width);
    let old = layer.clone();
    let button = layer.groups[from.group].buttons.remove(from.index);
    let Some(at) = slot_of(layer, &target.id) else {
        *layer = old;
        return Ok(false);
    };
    let index = if before_target { at.index } else { at.index + 1 };
    layer.groups[at.group].buttons.insert(index, button);
    drop_if_empty(layer, from.group);
    Ok(*layer != old)
}

/// Removes group `group` when it holds no button, so no editing step leaves an empty group.
pub(crate) fn drop_if_empty(layer: &mut Layer, group: usize) {
    if layer.groups.get(group).is_some_and(|g| g.buttons.is_empty()) {
        layer.groups.remove(group);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::chord::Chord;
    use crate::model::{Action, Button, Group};

    fn ids(layer: &Layer) -> Vec<Vec<&str>> {
        layer.groups.iter().map(|g| g.buttons.iter().map(|b| b.id.as_str()).collect()).collect()
    }

    fn layer() -> Layer {
        let g =
            |ids: &[&str]| Group { buttons: ids.iter().map(|i| Button::new(i, i, Action::None)).collect() };
        Layer { chord: Chord::Default, groups: vec![g(&["a", "b", "c"]), g(&["x", "y"])] }
    }

    #[test]
    fn moves_a_button_forward_within_a_group() {
        let mut l = layer();
        move_button(&mut l, Slot { group: 0, index: 0 }, Slot { group: 0, index: 2 }).unwrap();
        assert_eq!(ids(&l), vec![vec!["b", "c", "a"], vec!["x", "y"]]);
    }

    #[test]
    fn moves_a_button_to_another_group() {
        let mut l = layer();
        move_button(&mut l, Slot { group: 0, index: 1 }, Slot { group: 1, index: 0 }).unwrap();
        assert_eq!(ids(&l), vec![vec!["a", "c"], vec!["b", "x", "y"]]);
    }

    #[test]
    fn rejects_a_missing_source_and_an_out_of_range_target() {
        let mut l = layer();
        let bad_from = Slot { group: 0, index: 9 };
        assert_eq!(
            move_button(&mut l, bad_from, Slot { group: 0, index: 0 }),
            Err(ReorderError::NoSuchButton(bad_from))
        );
        let bad_to = Slot { group: 1, index: 5 };
        assert_eq!(
            move_button(&mut l, Slot { group: 0, index: 0 }, bad_to),
            Err(ReorderError::BadTarget(bad_to))
        );
        assert_eq!(l, layer(), "a rejected move changes nothing");
    }
}
