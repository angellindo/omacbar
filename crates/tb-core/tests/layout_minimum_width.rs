//! The 64 px minimum is checked per button on the widths the engine allocates, not per share.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use tb_core::{
    Action, BarSize, Button, ButtonId, Chord, Group, Layer, LayoutError, Placed, SpacingRules, Width, place,
};

const BAR: BarSize = BarSize::MACBOOK_PRO;
const RULES: SpacingRules = SpacingRules::STANDARD;

fn button(id: &str, width: Width) -> Button {
    Button::new(id, id, Action::None).with_width(width)
}

fn layer(groups: Vec<Vec<Button>>) -> Layer {
    Layer {
        group_gap: None,
        chord: Chord::Default,
        groups: groups.into_iter().map(|buttons| Group { buttons }).collect(),
    }
}

fn esc() -> Button {
    button("esc", Width::Px(120))
}

fn span_fours(n: usize) -> Vec<Button> {
    (0..n).map(|i| button(&format!("s{i}"), Width::Span(4))).collect()
}

fn spans(placed: &[Placed]) -> Vec<(String, u16, u16)> {
    placed.iter().map(|p| (p.id.to_string(), p.rect.x, p.rect.width)).collect()
}

#[test]
fn ten_span_four_buttons_after_esc_lay_out_at_about_168_px_each() {
    let placed = place(&layer(vec![vec![esc()], span_fours(10)]), BAR, &RULES).unwrap();
    let mut want = vec![("esc".to_owned(), 88, 120)];
    for i in 0..8u16 {
        want.push((format!("s{i}"), 296 + 180 * i, 168));
    }
    want.push(("s8".to_owned(), 1736, 167));
    want.push(("s9".to_owned(), 1915, 167));
    assert_eq!(spans(&placed), want);
    assert_eq!(placed.last().unwrap().rect.right(), 2082);
}

#[test]
fn an_auto_button_starved_by_wide_spans_is_too_narrow_by_name() {
    let mut shared = span_fours(8);
    shared.push(button("a", Width::Auto));
    let err = place(&layer(vec![vec![esc()], shared]), BAR, &RULES).unwrap_err();
    assert_eq!(err, LayoutError::TooNarrow { id: ButtonId::new("a"), width: 51, min: 64 });
}

#[test]
fn twenty_six_auto_buttons_still_fit_the_bar() {
    let keys: Vec<Button> = (0..26).map(|i| button(&format!("k{i}"), Width::Auto)).collect();
    let placed = place(&layer(vec![keys]), BAR, &RULES).unwrap();
    let got = spans(&placed);
    assert_eq!(got.len(), 26);
    assert_eq!(got[0], ("k0".to_owned(), 88, 66));
    assert_eq!(got[1], ("k1".to_owned(), 166, 66));
    assert_eq!(got[3], ("k3".to_owned(), 322, 66));
    assert_eq!(got[25], ("k25".to_owned(), 2017, 65));
    assert!(got[4..].iter().all(|(_, _, w)| *w == 65));
    assert_eq!(placed.last().unwrap().rect.right(), 2082);
}

#[test]
fn twenty_seven_auto_buttons_overflow_with_the_needed_width() {
    let keys: Vec<Button> = (0..27).map(|i| button(&format!("k{i}"), Width::Auto)).collect();
    let err = place(&layer(vec![keys]), BAR, &RULES).unwrap_err();
    assert_eq!(err, LayoutError::Overflow { needed: 2040, available: 1994 });
}

#[test]
fn a_fixed_button_under_the_minimum_is_too_narrow() {
    let err = place(&layer(vec![vec![button("tiny", Width::Px(40))]]), BAR, &RULES).unwrap_err();
    assert_eq!(err, LayoutError::TooNarrow { id: ButtonId::new("tiny"), width: 40, min: 64 });
}
