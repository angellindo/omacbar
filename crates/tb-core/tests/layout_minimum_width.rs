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
    Layer { chord: Chord::Default, groups: groups.into_iter().map(|buttons| Group { buttons }).collect() }
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
fn ten_span_four_buttons_after_esc_lay_out_at_about_193_px_each() {
    let placed = place(&layer(vec![vec![esc()], span_fours(10)]), BAR, &RULES).unwrap();
    let mut want = vec![("esc".to_owned(), 8, 120)];
    for i in 0..8u16 {
        want.push((format!("s{i}"), 152 + 202 * i, 194));
    }
    want.push(("s8".to_owned(), 1768, 193));
    want.push(("s9".to_owned(), 1969, 193));
    assert_eq!(spans(&placed), want);
    assert_eq!(placed.last().unwrap().rect.right(), 2162);
}

#[test]
fn an_auto_button_starved_by_wide_spans_is_too_narrow_by_name() {
    let mut shared = span_fours(8);
    shared.push(button("a", Width::Auto));
    let err = place(&layer(vec![vec![esc()], shared]), BAR, &RULES).unwrap_err();
    assert_eq!(err, LayoutError::TooNarrow { id: ButtonId::new("a"), width: 58, min: 64 });
}

#[test]
fn thirty_auto_buttons_still_fit_the_bar() {
    let keys: Vec<Button> = (0..30).map(|i| button(&format!("k{i}"), Width::Auto)).collect();
    let placed = place(&layer(vec![keys]), BAR, &RULES).unwrap();
    let got = spans(&placed);
    assert_eq!(got.len(), 30);
    assert_eq!(got[0], ("k0".to_owned(), 8, 65));
    assert_eq!(got[1], ("k1".to_owned(), 81, 65));
    assert_eq!(got[2], ("k2".to_owned(), 154, 64));
    assert_eq!(got[29], ("k29".to_owned(), 2098, 64));
    assert!(got[2..].iter().all(|(_, _, w)| *w == 64));
    assert_eq!(placed.last().unwrap().rect.right(), 2162);
}

#[test]
fn thirty_one_auto_buttons_overflow_with_the_needed_width() {
    let keys: Vec<Button> = (0..31).map(|i| button(&format!("k{i}"), Width::Auto)).collect();
    let err = place(&layer(vec![keys]), BAR, &RULES).unwrap_err();
    assert_eq!(err, LayoutError::Overflow { needed: 2224, available: 2154 });
}

#[test]
fn a_fixed_button_under_the_minimum_is_too_narrow() {
    let err = place(&layer(vec![vec![button("tiny", Width::Px(40))]]), BAR, &RULES).unwrap_err();
    assert_eq!(err, LayoutError::TooNarrow { id: ButtonId::new("tiny"), width: 40, min: 64 });
}
