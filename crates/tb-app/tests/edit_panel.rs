//! Functional: the edit panel shows the selected button's fields, applies every change to the
//! config at once (the preview follows), lists every `ConfigIssue` of the config live, and the
//! toolbar adds and removes buttons and groups. Driven through the real view with
//! `rune::Runtime`; each test edits its own config file under cargo's temp dir.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod support {
    pub mod ui;
}

use support::ui::*;
use tb_core::config::ConfigIssue;
use tb_core::{Action, Button, ButtonId, Chord, Width};
use tb_render::view::color;

fn open(test: &str) -> rune::Runtime<tb_app::State> {
    configurator(fresh_dir("edit_panel", test).join("config.toml"))
}

/// The configurator with `id` pressed in the preview.
fn open_on(test: &str, id: &str) -> rune::Runtime<tb_app::State> {
    let mut rt = open(test);
    tap(&mut rt, &format!("preview-{id}"));
    assert_eq!(rt.state.selected, Some(ButtonId::new(id)), "pressing preview-{id} selects it");
    rt
}

fn button(rt: &rune::Runtime<tb_app::State>, id: &str) -> Button {
    default_layer(rt)
        .buttons()
        .find(|b| b.id.as_str() == id)
        .cloned()
        .unwrap_or_else(|| panic!("no button {id}"))
}

/// The texts of `issue-0`, `issue-1`, … in order.
fn issues(rt: &rune::Runtime<tb_app::State>) -> Vec<String> {
    (0..).map_while(|n| text_of(rt, &format!("issue-{n}"))).collect()
}

fn id(s: &str) -> ButtonId {
    ButtonId::new(s)
}

#[test]
fn with_nothing_selected_the_panel_shows_its_empty_hint_and_the_config_has_no_issues() {
    let rt = open("idle");
    assert!(shown(&rt, "panel-empty"));
    assert!(!shown(&rt, "field-label"), "no fields without a selected button");
    assert!(shown(&rt, "no-issues"));
    assert_eq!(issues(&rt), Vec::<String>::new());
}

#[test]
fn selecting_f1_fills_the_panel_with_its_fields() {
    let rt = open_on("select_f1", "f1");
    assert!(!shown(&rt, "panel-empty"));
    assert_eq!(text_of(&rt, "panel-id").as_deref(), Some("f1"));
    assert_eq!(text_of(&rt, "field-label").as_deref(), Some("F1"));
    assert_eq!(text_of(&rt, "field-width").as_deref(), Some("auto"));
    assert_eq!(text_of(&rt, "field-action").as_deref(), Some("F1"));
    assert_eq!(text_of(&rt, "field-fill").as_deref(), Some(""));
    let fill = |key: &str| rt.elements().into_iter().find(|e| e.key.as_deref() == Some(key)).unwrap().fill;
    let accent = color(rt.state.theme.accent);
    assert_eq!(fill("action-key"), accent, "the key action kind is marked active");
    assert_ne!(fill("action-none"), accent);
    assert_ne!(fill("action-command"), accent);
}

#[test]
fn selecting_another_button_shows_its_fields() {
    let mut rt = open_on("select_other", "f1");
    tap(&mut rt, "preview-esc");
    assert_eq!(text_of(&rt, "panel-id").as_deref(), Some("esc"));
    assert_eq!(text_of(&rt, "field-label").as_deref(), Some("esc"));
    assert_eq!(text_of(&rt, "field-width").as_deref(), Some("px 116"));
    assert_eq!(text_of(&rt, "field-action").as_deref(), Some("Esc"));
}

#[test]
fn typing_a_label_renames_the_button_in_the_config_and_the_preview() {
    let mut rt = open_on("label", "f1");
    type_into(&mut rt, "field-label", "go");
    assert_eq!(button(&rt, "f1").label, "go");
    assert!(shows_text_inside(&rt, "preview-f1", "go"), "the preview draws the new label");
    assert!(shown(&rt, "no-issues"));
}

