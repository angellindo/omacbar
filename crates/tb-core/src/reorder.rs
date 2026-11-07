//! Reordering buttons within and across the groups of one layer.

use thiserror::Error;

use crate::model::Layer;

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
