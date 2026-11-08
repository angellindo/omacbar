//! Touch slop bridges only the gap between two buttons of the same group. Bar edges and group
//! gaps are dead. Default layer: esc 8..128, group gap 128..152, F1 152..313, gap 313..321,
//! F2 321..482, F12 2002..2162, right edge 2162..2170.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use tb_core::hit::hit;
use tb_core::{BarSize, Chord, Placed, SpacingRules, place, presets};

const RULES: SpacingRules = SpacingRules::STANDARD;

fn placed(chord: Chord) -> Vec<Placed> {
    place(presets::default_layout().layer(chord).unwrap(), BarSize::MACBOOK_PRO, &RULES).unwrap()
}

fn touch(placed: &[Placed], x: u16) -> Option<String> {
    hit(placed, x, &RULES).map(ToString::to_string)
}

fn hits(placed: &[Placed], x: u16, id: &str) {
    assert_eq!(touch(placed, x).as_deref(), Some(id), "x {x}");
}

fn misses(placed: &[Placed], x: u16) {
    assert_eq!(touch(placed, x), None, "x {x}");
}

#[test]
fn a_touch_in_the_left_edge_padding_hits_nothing() {
    let p = placed(Chord::Default);
    misses(&p, 4);
    misses(&p, 7);
}

#[test]
fn a_touch_in_the_right_edge_padding_hits_nothing() {
    let p = placed(Chord::Default);
    misses(&p, 2162);
    misses(&p, 2165);
}

#[test]
fn a_touch_in_a_group_gap_next_to_the_left_group_hits_nothing() {
    let p = placed(Chord::Default);
    misses(&p, 128);
    misses(&p, 131);
}

#[test]
fn a_touch_in_a_group_gap_next_to_the_right_group_hits_nothing() {
    let p = placed(Chord::Default);
    misses(&p, 148);
    misses(&p, 151);
}

#[test]
fn a_touch_in_a_group_gap_between_two_shared_groups_hits_nothing() {
    let p = placed(Chord::Fn);
    misses(&p, 643);
    misses(&p, 664);
}

#[test]
fn a_touch_in_a_gap_inside_a_group_hits_the_nearer_button() {
    let p = placed(Chord::Default);
    hits(&p, 315, "f1");
    hits(&p, 316, "f1");
    hits(&p, 317, "f2");
    hits(&p, 318, "f2");
}

#[test]
fn a_touch_inside_a_button_hits_it_and_far_touches_hit_nothing() {
    let p = placed(Chord::Default);
    hits(&p, 8, "esc");
    hits(&p, 127, "esc");
    hits(&p, 152, "f1");
    hits(&p, 2161, "f12");
    misses(&p, 0);
    misses(&p, 3);
    misses(&p, 140);
}
