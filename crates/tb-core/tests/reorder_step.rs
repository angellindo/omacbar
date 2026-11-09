//! Keyboard reorder (Alt+Left / Alt+Right in the configurator): `step_button` moves a button one
//! step toward the bar's left edge (the ESC side) or right edge, crossing into the adjacent group
//! at a group's edge, and never leaves an empty group behind.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use tb_core::reorder::{Toward, step_button};
use tb_core::{ButtonId, Chord, Layer, presets};

/// The default layer: `esc | f1 … f12`.
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

fn step(layer: &mut Layer, id: &str, toward: Toward) -> bool {
    step_button(layer, &ButtonId::new(id), toward)
}

#[test]
fn stepping_f3_left_twice_puts_it_first_in_the_f_key_group() {
    let mut layer = default_layer();
    assert!(step(&mut layer, "f3", Toward::Left));
    assert_eq!(ids(&layer), groups("esc | f1 f3 f2 f4 f5 f6 f7 f8 f9 f10 f11 f12"));
    assert!(step(&mut layer, "f3", Toward::Left));
    assert_eq!(ids(&layer), groups("esc | f3 f1 f2 f4 f5 f6 f7 f8 f9 f10 f11 f12"));
}

#[test]
fn stepping_left_from_the_first_slot_of_a_group_ends_the_previous_group() {
    let mut layer = default_layer();
    step(&mut layer, "f3", Toward::Left);
    step(&mut layer, "f3", Toward::Left);
    assert!(step(&mut layer, "f3", Toward::Left));
    assert_eq!(ids(&layer), groups("esc f3 | f1 f2 f4 f5 f6 f7 f8 f9 f10 f11 f12"));
}

#[test]
fn stepping_right_from_the_last_slot_of_a_group_starts_the_next_group() {
    let mut layer = default_layer();
    for _ in 0..3 {
        step(&mut layer, "f3", Toward::Left);
    }
    assert!(step(&mut layer, "f3", Toward::Right));
    assert_eq!(ids(&layer), groups("esc | f3 f1 f2 f4 f5 f6 f7 f8 f9 f10 f11 f12"));
    assert!(step(&mut layer, "f3", Toward::Right));
    assert_eq!(ids(&layer), groups("esc | f1 f3 f2 f4 f5 f6 f7 f8 f9 f10 f11 f12"));
}

#[test]
fn stepping_the_first_button_left_changes_nothing() {
    let mut layer = default_layer();
    assert!(!step(&mut layer, "esc", Toward::Left));
    assert_eq!(layer, default_layer());
}

#[test]
fn stepping_the_last_button_right_changes_nothing() {
    let mut layer = default_layer();
    assert!(!step(&mut layer, "f12", Toward::Right));
    assert_eq!(layer, default_layer());
}

#[test]
fn stepping_an_unknown_button_changes_nothing() {
    let mut layer = default_layer();
    assert!(!step(&mut layer, "nope", Toward::Left));
    assert!(!step(&mut layer, "nope", Toward::Right));
    assert_eq!(layer, default_layer());
}

#[test]
fn stepping_the_only_button_of_a_group_out_removes_the_group() {
    let mut layer = default_layer();
    assert!(step(&mut layer, "esc", Toward::Right));
    assert_eq!(ids(&layer), groups("esc f1 f2 f3 f4 f5 f6 f7 f8 f9 f10 f11 f12"));
    assert_eq!(layer.groups.len(), 1, "no empty group is left behind");
}
