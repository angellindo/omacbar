use super::*;
use crate::geometry::Rect;

fn at(id: &str, x: u16, width: u16) -> Placed {
    in_group(id, x, width, 0)
}

fn in_group(id: &str, x: u16, width: u16, group: usize) -> Placed {
    Placed { id: ButtonId::new(id), rect: Rect { x, y: 8, width, height: 44 }, group }
}

#[test]
fn a_touch_inside_a_button_hits_it() {
    let placed = [at("a", 8, 100), at("b", 116, 100)];
    assert_eq!(hit(&placed, 50, &SpacingRules::STANDARD).map(ButtonId::as_str), Some("a"));
    assert_eq!(hit(&placed, 116, &SpacingRules::STANDARD).map(ButtonId::as_str), Some("b"));
}

#[test]
fn a_touch_in_a_gap_goes_to_the_nearest_button() {
    let placed = [at("a", 8, 100), at("b", 116, 100)];
    assert_eq!(hit(&placed, 109, &SpacingRules::STANDARD).map(ButtonId::as_str), Some("a"));
    assert_eq!(hit(&placed, 114, &SpacingRules::STANDARD).map(ButtonId::as_str), Some("b"));
}

#[test]
fn a_touch_far_from_any_button_hits_nothing() {
    let placed = [at("a", 8, 100), at("b", 140, 100)];
    assert_eq!(hit(&placed, 124, &SpacingRules::STANDARD), None);
    assert_eq!(hit(&placed, 0, &SpacingRules::STANDARD), None);
}

#[test]
fn slop_never_crosses_into_another_group_even_when_the_gap_is_narrow() {
    let placed = [in_group("a", 8, 100, 0), in_group("b", 116, 100, 1)];
    assert_eq!(hit(&placed, 109, &SpacingRules::STANDARD), None);
    assert_eq!(hit(&placed, 114, &SpacingRules::STANDARD), None);
}
