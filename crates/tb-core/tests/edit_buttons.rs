//! Adding and removing buttons and groups in a layer (the configurator's toolbar): a new button is
//! a span-1 button labelled `new` with no action and the first free `button-<n>` id; every
//! operation leaves the layer with no empty group.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use tb_core::edit::{add_button, add_group, remove_button};
use tb_core::{Action, Button, ButtonId, ButtonStyle, Chord, Group, Layer, Width, presets};

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

fn id(s: &str) -> ButtonId {
    ButtonId::new(s)
}

fn button(layer: &Layer, s: &str) -> Button {
    layer.buttons().find(|b| b.id.as_str() == s).cloned().unwrap()
}

#[test]
fn adding_after_f3_puts_a_new_button_right_after_it_in_its_group() {
    let mut layer = default_layer();
    let added = add_button(&mut layer, Some(&id("f3")));
    assert_eq!(added, id("button-1"));
    assert_eq!(ids(&layer), groups("esc | f1 f2 f3 button-1 f4 f5 f6 f7 f8 f9 f10 f11 f12"));
}

#[test]
fn a_new_button_is_labelled_new_spans_one_share_and_does_nothing() {
    let mut layer = default_layer();
    add_button(&mut layer, Some(&id("f3")));
    assert_eq!(
        button(&layer, "button-1"),
        Button {
            id: id("button-1"),
            label: "new".to_owned(),
            width: Width::Auto,
            style: ButtonStyle::default(),
            action: Action::None,
        }
    );
}

#[test]
fn adding_with_nothing_selected_ends_the_last_group() {
    let mut layer = default_layer();
    assert_eq!(add_button(&mut layer, None), id("button-1"));
    assert_eq!(ids(&layer), groups("esc | f1 f2 f3 f4 f5 f6 f7 f8 f9 f10 f11 f12 button-1"));
}

#[test]
fn adding_after_an_absent_button_ends_the_last_group() {
    let mut layer = default_layer();
    add_button(&mut layer, Some(&id("nope")));
    assert_eq!(ids(&layer), groups("esc | f1 f2 f3 f4 f5 f6 f7 f8 f9 f10 f11 f12 button-1"));
}

#[test]
fn adding_to_a_layer_without_groups_makes_a_first_group() {
    let mut layer = Layer { chord: Chord::Fn, groups: Vec::new() };
    assert_eq!(add_button(&mut layer, None), id("button-1"));
    assert_eq!(ids(&layer), groups("button-1"));
}

#[test]
fn every_added_button_gets_an_id_no_other_button_has() {
    let mut layer = default_layer();
    let first = add_button(&mut layer, Some(&id("f3")));
    let second = add_button(&mut layer, Some(&id("f3")));
    assert_eq!((first, second), (id("button-1"), id("button-2")));
    assert_eq!(ids(&layer), groups("esc | f1 f2 f3 button-2 button-1 f4 f5 f6 f7 f8 f9 f10 f11 f12"));
}

#[test]
fn a_new_id_takes_the_first_free_number() {
    let mut layer = Layer {
        chord: Chord::Fn,
        groups: vec![Group {
            buttons: vec![
                Button::new("button-1", "a", Action::None),
                Button::new("button-3", "b", Action::None),
            ],
        }],
    };
    assert_eq!(add_button(&mut layer, None), id("button-2"));
    assert_eq!(add_button(&mut layer, None), id("button-4"));
}

#[test]
fn adding_a_group_after_f3_puts_a_one_button_group_after_the_f_key_group() {
    let mut layer = default_layer();
    assert_eq!(add_group(&mut layer, Some(&id("f3"))), id("button-1"));
    assert_eq!(ids(&layer), groups("esc | f1 f2 f3 f4 f5 f6 f7 f8 f9 f10 f11 f12 | button-1"));
    assert_eq!(button(&layer, "button-1").label, "new");
}

#[test]
fn adding_a_group_after_esc_puts_it_between_esc_and_the_f_keys() {
    let mut layer = default_layer();
    add_group(&mut layer, Some(&id("esc")));
    assert_eq!(ids(&layer), groups("esc | button-1 | f1 f2 f3 f4 f5 f6 f7 f8 f9 f10 f11 f12"));
}

#[test]
fn adding_a_group_with_nothing_selected_ends_the_layer() {
    let mut layer = default_layer();
    add_group(&mut layer, None);
    assert_eq!(ids(&layer), groups("esc | f1 f2 f3 f4 f5 f6 f7 f8 f9 f10 f11 f12 | button-1"));
}

#[test]
fn removing_f3_takes_it_out_and_returns_it() {
    let mut layer = default_layer();
    let f3 = button(&layer, "f3");
    assert_eq!(remove_button(&mut layer, &id("f3")), Some(f3));
    assert_eq!(ids(&layer), groups("esc | f1 f2 f4 f5 f6 f7 f8 f9 f10 f11 f12"));
}

#[test]
fn removing_the_only_button_of_a_group_removes_the_group() {
    let mut layer = default_layer();
    assert!(remove_button(&mut layer, &id("esc")).is_some());
    assert_eq!(ids(&layer), groups("f1 f2 f3 f4 f5 f6 f7 f8 f9 f10 f11 f12"));
    assert_eq!(layer.groups.len(), 1, "no empty group is left behind");
}

#[test]
fn removing_an_absent_button_changes_nothing() {
    let mut layer = default_layer();
    assert_eq!(remove_button(&mut layer, &id("nope")), None);
    assert_eq!(layer, default_layer());
}
