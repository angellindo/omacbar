//! The layout engine places every free pixel: shared buttons get the whole pixels of their
//! shares, and the few leftover pixels go one each to the leftmost shared buttons.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use tb_core::{Action, BarSize, Button, Chord, Group, Layer, Placed, SpacingRules, Width, place, presets};

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

/// `(id, x, width)` of every placed button, after checking the vertical placement.
fn spans(placed: &[Placed]) -> Vec<(&str, u16, u16)> {
    for p in placed {
        assert_eq!((p.rect.y, p.rect.height), (8, 44), "{}", p.id);
    }
    placed.iter().map(|p| (p.id.as_str(), p.rect.x, p.rect.width)).collect()
}

#[test]
fn two_span_three_buttons_after_esc_fill_the_bar_to_the_right_edge_padding() {
    let l = layer(vec![vec![esc()], vec![button("a", Width::Span(3)), button("b", Width::Span(3))]]);
    let placed = place(&l, BAR, &RULES).unwrap();
    assert_eq!(spans(&placed), vec![("esc", 88, 120), ("a", 296, 887), ("b", 1195, 887)]);
    assert_eq!(placed.last().unwrap().rect.right(), 2082);
}

#[test]
fn a_remainder_larger_than_the_shared_buttons_goes_to_the_leftmost_after_proportional_floors() {
    let l = layer(vec![
        vec![esc()],
        vec![button("a", Width::Auto), button("b", Width::Span(3)), button("c", Width::Auto)],
    ]);
    // Keep 1994 free pixels and five shares: a per-share remainder of four for three buttons.
    let rules = SpacingRules { edge_padding: 8, gap: 8, group_gap: 24, ..RULES };
    let placed = place(&l, BAR, &rules).unwrap();
    assert_eq!(spans(&placed), vec![("esc", 8, 120), ("a", 152, 399), ("b", 559, 1197), ("c", 1764, 398)]);
    assert_eq!(placed.last().unwrap().rect.right(), 2162);
}

#[test]
fn the_default_f_key_layer_keeps_its_rects() {
    let placed = place(presets::default_layout().layer(Chord::Default).unwrap(), BAR, &RULES).unwrap();
    let mut want = vec![("esc", 88, 116), ("f1", 232, 144), ("f2", 388, 144)];
    let ids = ["f3", "f4", "f5", "f6", "f7", "f8", "f9", "f10", "f11", "f12"];
    for (i, id) in ids.into_iter().enumerate() {
        want.push((id, 544 + 155 * u16::try_from(i).unwrap(), 143));
    }
    assert_eq!(spans(&placed), want);
    assert_eq!(placed.last().unwrap().rect.right(), 2082);
}

#[test]
fn the_fn_ctrl_layer_keeps_its_rects() {
    let placed = place(presets::default_layout().layer(Chord::FnCtrl).unwrap(), BAR, &RULES).unwrap();
    assert_eq!(
        spans(&placed),
        vec![
            ("esc", 88, 116),
            ("theme-next", 292, 672),
            ("screenshot", 976, 336),
            ("background-next", 1324, 335),
            ("lock", 1747, 335),
        ]
    );
    assert_eq!(placed.last().unwrap().rect.right(), 2082);
}

#[test]
fn a_span_two_between_two_auto_buttons_keeps_its_rects() {
    let l =
        layer(vec![vec![button("a", Width::Auto), button("wide", Width::Span(2)), button("c", Width::Auto)]]);
    let placed = place(&l, BAR, &RULES).unwrap();
    assert_eq!(spans(&placed), vec![("a", 88, 493), ("wide", 593, 985), ("c", 1590, 492)]);
}