#[test]
fn clearing_the_label_lists_an_empty_label_issue_at_once_and_retyping_clears_it() {
    let mut rt = open_on("empty_label", "f1");
    type_into(&mut rt, "field-label", "");
    assert_eq!(button(&rt, "f1").label, "");
    let expected = ConfigIssue::EmptyLabel { chord: Chord::Default, id: id("f1") }.to_string();
    assert_eq!(issues(&rt), vec![expected], "the issue is listed before any save");
    assert!(!shown(&rt, "no-issues"));
    type_into(&mut rt, "field-label", "F1");
    assert_eq!(issues(&rt), Vec::<String>::new());
    assert!(shown(&rt, "no-issues"));
}

#[test]
fn an_unknown_fill_colour_is_an_issue_and_a_theme_token_is_not() {
    let mut rt = open_on("fill_token", "f1");
    type_into(&mut rt, "field-fill", "nope");
    let expected =
        ConfigIssue::UnknownColor { chord: Chord::Default, id: id("f1"), color: "nope".to_owned() }
            .to_string();
    assert_eq!(issues(&rt), vec![expected]);
    type_into(&mut rt, "field-fill", "red");
    assert_eq!(button(&rt, "f1").style.fill.as_deref(), Some("red"));
    assert_eq!(issues(&rt), Vec::<String>::new());
    assert!(!shown(&rt, "hex-fill"), "a theme token follows theme switches: no hex note");
}

#[test]
fn a_hex_fill_is_kept_and_marked_as_not_following_the_theme() {
    let mut rt = open_on("fill_hex", "f1");
    type_into(&mut rt, "field-fill", "#ff0000");
    assert_eq!(button(&rt, "f1").style.fill.as_deref(), Some("#ff0000"));
    assert!(shown(&rt, "hex-fill"));
    assert_eq!(issues(&rt), Vec::<String>::new());
}

#[test]
fn an_empty_colour_field_removes_the_override() {
    let mut rt = open_on("fill_cleared", "f1");
    type_into(&mut rt, "field-fill", "red");
    type_into(&mut rt, "field-fill", "");
    assert_eq!(button(&rt, "f1").style.fill, None);
}

#[test]
fn text_and_border_colours_are_edited_and_checked_like_the_fill() {
    let mut rt = open_on("text_border", "f1");
    type_into(&mut rt, "field-text", "accent");
    type_into(&mut rt, "field-border", "nope");
    let f1 = button(&rt, "f1");
    assert_eq!(f1.style.text.as_deref(), Some("accent"));
    assert_eq!(f1.style.border.as_deref(), Some("nope"));
    let expected =
        ConfigIssue::UnknownColor { chord: Chord::Default, id: id("f1"), color: "nope".to_owned() }
            .to_string();
    assert_eq!(issues(&rt), vec![expected]);
    type_into(&mut rt, "field-border", "#00ff00");
    assert!(shown(&rt, "hex-border"));
    assert!(!shown(&rt, "hex-text"));
    assert_eq!(issues(&rt), Vec::<String>::new());
}

#[test]
fn a_span_two_width_makes_f1_wider_than_f2_in_the_preview() {
    let mut rt = open_on("span_two", "f1");
    let before = rect(&rt, "preview-f1")[2];
    type_into(&mut rt, "field-width", "span 2");
    assert_eq!(button(&rt, "f1").width, Width::Span(2));
    let (f1, f2) = (rect(&rt, "preview-f1")[2], rect(&rt, "preview-f2")[2]);
    assert!(f1 > before, "F1 grew: {before} -> {f1}");
    assert!(f1 > f2 * 1.5, "F1 takes two shares: f1 {f1} vs f2 {f2}");
}

#[test]
fn width_text_that_is_not_a_width_shows_an_error_and_keeps_the_width() {
    let mut rt = open_on("bad_width", "f1");
    assert!(!shown(&rt, "width-error"));
    type_into(&mut rt, "field-width", "spam");
    assert!(shown(&rt, "width-error"));
    assert_eq!(button(&rt, "f1").width, Width::Auto, "the model keeps its last width");
    assert_eq!(issues(&rt), Vec::<String>::new());
    type_into(&mut rt, "field-width", "span 2");
    assert!(!shown(&rt, "width-error"));
    assert_eq!(button(&rt, "f1").width, Width::Span(2));
}

