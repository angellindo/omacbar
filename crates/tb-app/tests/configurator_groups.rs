//! Group insertion and card drag, through actual pointer and keyboard input.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod support {
    pub mod configurator;
    pub mod ui;
}
use rune::Key;
use support::configurator::*;
use tb_core::{Action, ButtonId, Chord, Group};

#[test]
fn a_group_add_targets_its_own_group_independently_of_selection_and_allocates_free_ids() {
    let mut rt = open("target_group");
    operate(&mut rt, "row-e");
    operate(&mut rt, "add-button-group-0");
    assert_eq!(default_ids(&rt), groups("a b button-1 | c | d e"));
    assert_eq!(rt.state.selected, Some(ButtonId::new("button-1")));
    operate(&mut rt, "row-e");
    operate(&mut rt, "add-button-group-0");
    assert_eq!(default_ids(&rt), groups("a b button-1 button-2 | c | d e"));
    assert_eq!(button(&rt, Chord::Default, "button-2").action, Action::None);
    assert_eq!(default_layer(&rt).group_gap, Some(28));
    assert_eq!(rt.state.config.layout.layer(Chord::Fn), fixture().layout.layer(Chord::Fn));
}

#[test]
fn an_explicit_loaded_empty_group_can_receive_its_first_button() {
    let mut config = fixture();
    config.layout.layer_mut(Chord::Default).unwrap().groups.insert(1, Group::default());
    let (mut rt, _) = open_config("empty_group", &config, (1200.0, 760.0));
    operate(&mut rt, "row-e");
    operate(&mut rt, "add-button-group-1");
    assert_eq!(default_ids(&rt), groups("a b | button-1 | c | d e"));
    assert_eq!(rt.state.selected, Some(ButtonId::new("button-1")));
}

#[test]
fn add_group_on_an_absent_layer_creates_exactly_one_group_and_selects_its_none_button() {
    let mut config = fixture();
    config.layout.layers.retain(|l| l.chord == Chord::Default);
    let (mut rt, _) = open_config("missing_add_group", &config, (1200.0, 760.0));
    operate(&mut rt, "tab-fnshift");
    operate(&mut rt, "add-group");
    assert_eq!(rt.state.config.layout.layers.len(), 2);
    let layer = rt.state.config.layout.layer(Chord::FnShift).unwrap();
    assert_eq!(ids(layer), groups("button-1"));
    assert_eq!(layer.buttons().next().unwrap().action, Action::None);
    assert_eq!(rt.state.selected, Some(ButtonId::new("button-1")));
}

#[test]
fn card_drag_moves_before_and_after_targets_in_other_groups_without_losing_selection() {
    let mut rt = open("card_drag_cross_groups");
    drag(&mut rt, "row-c", "row-a", 0.2);
    assert_eq!(default_ids(&rt), groups("c a b | d e"));
    assert_eq!(rt.state.selected, Some(ButtonId::new("c")));
    assert_eq!(words(&rt, "panel-id"), "c");
    drag(&mut rt, "row-c", "row-e", 0.8);
    assert_eq!(default_ids(&rt), groups("a b | d e c"));
    assert_eq!(rt.state.selected, Some(ButtonId::new("c")));
    assert_eq!(default_layer(&rt).group_gap, Some(28));
}

#[test]
fn card_drag_reorders_inside_a_group_and_cancel_or_self_drop_changes_nothing() {
    let mut rt = open("card_drag_within");
    drag(&mut rt, "row-b", "row-a", 0.2);
    assert_eq!(default_ids(&rt), groups("b a | c | d e"));
    let before = rt.state.config.clone();
    drag(&mut rt, "row-b", "row-b", 0.5);
    assert_eq!(rt.state.config, before);
    let (x, y) = centre(&rt, "row-b");
    rt.pointer_down(x, y);
    rt.frame(1.0 / 60.0);
    rt.pointer_move(x + 120.0, y + 80.0);
    rt.frame(1.0 / 60.0);
    press(&mut rt, Key::Escape, rune::Mods::default());
    rt.pointer_up(x + 120.0, y + 80.0);
    settle(&mut rt);
    assert_eq!(rt.state.config, before, "Escape cancels a card drag");
}

#[test]
fn selecting_a_card_blurs_text_and_alt_arrows_move_across_groups_with_the_draft_attached() {
    let mut rt = open("card_alt_draft");
    operate(&mut rt, "row-c");
    edit(&mut rt, "field-width", "not-a-width");
    operate(&mut rt, "row-c");
    assert!(!element(&rt, "field-width").focused);
    alt(&mut rt, Key::Left);
    assert_eq!(default_ids(&rt), groups("a b c | d e"));
    assert_eq!(rt.state.selected, Some(ButtonId::new("c")));
    assert_eq!(words(&rt, "field-width"), "not-a-width");
    assert!(shown(&rt, "width-error"));
    assert_blocked(&rt, &["c"]);
}

#[test]
fn removing_the_only_button_discards_its_issue_and_reindexes_group_add_targets() {
    let mut rt = open("remove_group_issue");
    operate(&mut rt, "row-c");
    edit(&mut rt, "field-width", "bad-width");
    operate(&mut rt, "remove-button");
    assert_eq!(rt.state.selected, None);
    assert_eq!(default_ids(&rt), groups("a b | d e"));
    assert!(shown(&rt, "panel-empty"));
    operate(&mut rt, "add-button-group-1");
    assert_eq!(default_ids(&rt), groups("a b | d e button-1"));
    assert!(!disabled(&rt, "save"), "removed draft no longer blocks save");
}

#[test]
fn existing_toolbar_add_and_keyboard_step_keep_their_domain_order() {
    let mut rt = open("preserve_toolbar_order");
    operate(&mut rt, "row-c");
    operate(&mut rt, "add-button");
    assert_eq!(default_ids(&rt), groups("a b | c button-1 | d e"));
    alt(&mut rt, Key::Left);
    assert_eq!(default_ids(&rt), groups("a b | button-1 c | d e"));
    operate(&mut rt, "add-group");
    assert_eq!(default_ids(&rt), groups("a b | button-1 c | button-2 | d e"));
    operate(&mut rt, "remove-button");
    assert_eq!(default_ids(&rt), groups("a b | button-1 c | d e"));
}
