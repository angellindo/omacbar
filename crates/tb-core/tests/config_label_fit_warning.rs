//! A label wider than its label room (the button's width minus the label padding on both sides) is
//! a warning, never an issue. The width comes from the measurer the caller injects; here a table of
//! known widths stands in for the text system.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::cell::RefCell;

use tb_core::config::{Config, ConfigIssue, ConfigWarning, validate, warnings};
use tb_core::{Action, BarSize, BarTheme, Button, ButtonId, Chord, SpacingRules};

/// Labels with a known measured width; every other label measures 10 px.
const KNOWN_WIDTHS: [(&str, u32); 4] =
    [("fits-f-key", 119), ("wide-f-key", 120), ("fits-esc", 92), ("wide-esc", 93)];

fn measured(label: &str) -> u32 {
    KNOWN_WIDTHS.iter().find(|(l, _)| *l == label).map_or(10, |(_, w)| *w)
}

fn warnings_of(config: &Config, label_width: impl Fn(&str) -> u32) -> Vec<ConfigWarning> {
    warnings(config, BarSize::MACBOOK_PRO, &SpacingRules::STANDARD, label_width)
}

fn relabel(config: &mut Config, chord: Chord, id: &str, label: &str) {
    let layer = config.layout.layer_mut(chord).unwrap();
    let button =
        layer.groups.iter_mut().flat_map(|g| g.buttons.iter_mut()).find(|b| b.id.as_str() == id).unwrap();
    label.clone_into(&mut button.label);
}

/// Replaces the F-key group of the default layer with `count` F-keys (esc stays alone in front).
fn default_layer_with_f_keys(config: &mut Config, count: usize) {
    let layer = config.layout.layer_mut(Chord::Default).unwrap();
    let f_keys = &mut layer.groups.last_mut().unwrap().buttons;
    *f_keys = (1..=count).map(|n| Button::new(&format!("f{n}"), &format!("F{n}"), Action::None)).collect();
}

fn label_too_wide(chord: Chord, id: &str, width: u32, room: u32) -> ConfigWarning {
    ConfigWarning::LabelTooWide { chord, id: ButtonId::new(id), width, room }
}

#[test]
fn an_f_key_label_exactly_as_wide_as_its_room_has_no_warning() {
    let mut config = Config::default();
    relabel(&mut config, Chord::Default, "f5", "fits-f-key");
    assert_eq!(warnings_of(&config, measured), vec![]);
}

#[test]
fn an_f_key_label_one_pixel_wider_than_its_room_is_a_label_too_wide_warning() {
    let mut config = Config::default();
    relabel(&mut config, Chord::Default, "f5", "wide-f-key");
    // F-keys on the default layer are 143 px wide (button standard § Width): room 143 - 2 x 12.
    assert_eq!(warnings_of(&config, measured), vec![label_too_wide(Chord::Default, "f5", 120, 119)]);
}

#[test]
fn the_room_of_esc_is_its_fixed_width_minus_both_label_paddings() {
    let mut config = Config::default();
    relabel(&mut config, Chord::Default, "esc", "fits-esc");
    assert_eq!(warnings_of(&config, measured), vec![]);
    relabel(&mut config, Chord::Default, "esc", "wide-esc");
    assert_eq!(warnings_of(&config, measured), vec![label_too_wide(Chord::Default, "esc", 93, 92)]);
}

#[test]
fn the_measurer_is_given_every_label_as_written_in_the_config() {
    let mut config = Config::default();
    relabel(&mut config, Chord::Default, "f5", "two\nlines");
    let seen = RefCell::new(Vec::<String>::new());
    let found = warnings_of(&config, |label| {
        seen.borrow_mut().push(label.to_owned());
        10
    });
    assert_eq!(found, vec![]);
    let mut seen = seen.into_inner();
    seen.sort();
    seen.dedup();
    let mut written: Vec<String> =
        config.layout.layers.iter().flat_map(tb_core::Layer::buttons).map(|b| b.label.clone()).collect();
    written.sort();
    written.dedup();
    assert!(written.contains(&"two\nlines".to_owned()));
    assert_eq!(seen, written);
}

#[test]
fn labels_too_wide_in_one_layer_are_warned_in_button_order() {
    let mut config = Config::default();
    relabel(&mut config, Chord::Default, "f9", "wide-f-key");
    relabel(&mut config, Chord::Default, "f5", "wide-f-key");
    assert_eq!(
        warnings_of(&config, measured),
        vec![label_too_wide(Chord::Default, "f5", 120, 119), label_too_wide(Chord::Default, "f9", 120, 119)]
    );
}

#[test]
fn a_crowded_layer_lists_its_capacity_warning_before_its_label_warnings() {
    let mut config = Config::default();
    default_layer_with_f_keys(&mut config, 16);
    relabel(&mut config, Chord::Default, "f5", "far too wide");
    let found = warnings_of(&config, |label| if label == "far too wide" { 1000 } else { 10 });
    assert_eq!(found.len(), 2, "{found:?}");
    assert_eq!(found[0], ConfigWarning::TooManyButtons { chord: Chord::Default, buttons: 17 });
    assert!(
        matches!(&found[1], ConfigWarning::LabelTooWide { chord: Chord::Default, id, width: 1000, room }
            if id.as_str() == "f5" && *room > 0 && *room < 119),
        "{found:?}"
    );
}

#[test]
fn a_layer_that_does_not_fit_gets_no_label_warning_but_other_layers_are_still_checked() {
    let mut config = Config::default();
    default_layer_with_f_keys(&mut config, 30);
    relabel(&mut config, Chord::Default, "f5", "far too wide");
    relabel(&mut config, Chord::Fn, "play-pause", "far too wide");
    let found = warnings_of(&config, |label| if label == "far too wide" { 1000 } else { 10 });
    assert_eq!(found.len(), 2, "{found:?}");
    assert_eq!(found[0], ConfigWarning::TooManyButtons { chord: Chord::Default, buttons: 31 });
    assert!(
        matches!(&found[1], ConfigWarning::LabelTooWide { chord: Chord::Fn, id, width: 1000, room }
            if id.as_str() == "play-pause" && *room > 0 && *room < 1000),
        "{found:?}"
    );
}

#[test]
fn a_config_whose_labels_are_too_wide_still_validates() {
    let mut config = Config::default();
    relabel(&mut config, Chord::Default, "f5", "a label far too long to fit inside one button");
    relabel(&mut config, Chord::Default, "esc", "an escape label far too long");
    let issues: Vec<ConfigIssue> =
        validate(&config, BarSize::MACBOOK_PRO, &SpacingRules::STANDARD, &BarTheme::omarchy_fallback());
    assert_eq!(issues, vec![]);
}
