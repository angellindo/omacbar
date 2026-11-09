//! Drag to reorder (the configurator preview): `drop_button` moves a button to where a drop at bar
//! x puts it: before the button under x when x is left of that button's centre, after it
//! otherwise, in that button's group; a drop in a gap counts for the nearest button. No empty
//! group is left behind.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use tb_core::reorder::drop_button;
use tb_core::{BarSize, ButtonId, Chord, Layer, Rect, SpacingRules, Width, place, presets};

fn default_layer() -> Layer {
    presets::default_layout().layer(Chord::Default).unwrap().clone()
}

/// Button ids per group, groups split by `|`: `"esc | f1 f2"`.
fn groups(spec: &str) -> Vec<Vec<String>> {
    spec.split('|').map(|g| g.split_whitespace().map(str::to_owned).collect()).collect()
}

fn ids(layer: &Layer) -> Vec<Vec<String>> {
    layer.groups.iter().map(|g| g.buttons.iter().map(|b| b.id.to_string()).collect()).collect()
}

/// Where `id` sits on the bar in the default layer.
fn rect_of(id: &str) -> Rect {
    place(&default_layer(), BarSize::MACBOOK_PRO, &SpacingRules::STANDARD)
        .unwrap()
        .into_iter()
        .find(|p| p.id.as_str() == id)
        .unwrap()
        .rect
}

/// A bar x a quarter of the way into `id` (its left half).
fn left_quarter(id: &str) -> u16 {
    let r = rect_of(id);
    r.x + r.width / 4
}

/// A bar x three quarters of the way into `id` (its right half).
fn right_quarter(id: &str) -> u16 {
    let r = rect_of(id);
    r.x + r.width * 3 / 4
}

fn drop_at(layer: &mut Layer, id: &str, x: u16) -> bool {
    drop_button(layer, &ButtonId::new(id), x, BarSize::MACBOOK_PRO, &SpacingRules::STANDARD).unwrap()
}

#[test]
fn dropping_f3_on_the_left_half_of_f1_puts_f3_before_f1() {
    let mut layer = default_layer();
    assert!(drop_at(&mut layer, "f3", left_quarter("f1")));
    assert_eq!(ids(&layer), groups("esc | f3 f1 f2 f4 f5 f6 f7 f8 f9 f10 f11 f12"));
}

#[test]
fn dropping_f3_on_the_right_half_of_f1_puts_f3_after_f1() {
    let mut layer = default_layer();
    assert!(drop_at(&mut layer, "f3", right_quarter("f1")));
    assert_eq!(ids(&layer), groups("esc | f1 f3 f2 f4 f5 f6 f7 f8 f9 f10 f11 f12"));
}

#[test]
fn dropping_f1_on_the_right_half_of_f5_puts_f1_after_f5() {
    let mut layer = default_layer();
    assert!(drop_at(&mut layer, "f1", right_quarter("f5")));
    assert_eq!(ids(&layer), groups("esc | f2 f3 f4 f5 f1 f6 f7 f8 f9 f10 f11 f12"));
}

#[test]
fn dropping_a_button_on_itself_changes_nothing() {
    let mut layer = default_layer();
    assert!(!drop_at(&mut layer, "f3", left_quarter("f3")));
    assert!(!drop_at(&mut layer, "f3", right_quarter("f3")));
    assert_eq!(layer, default_layer());
}

#[test]
fn dropping_on_the_right_half_of_esc_moves_the_button_into_the_esc_group() {
    let mut layer = default_layer();
    assert!(drop_at(&mut layer, "f1", right_quarter("esc")));
    assert_eq!(ids(&layer), groups("esc f1 | f2 f3 f4 f5 f6 f7 f8 f9 f10 f11 f12"));
}

#[test]
fn a_drop_in_the_group_gap_counts_for_the_nearest_button() {
    let mut layer = default_layer();
    // Two px left of F1: inside the 24 px group gap, nearer F1 than ESC.
    let x = rect_of("f1").x - 2;
    assert!(drop_at(&mut layer, "f3", x));
    assert_eq!(ids(&layer), groups("esc | f3 f1 f2 f4 f5 f6 f7 f8 f9 f10 f11 f12"));
}

#[test]
fn dragging_the_only_button_of_a_group_out_removes_the_group() {
    let mut layer = default_layer();
    assert!(drop_at(&mut layer, "esc", left_quarter("f2")));
    assert_eq!(ids(&layer), groups("f1 esc f2 f3 f4 f5 f6 f7 f8 f9 f10 f11 f12"));
    assert_eq!(layer.groups.len(), 1, "no empty group is left behind");
}

#[test]
fn dropping_an_unknown_button_changes_nothing() {
    let mut layer = default_layer();
    assert!(!drop_at(&mut layer, "nope", left_quarter("f1")));
    assert_eq!(layer, default_layer());
}

#[test]
fn a_drop_on_a_layer_that_does_not_fit_the_bar_is_an_error_and_changes_nothing() {
    let mut layer = default_layer();
    layer.groups[1].buttons[0].width = Width::Px(3000);
    let before = layer.clone();
    let result =
        drop_button(&mut layer, &ButtonId::new("f3"), 200, BarSize::MACBOOK_PRO, &SpacingRules::STANDARD);
    assert!(result.is_err(), "a layer wider than the bar has no placements to drop on: {result:?}");
    assert_eq!(layer, before);
}
