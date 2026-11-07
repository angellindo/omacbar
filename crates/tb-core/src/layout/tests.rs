use super::*;
use crate::chord::Chord;
use crate::model::{Action, Button, Group, Width};
use crate::presets;

const BAR: BarSize = BarSize::MACBOOK_PRO;
const RULES: SpacingRules = SpacingRules::STANDARD;

fn layer(groups: Vec<Vec<Button>>) -> Layer {
    Layer { chord: Chord::Default, groups: groups.into_iter().map(|buttons| Group { buttons }).collect() }
}

fn b(id: &str) -> Button {
    Button::new(id, id, Action::None)
}

#[test]
fn the_default_f_key_layer_fills_the_bar_edge_to_edge() {
    let placed = place(presets::default_layout().layer(Chord::Default).unwrap(), BAR, &RULES).unwrap();
    assert_eq!(placed.first().unwrap().rect.x, RULES.edge_padding);
    assert_eq!(placed.last().unwrap().rect.right(), BAR.width - RULES.edge_padding);
}

#[test]
fn buttons_never_overlap_and_respect_the_gap() {
    let placed = place(presets::default_layout().layer(Chord::Default).unwrap(), BAR, &RULES).unwrap();
    for pair in placed.windows(2) {
        let space = pair[1].rect.x - pair[0].rect.right();
        let want = if pair[0].group == pair[1].group { RULES.gap } else { RULES.group_gap };
        assert_eq!(space, want, "{} -> {}", pair[0].id, pair[1].id);
    }
}

#[test]
fn every_button_is_vertically_inset() {
    let placed = place(&layer(vec![vec![b("a"), b("b")]]), BAR, &RULES).unwrap();
    for p in &placed {
        assert_eq!(p.rect.y, RULES.vertical_inset);
        assert_eq!(p.rect.height, BAR.height - 2 * RULES.vertical_inset);
    }
}

#[test]
fn a_span_two_button_is_twice_a_span_one_plus_at_most_one_pixel() {
    let placed =
        place(&layer(vec![vec![b("a"), b("wide").with_width(Width::Span(2)), b("c")]]), BAR, &RULES).unwrap();
    let (one, two) = (placed[2].rect.width, placed[1].rect.width);
    assert!(two.abs_diff(2 * one) <= 1, "span 2 = {two}, span 1 = {one}");
}

#[test]
fn fixed_width_buttons_keep_their_exact_width() {
    let placed =
        place(&layer(vec![vec![b("a"), b("fixed").with_width(Width::Px(120))]]), BAR, &RULES).unwrap();
    assert_eq!(placed[1].rect.width, 120);
    assert_eq!(placed[1].rect.right(), BAR.width - RULES.edge_padding);
}

#[test]
fn only_fixed_buttons_are_left_aligned() {
    let placed = place(&layer(vec![vec![b("a").with_width(Width::Px(100))]]), BAR, &RULES).unwrap();
    assert_eq!(placed[0].rect, Rect { x: 8, y: 8, width: 100, height: 44 });
}

#[test]
fn too_many_buttons_overflow_with_the_needed_width() {
    let many = (0..40).map(|i| b(&format!("k{i}"))).collect();
    let err = place(&layer(vec![many]), BAR, &RULES).unwrap_err();
    assert!(matches!(err, LayoutError::Overflow { .. }), "{err}");
}

#[test]
fn a_fixed_button_below_the_minimum_is_rejected() {
    let err = place(&layer(vec![vec![b("tiny").with_width(Width::Px(40))]]), BAR, &RULES).unwrap_err();
    assert_eq!(err, LayoutError::TooNarrow { id: ButtonId::new("tiny"), width: 40, min: 64 });
}

#[test]
fn empty_groups_add_no_gap() {
    let with_empty = place(&layer(vec![vec![b("a")], vec![], vec![b("b")]]), BAR, &RULES).unwrap();
    let without = place(&layer(vec![vec![b("a")], vec![b("b")]]), BAR, &RULES).unwrap();
    assert_eq!(
        with_empty.iter().map(|p| p.rect).collect::<Vec<_>>(),
        without.iter().map(|p| p.rect).collect::<Vec<_>>()
    );
}

#[test]
fn an_empty_layer_places_nothing() {
    assert_eq!(place(&layer(vec![]), BAR, &RULES).unwrap(), vec![]);
}
