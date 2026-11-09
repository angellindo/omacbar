//! A layer with more buttons than the recommended capacity (16) is a warning, never an issue: the
//! config still validates and the bar still draws it.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use tb_core::config::{
    Config, ConfigWarning, RECOMMENDED_MAX_BUTTONS, parse_valid, to_toml, validate, warnings,
};
use tb_core::{Action, BarSize, BarTheme, Button, Chord, SpacingRules};

/// A measurer that finds every label narrow, so only capacity can warn.
fn narrow(_: &str) -> u32 {
    0
}

fn warnings_of(config: &Config) -> Vec<ConfigWarning> {
    warnings(config, BarSize::MACBOOK_PRO, &SpacingRules::STANDARD, narrow)
}

/// Replaces the F-key group of the default layer with `count` F-keys (esc stays alone in front).
fn default_layer_with_f_keys(config: &mut Config, count: usize) {
    let layer = config.layout.layer_mut(Chord::Default).unwrap();
    let f_keys = &mut layer.groups.last_mut().unwrap().buttons;
    *f_keys = (1..=count).map(|n| Button::new(&format!("f{n}"), &format!("F{n}"), Action::None)).collect();
}

/// Adds `extra` buttons to the last group of `chord`'s layer.
fn add_buttons(config: &mut Config, chord: Chord, extra: usize) {
    let layer = config.layout.layer_mut(chord).unwrap();
    let group = &mut layer.groups.last_mut().unwrap().buttons;
    for n in 1..=extra {
        group.push(Button::new(&format!("extra-{n}"), &format!("x{n}"), Action::None));
    }
}

fn buttons_in(config: &Config, chord: Chord) -> usize {
    config.layout.layer(chord).unwrap().buttons().count()
}

#[test]
fn the_recommended_capacity_is_sixteen_buttons_per_layer() {
    assert_eq!(RECOMMENDED_MAX_BUTTONS, 16);
}

#[test]
fn the_default_config_has_no_warning() {
    assert_eq!(warnings_of(&Config::default()), vec![]);
}

#[test]
fn a_layer_of_exactly_sixteen_buttons_has_no_warning() {
    let mut config = Config::default();
    default_layer_with_f_keys(&mut config, 15);
    assert_eq!(buttons_in(&config, Chord::Default), 16);
    assert_eq!(warnings_of(&config), vec![]);
}

#[test]
fn a_layer_of_seventeen_buttons_is_one_too_many_buttons_warning() {
    let mut config = Config::default();
    default_layer_with_f_keys(&mut config, 16);
    assert_eq!(buttons_in(&config, Chord::Default), 17);
    assert_eq!(
        warnings_of(&config),
        vec![ConfigWarning::TooManyButtons { chord: Chord::Default, buttons: 17 }]
    );
}

#[test]
fn a_layer_over_capacity_still_validates_and_its_text_still_parses() {
    let mut config = Config::default();
    default_layer_with_f_keys(&mut config, 16);
    let (size, rules, theme) = (BarSize::MACBOOK_PRO, SpacingRules::STANDARD, BarTheme::omarchy_fallback());
    assert_eq!(validate(&config, size, &rules, &theme), vec![]);
    let text = to_toml(&config).unwrap();
    assert_eq!(parse_valid(&text, size, &rules, &theme), Ok(config));
}

#[test]
fn every_crowded_layer_gets_its_own_warning_in_layer_order() {
    let mut config = Config::default();
    default_layer_with_f_keys(&mut config, 16);
    let fn_buttons = buttons_in(&config, Chord::Fn);
    add_buttons(&mut config, Chord::Fn, 18 - fn_buttons);
    assert_eq!(buttons_in(&config, Chord::Fn), 18);
    assert_eq!(
        warnings_of(&config),
        vec![
            ConfigWarning::TooManyButtons { chord: Chord::Default, buttons: 17 },
            ConfigWarning::TooManyButtons { chord: Chord::Fn, buttons: 18 },
        ]
    );
}

#[test]
fn a_crowded_layer_too_wide_for_the_bar_still_gets_the_capacity_warning() {
    let mut config = Config::default();
    default_layer_with_f_keys(&mut config, 30);
    let (size, rules, theme) = (BarSize::MACBOOK_PRO, SpacingRules::STANDARD, BarTheme::omarchy_fallback());
    let issues = validate(&config, size, &rules, &theme);
    assert!(
        issues
            .iter()
            .any(|i| matches!(i, tb_core::config::ConfigIssue::Layout { chord: Chord::Default, .. })),
        "31 buttons cannot each get 64 px: {issues:?}"
    );
    assert_eq!(
        warnings_of(&config),
        vec![ConfigWarning::TooManyButtons { chord: Chord::Default, buttons: 31 }]
    );
}
