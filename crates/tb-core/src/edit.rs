//! Adding and removing buttons and groups in a layer (the configurator's toolbar). Every operation
//! leaves the layer with no empty group.

use crate::model::{Action, Button, ButtonId, Group, Layer};
use crate::reorder::{drop_if_empty, slot_of};

/// Adds a span-1 button labelled `new` with no action right after `after` in its group, or at the
/// end of the last group when `after` is `None` or absent (a first group when the layer has none).
/// Its id is the first free `button-<n>`, n from 1. Returns the id.
pub fn add_button(layer: &mut Layer, after: Option<&ButtonId>) -> ButtonId {
    let id = free_id(layer);
    let button = new_button(&id);
    match after.and_then(|a| slot_of(layer, a)) {
        Some(at) => layer.groups[at.group].buttons.insert(at.index + 1, button),
        None => match layer.groups.last_mut() {
            Some(last) => last.buttons.push(button),
            None => layer.groups.push(Group { buttons: vec![button] }),
        },
    }
    id
}

/// Adds a group holding one new button (as `add_button`) right after the group of `after`, or at
/// the end. Returns the new button's id.
pub fn add_group(layer: &mut Layer, after: Option<&ButtonId>) -> ButtonId {
    let id = free_id(layer);
    let at = after.and_then(|a| slot_of(layer, a)).map_or(layer.groups.len(), |s| s.group + 1);
    layer.groups.insert(at, Group { buttons: vec![new_button(&id)] });
    id
}

/// Removes button `id`; a group left empty is removed. The removed button, or `None` if absent.
pub fn remove_button(layer: &mut Layer, id: &ButtonId) -> Option<Button> {
    let at = slot_of(layer, id)?;
    let button = layer.groups[at.group].buttons.remove(at.index);
    drop_if_empty(layer, at.group);
    Some(button)
}

fn new_button(id: &ButtonId) -> Button {
    Button::new(id.as_str(), "new", Action::None)
}

/// The first `button-<n>` (n from 1) no button of `layer` has.
fn free_id(layer: &Layer) -> ButtonId {
    let mut n = 1_u32;
    loop {
        let id = ButtonId::new(format!("button-{n}"));
        if !layer.buttons().any(|b| b.id == id) {
            return id;
        }
        n += 1;
    }
}