#[test]
fn a_width_past_the_bar_is_a_layout_issue_and_the_preview_says_it_does_not_fit() {
    let mut rt = open_on("too_wide", "f1");
    type_into(&mut rt, "field-width", "px 2000");
    assert_eq!(button(&rt, "f1").width, Width::Px(2000));
    let listed = issues(&rt);
    assert_eq!(listed.len(), 1, "one issue: {listed:?}");
    assert!(listed[0].starts_with("layer `Default` does not fit"), "a layout issue: {}", listed[0]);
    assert!(!shown(&rt, "preview-f2"), "a layer that does not fit is not drawn as a bar");
    assert!(
        rt.elements().iter().any(|e| e.text.as_deref().is_some_and(|t| t.contains("does not fit the bar"))),
        "the preview says the layer does not fit the bar"
    );
}

#[test]
fn a_command_action_is_set_from_its_kind_and_command_line() {
    let mut rt = open_on("command_action", "f1");
    tap(&mut rt, "action-command");
    type_into(&mut rt, "field-action", "omarchy lock");
    assert_eq!(button(&rt, "f1").action, Action::Command { run: "omarchy lock".to_owned() });
}

#[test]
fn a_key_action_reads_plus_separated_key_names() {
    let mut rt = open_on("key_action", "f1");
    type_into(&mut rt, "field-action", "LeftCtrl + F5");
    assert_eq!(button(&rt, "f1").action, Action::Key { keys: vec!["LeftCtrl".to_owned(), "F5".to_owned()] });
}

#[test]
fn the_none_action_kind_clears_the_action_and_hides_its_field() {
    let mut rt = open_on("none_action", "f1");
    tap(&mut rt, "action-none");
    assert_eq!(button(&rt, "f1").action, Action::None);
    assert!(!shown(&rt, "field-action"));
}

#[test]
fn add_button_with_f3_selected_adds_a_selected_new_button_after_f3() {
    let mut rt = open_on("add_button", "f3");
    tap(&mut rt, "add-button");
    assert_eq!(default_ids(&rt), groups("esc | f1 f2 f3 button-1 f4 f5 f6 f7 f8 f9 f10 f11 f12"));
    assert_eq!(rt.state.selected, Some(id("button-1")));
    assert_eq!(button(&rt, "button-1").label, "new");
    assert!(shown(&rt, "preview-button-1"), "the preview draws the new button");
    assert_eq!(text_of(&rt, "field-label").as_deref(), Some("new"));
}

#[test]
fn add_group_with_f3_selected_adds_a_group_after_the_f_key_group() {
    let mut rt = open_on("add_group", "f3");
    tap(&mut rt, "add-group");
    assert_eq!(default_ids(&rt), groups("esc | f1 f2 f3 f4 f5 f6 f7 f8 f9 f10 f11 f12 | button-1"));
    assert_eq!(rt.state.selected, Some(id("button-1")));
    assert!(shown(&rt, "preview-button-1"));
}

#[test]
fn remove_button_removes_the_selected_button_and_clears_the_selection() {
    let mut rt = open_on("remove_button", "f3");
    tap(&mut rt, "remove-button");
    assert_eq!(default_ids(&rt), groups("esc | f1 f2 f4 f5 f6 f7 f8 f9 f10 f11 f12"));
    assert_eq!(rt.state.selected, None);
    assert!(!shown(&rt, "preview-f3"));
    assert!(shown(&rt, "panel-empty"));
}

#[test]
fn removing_the_only_button_of_a_group_removes_the_group() {
    let mut rt = open_on("remove_esc", "esc");
    tap(&mut rt, "remove-button");
    assert_eq!(default_ids(&rt), groups("f1 f2 f3 f4 f5 f6 f7 f8 f9 f10 f11 f12"));
}

#[test]
fn two_added_buttons_never_share_an_id() {
    let mut rt = open_on("two_adds", "f3");
    tap(&mut rt, "add-button");
    tap(&mut rt, "add-button");
    assert_eq!(default_ids(&rt), groups("esc | f1 f2 f3 button-1 button-2 f4 f5 f6 f7 f8 f9 f10 f11 f12"));
    assert_eq!(rt.state.selected, Some(id("button-2")));
    assert_eq!(issues(&rt), Vec::<String>::new(), "no duplicate id issue");
}
