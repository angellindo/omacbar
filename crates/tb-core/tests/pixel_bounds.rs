//! Pixel arithmetic at the u16 limit neither panics (debug builds) nor wraps.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use tb_core::hit::hit;
use tb_core::{
    Action, BarSize, Button, Chord, Group, Layer, LayoutError, Placed, Rect, SpacingRules, place, presets,
};

const BAR: BarSize = BarSize::MACBOOK_PRO;
const RULES: SpacingRules = SpacingRules::STANDARD;

fn default_layer() -> Layer {
    presets::default_layout().layer(Chord::Default).unwrap().clone()
}

fn spans(placed: &[Placed]) -> Vec<(&str, u16, u16)> {
    placed.iter().map(|p| (p.id.as_str(), p.rect.x, p.rect.width)).collect()
}

#[test]
fn distance_to_the_last_pixel_column_is_exact() {
    let r = Rect { x: 8, y: 8, width: 100, height: 44 };
    assert_eq!(r.distance_x(u16::MAX), 65428);
}

#[test]
fn a_rect_reaching_past_the_last_pixel_column_keeps_its_true_extent() {
    let r = Rect { x: 65530, y: 0, width: 100, height: 1 };
    assert_eq!(r.right(), u16::MAX);
    assert!(r.contains_x(u16::MAX));
    assert_eq!(r.distance_x(u16::MAX), 0);
    assert_eq!(r.distance_x(0), 65530);
}

#[test]
fn a_distance_that_does_not_fit_saturates() {
    let r = Rect { x: 0, y: 0, width: 0, height: 0 };
    assert_eq!(r.distance_x(u16::MAX), u16::MAX);
}

#[test]
fn a_touch_at_the_last_pixel_column_hits_nothing() {
    let placed = place(&default_layer(), BAR, &RULES).unwrap();
    assert_eq!(hit(&placed, u16::MAX, &RULES), None);
}

#[test]
fn edge_padding_at_the_limit_leaves_no_room() {
    let rules = SpacingRules { edge_padding: u16::MAX, ..RULES };
    let l = Layer {
        group_gap: None,
        chord: Chord::Default,
        groups: vec![Group { buttons: vec![Button::new("a", "a", Action::None)] }],
    };
    assert_eq!(place(&l, BAR, &rules), Err(LayoutError::Overflow { needed: 64, available: 0 }));
}

#[test]
fn a_vertical_inset_at_the_limit_leaves_buttons_zero_tall() {
    let huge = SpacingRules { vertical_inset: u16::MAX, ..RULES };
    assert_eq!(huge.button_height(60), 0);
    assert_eq!(SpacingRules { vertical_inset: 30_000, ..RULES }.button_height(60), 0);
    let placed = place(&default_layer(), BAR, &huge).unwrap();
    assert_eq!(placed.len(), 13);
    assert!(placed.iter().all(|p| p.rect.height == 0));
}

#[test]
fn the_widest_bar_lays_out_the_default_layer() {
    let wide = BarSize { width: u16::MAX, height: 60 };
    let placed = place(&default_layer(), wide, &RULES).unwrap();
    let mut want = vec![("esc", 88, 116)];
    let ids = ["f1", "f2", "f3", "f4", "f5", "f6", "f7", "f8", "f9", "f10", "f11", "f12"];
    let mut x = 232;
    for (i, id) in ids.into_iter().enumerate() {
        let w = if i < 7 { 5424 } else { 5423 };
        want.push((id, x, w));
        x += w + 12;
    }
    assert_eq!(spans(&placed), want);
    assert_eq!(spans(&placed)[7], ("f7", 32848, 5424));
    assert_eq!(spans(&placed)[8], ("f8", 38284, 5423));
    assert_eq!(spans(&placed)[12], ("f12", 60024, 5423));
    assert_eq!(placed.last().unwrap().rect.right(), 65447);
}
