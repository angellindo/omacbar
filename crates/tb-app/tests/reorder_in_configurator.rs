//! Functional: in the configurator, pressing a button in the preview makes it the selected
//! button; Alt+Left / Alt+Right move the selected button one step along the bar, and dragging a
//! button in the preview drops it before or after the button it is released over. Driven through
//! the real view with `rune::Runtime`; each test edits its own config file under cargo's temp dir.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod support {
    pub mod ui;
}

use rune::{Key, Mods};
use support::ui::*;
use tb_core::ButtonId;

const DEFAULT: &str = "esc | f1 f2 f3 f4 f5 f6 f7 f8 f9 f10 f11 f12";

fn open(test: &str) -> rune::Runtime<tb_app::State> {
    configurator(fresh_dir("reorder_in_configurator", test).join("config.toml"))
}

fn selected(rt: &rune::Runtime<tb_app::State>) -> Option<String> {
    rt.state.selected.as_ref().map(ButtonId::to_string)
}

#[test]
fn the_configurator_starts_with_nothing_selected() {
    let rt = open("starts_unselected");
    assert_eq!(rt.state.selected, None);
    assert_eq!(default_ids(&rt), groups(DEFAULT));
}

#[test]
fn pressing_a_button_in_the_preview_selects_it() {
    let mut rt = open("press_selects");
    tap(&mut rt, "preview-f3");
    assert_eq!(selected(&rt).as_deref(), Some("f3"));
    assert_eq!(default_ids(&rt), groups(DEFAULT), "a press alone moves nothing");
}

#[test]
fn alt_left_twice_moves_the_selected_f3_before_f1_in_the_config_and_the_preview() {
    let mut rt = open("alt_left_twice");
    tap(&mut rt, "preview-f3");
    alt(&mut rt, Key::Left);
    assert_eq!(default_ids(&rt), groups("esc | f1 f3 f2 f4 f5 f6 f7 f8 f9 f10 f11 f12"));
    alt(&mut rt, Key::Left);
    assert_eq!(default_ids(&rt), groups("esc | f3 f1 f2 f4 f5 f6 f7 f8 f9 f10 f11 f12"));
    assert_eq!(selected(&rt).as_deref(), Some("f3"), "the moved button stays selected");
    let (f3, f1) = (rect(&rt, "preview-f3"), rect(&rt, "preview-f1"));
    assert!(f3[0] < f1[0], "the preview draws F3 left of F1: f3 x {} vs f1 x {}", f3[0], f1[0]);
}

#[test]
fn a_third_alt_left_moves_f3_into_the_esc_group() {
    let mut rt = open("alt_left_thrice");
    tap(&mut rt, "preview-f3");
    for _ in 0..3 {
        alt(&mut rt, Key::Left);
    }
    assert_eq!(default_ids(&rt), groups("esc f3 | f1 f2 f4 f5 f6 f7 f8 f9 f10 f11 f12"));
    assert_eq!(selected(&rt).as_deref(), Some("f3"));
}

#[test]
fn alt_right_moves_the_selected_button_back_toward_the_right_edge() {
    let mut rt = open("alt_right_back");
    tap(&mut rt, "preview-f3");
    alt(&mut rt, Key::Left);
    alt(&mut rt, Key::Left);
    alt(&mut rt, Key::Right);
    assert_eq!(default_ids(&rt), groups("esc | f1 f3 f2 f4 f5 f6 f7 f8 f9 f10 f11 f12"));
    alt(&mut rt, Key::Right);
    assert_eq!(default_ids(&rt), groups(DEFAULT));
    assert_eq!(selected(&rt).as_deref(), Some("f3"));
}

#[test]
fn alt_left_on_the_first_button_and_alt_right_on_the_last_change_nothing() {
    let mut rt = open("alt_at_the_ends");
    tap(&mut rt, "preview-esc");
    alt(&mut rt, Key::Left);
    assert_eq!(default_ids(&rt), groups(DEFAULT));
    assert_eq!(selected(&rt).as_deref(), Some("esc"));
    tap(&mut rt, "preview-f12");
    alt(&mut rt, Key::Right);
    assert_eq!(default_ids(&rt), groups(DEFAULT));
    assert_eq!(selected(&rt).as_deref(), Some("f12"));
}

#[test]
fn alt_arrows_with_nothing_selected_move_nothing() {
    let mut rt = open("alt_without_selection");
    alt(&mut rt, Key::Left);
    alt(&mut rt, Key::Right);
    assert_eq!(default_ids(&rt), groups(DEFAULT));
    assert_eq!(rt.state.selected, None);
}

#[test]
fn arrows_without_alt_do_not_reorder() {
    let mut rt = open("arrows_without_alt");
    tap(&mut rt, "preview-f3");
    press(&mut rt, Key::Left, Mods::default());
    press(&mut rt, Key::Right, Mods::default());
    press(&mut rt, Key::Left, Mods { ctrl: true, alt: true, ..Mods::default() });
    assert_eq!(default_ids(&rt), groups(DEFAULT));
}

#[test]
fn dragging_f3_onto_the_left_half_of_f1_puts_f3_first_and_keeps_it_selected() {
    let mut rt = open("drag_f3_before_f1");
    let f1 = rect(&rt, "preview-f1");
    drag_to(&mut rt, "preview-f3", f1[0] + f1[2] * 0.25);
    assert_eq!(default_ids(&rt), groups("esc | f3 f1 f2 f4 f5 f6 f7 f8 f9 f10 f11 f12"));
    assert_eq!(selected(&rt).as_deref(), Some("f3"));
    let (f3, f1) = (rect(&rt, "preview-f3"), rect(&rt, "preview-f1"));
    assert!(
        f3[0] < f1[0],
        "the preview draws F3 left of F1 after the drop: f3 x {} vs f1 x {}",
        f3[0],
        f1[0]
    );
}

#[test]
fn dragging_f3_onto_the_right_half_of_f5_puts_f3_after_f5() {
    let mut rt = open("drag_f3_after_f5");
    let f5 = rect(&rt, "preview-f5");
    drag_to(&mut rt, "preview-f3", f5[0] + f5[2] * 0.75);
    assert_eq!(default_ids(&rt), groups("esc | f1 f2 f4 f5 f3 f6 f7 f8 f9 f10 f11 f12"));
}

#[test]
fn dragging_f3_and_releasing_it_over_its_own_place_changes_nothing() {
    let mut rt = open("drag_f3_onto_itself");
    let f3 = rect(&rt, "preview-f3");
    drag_to(&mut rt, "preview-f3", f3[0] + f3[2] * 0.8);
    assert_eq!(default_ids(&rt), groups(DEFAULT));
    assert_eq!(selected(&rt).as_deref(), Some("f3"));
}

#[test]
fn dragging_esc_out_of_its_group_removes_the_emptied_group() {
    let mut rt = open("drag_esc_out");
    let f2 = rect(&rt, "preview-f2");
    drag_to(&mut rt, "preview-esc", f2[0] + f2[2] * 0.25);
    assert_eq!(default_ids(&rt), groups("f1 esc f2 f3 f4 f5 f6 f7 f8 f9 f10 f11 f12"));
}
