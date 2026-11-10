//! `place` totals a layer's fixed (`Px`) widths without overflowing the integer type: a total too
//! large to count is an `Overflow` that needs `u32::MAX` px, never a panic or a wrapped total.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use tb_core::{Action, BarSize, Button, Chord, Group, Layer, LayoutError, SpacingRules, Width, place};

const BAR: BarSize = BarSize::MACBOOK_PRO;
const RULES: SpacingRules = SpacingRules::STANDARD;

fn fixed(id: &str, px: u16) -> Button {
    Button::new(id, id, Action::None).with_width(Width::Px(px))
}

fn one_group(buttons: Vec<Button>) -> Layer {
    Layer { group_gap: None, chord: Chord::Default, groups: vec![Group { buttons }] }
}

fn widest_buttons(n: usize) -> Vec<Button> {
    (0..n).map(|i| fixed(&format!("w{i}"), u16::MAX)).collect()
}

#[test]
fn fixed_widths_summing_past_the_integer_type_overflow_with_the_largest_need() {
    // 65,538 x 65,535 px is past u32::MAX.
    let err = place(&one_group(widest_buttons(65_538)), BAR, &RULES).unwrap_err();
    assert_eq!(err, LayoutError::Overflow { needed: u32::MAX, available: 1994 });
}

#[test]
fn fixed_widths_that_would_wrap_to_a_tiny_total_still_overflow() {
    // 65,537 x 65,535 px is exactly u32::MAX; 2 px more wraps a plain sum around to 1 px,
    // which would fit the bar when there are no gaps.
    let mut buttons = widest_buttons(65_537);
    buttons.push(fixed("tail", 2));
    let rules = SpacingRules { gap: 0, group_gap: 0, ..RULES };
    let err = place(&one_group(buttons), BAR, &rules).unwrap_err();
    assert_eq!(err, LayoutError::Overflow { needed: u32::MAX, available: 1994 });
}

#[test]
fn fixed_widths_a_little_past_the_bar_overflow_with_their_exact_need() {
    // 1,100 + 12 (gap) + 1,100 = 2,212 px on a 1,994 px usable bar.
    let err = place(&one_group(vec![fixed("a", 1100), fixed("b", 1100)]), BAR, &RULES).unwrap_err();
    assert_eq!(err, LayoutError::Overflow { needed: 2212, available: 1994 });
}

#[test]
fn a_fixed_width_equal_to_the_usable_bar_fills_it() {
    let placed = place(&one_group(vec![fixed("all", 1994)]), BAR, &RULES).unwrap();
    assert_eq!(placed.len(), 1);
    assert_eq!((placed[0].rect.x, placed[0].rect.width), (88, 1994));
    assert_eq!(placed[0].rect.right(), 2082);
}
