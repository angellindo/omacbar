//! Touch slop bridges only the gap between two buttons of the same group. Bar edges and group
//! gaps are dead. Default layer: esc 88..204, group gap 204..232, F1 232..376, gap 376..388,
//! F2 388..532, F12 1939..2082, right edge 2082..2170.
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
    misses(&p, 84);
    misses(&p, 87);
}

#[test]
fn a_touch_in_the_right_edge_padding_hits_nothing() {
    let p = placed(Chord::Default);
    misses(&p, 2082);
    misses(&p, 2085);
}

#[test]
fn a_touch_in_a_group_gap_next_to_the_left_group_hits_nothing() {
    let p = placed(Chord::Default);
    misses(&p, 204);
    misses(&p, 207);
}

#[test]
fn a_touch_in_a_group_gap_next_to_the_right_group_hits_nothing() {
    let p = placed(Chord::Default);
    misses(&p, 228);
    misses(&p, 231);
}

#[test]
fn a_touch_in_a_group_gap_between_two_media_groups_hits_nothing() {
    let p = placed(Chord::Fn);
    misses(&p, 568);
    misses(&p, 655);
}

#[test]
fn a_touch_in_a_gap_inside_a_group_hits_the_nearer_button() {
    let p = placed(Chord::Default);
    hits(&p, 379, "f1");
    hits(&p, 381, "f1");
    hits(&p, 382, "f2");
    hits(&p, 385, "f2");
}

#[test]
fn a_touch_inside_a_button_hits_it_and_far_touches_hit_nothing() {
    let p = placed(Chord::Default);
    hits(&p, 88, "esc");
    hits(&p, 203, "esc");
    hits(&p, 232, "f1");
    hits(&p, 2081, "f12");
    misses(&p, 0);
    misses(&p, 3);
    misses(&p, 218);
}
