//! Key names are checked with the config. A `key` action with a name the key vocabulary does not
//! know, or with no keys, is an issue and rejects the config, reported with every other issue. A
//! `key` action on a modifier layer (`fn-ctrl`, `fn-alt`, `fn-shift`, `fn-super`) is a warning, not
//! an issue: its keys would combine with the held modifier, so it should be a command (ADR-0011).
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use tb_core::config::{Config, ConfigIssue, ConfigWarning, parse_valid, to_toml, validate, warnings};
use tb_core::{Action, BarSize, BarTheme, Button, ButtonId, Chord, Group, Layer, SpacingRules};

fn issues_of(config: &Config) -> Vec<ConfigIssue> {
    validate(config, BarSize::MACBOOK_PRO, &SpacingRules::STANDARD, &BarTheme::omarchy_fallback())
}

/// The config written as text and read back with the checked parse.
fn checked(config: &Config) -> Result<Config, Vec<ConfigIssue>> {
    let text = to_toml(config).unwrap();
    parse_valid(&text, BarSize::MACBOOK_PRO, &SpacingRules::STANDARD, &BarTheme::omarchy_fallback())
}

/// Warnings with a measurer that finds every label narrow, so labels never warn.
fn warnings_of(config: &Config) -> Vec<ConfigWarning> {
    warnings(config, BarSize::MACBOOK_PRO, &SpacingRules::STANDARD, |_| 0)
}

fn key(id: &str, keys: &[&str]) -> Button {
    Button::new(id, id, Action::Key { keys: keys.iter().map(|k| (*k).to_owned()).collect() })
}

fn command(id: &str) -> Button {
    Button::new(id, id, Action::Command { run: format!("echo {id}") })
}

fn button_mut<'a>(config: &'a mut Config, chord: Chord, id: &str) -> &'a mut Button {
    let layer = config.layout.layer_mut(chord).unwrap();
    layer.groups.iter_mut().flat_map(|g| g.buttons.iter_mut()).find(|b| b.id.as_str() == id).unwrap()
}

fn set_keys(config: &mut Config, chord: Chord, id: &str, keys: &[&str]) {
    button_mut(config, chord, id).action =
        Action::Key { keys: keys.iter().map(|k| (*k).to_owned()).collect() };
}

/// The default config with its `chord` layer replaced (or added) as one group of `buttons`.
fn with_layer(mut config: Config, chord: Chord, buttons: Vec<Button>) -> Config {
    config.layout.layers.retain(|l| l.chord != chord);
    config.layout.layers.push(Layer { chord, groups: vec![Group { buttons }] });
    config
}

fn unknown_key(chord: Chord, id: &str, key: &str) -> ConfigIssue {
    ConfigIssue::UnknownKey { chord, id: ButtonId::new(id), key: key.to_owned() }
}

fn key_on_modifier_layer(chord: Chord, id: &str) -> ConfigWarning {
    ConfigWarning::KeyOnModifierLayer { chord, id: ButtonId::new(id) }
}

#[test]
fn a_key_action_with_an_unknown_key_name_rejects_the_config_with_one_unknown_key_issue() {
    let mut config = Config::default();
    set_keys(&mut config, Chord::Default, "f5", &["F55"]);
    assert_eq!(checked(&config), Err(vec![unknown_key(Chord::Default, "f5", "F55")]));
}

#[test]
fn a_key_action_with_no_keys_rejects_the_config_with_one_no_keys_issue() {
    let mut config = Config::default();
    set_keys(&mut config, Chord::Default, "f5", &[]);
    assert_eq!(
        checked(&config),
        Err(vec![ConfigIssue::NoKeys { chord: Chord::Default, id: ButtonId::new("f5") }])
    );
}

#[test]
fn each_unknown_name_of_one_key_action_is_its_own_issue_in_key_order() {
    let mut config = Config::default();
    set_keys(&mut config, Chord::Fn, "volume-up", &["LeftCtrl", "f5", "VolumeUp", "KEY_F5"]);
    assert_eq!(
        issues_of(&config),
        vec![unknown_key(Chord::Fn, "volume-up", "f5"), unknown_key(Chord::Fn, "volume-up", "KEY_F5")]
    );
}

#[test]
fn key_issues_are_reported_with_every_other_issue_after_the_colour_issues_of_their_button() {
    let mut config = Config::default();
    set_keys(&mut config, Chord::Default, "f5", &["F55"]);
    button_mut(&mut config, Chord::Default, "f5").style.fill = Some("no-such-colour".to_owned());
    button_mut(&mut config, Chord::Default, "f6").label = " ".to_owned();
    set_keys(&mut config, Chord::Fn, "mute", &[]);
    assert_eq!(
        issues_of(&config),
        vec![
            ConfigIssue::UnknownColor {
                chord: Chord::Default,
                id: ButtonId::new("f5"),
                color: "no-such-colour".to_owned()
            },
            unknown_key(Chord::Default, "f5", "F55"),
            ConfigIssue::EmptyLabel { chord: Chord::Default, id: ButtonId::new("f6") },
            ConfigIssue::NoKeys { chord: Chord::Fn, id: ButtonId::new("mute") },
        ]
    );
}

#[test]
fn a_key_action_with_known_names_has_no_issue() {
    let mut config = Config::default();
    set_keys(&mut config, Chord::Default, "f5", &["LeftCtrl", "LeftShift", "F5"]);
    assert_eq!(issues_of(&config), vec![]);
}

#[test]
fn a_key_action_on_fn_ctrl_is_one_key_on_modifier_layer_warning() {
    let config = with_layer(Config::default(), Chord::FnCtrl, vec![command("lock"), key("refresh", &["F5"])]);
    assert_eq!(warnings_of(&config), vec![key_on_modifier_layer(Chord::FnCtrl, "refresh")]);
}

#[test]
fn a_key_action_on_every_modifier_layer_is_warned_in_layer_order() {
    let mut config = Config::default();
    for chord in [Chord::FnAlt, Chord::FnShift, Chord::FnSuper, Chord::FnCtrl] {
        config = with_layer(config, chord, vec![command("run"), key("press", &["F5"])]);
    }
    assert_eq!(
        warnings_of(&config),
        vec![
            key_on_modifier_layer(Chord::FnAlt, "press"),
            key_on_modifier_layer(Chord::FnShift, "press"),
            key_on_modifier_layer(Chord::FnSuper, "press"),
            key_on_modifier_layer(Chord::FnCtrl, "press"),
        ]
    );
}

#[test]
fn keys_on_the_default_and_fn_layers_and_commands_on_a_modifier_layer_are_not_warned() {
    let config =
        with_layer(Config::default(), Chord::FnCtrl, vec![command("lock"), command("shot"), command("bg")]);
    assert!(
        config
            .layout
            .layer(Chord::Default)
            .unwrap()
            .buttons()
            .all(|b| matches!(b.action, Action::Key { .. }))
    );
    assert!(
        config.layout.layer(Chord::Fn).unwrap().buttons().all(|b| matches!(b.action, Action::Key { .. }))
    );
    assert_eq!(warnings_of(&config), vec![]);
}

#[test]
fn the_key_on_modifier_layer_warning_names_the_button_and_the_send_shortcut_recipe() {
    let message = key_on_modifier_layer(Chord::FnCtrl, "refresh").to_string();
    assert!(message.contains("refresh"), "{message}");
    assert!(message.contains("send_shortcut"), "{message}");
    assert!(!message.contains('\n'), "one line: {message}");
}

#[test]
fn a_crowded_modifier_layer_lists_capacity_then_key_then_label_warnings() {
    let mut buttons: Vec<Button> = (1..=17).map(|n| command(&format!("c{n}"))).collect();
    buttons[1].label = "far too wide".to_owned();
    buttons[4] = key("first-key", &["F1"]);
    buttons[9] = key("second-key", &["F2"]);
    let config = with_layer(Config::default(), Chord::FnCtrl, buttons);
    let found = warnings(&config, BarSize::MACBOOK_PRO, &SpacingRules::STANDARD, |label| {
        if label == "far too wide" { 1000 } else { 0 }
    });
    assert_eq!(found.len(), 4, "{found:?}");
    assert_eq!(found[0], ConfigWarning::TooManyButtons { chord: Chord::FnCtrl, buttons: 17 });
    assert_eq!(found[1], key_on_modifier_layer(Chord::FnCtrl, "first-key"));
    assert_eq!(found[2], key_on_modifier_layer(Chord::FnCtrl, "second-key"));
    assert!(
        matches!(&found[3], ConfigWarning::LabelTooWide { chord: Chord::FnCtrl, id, width: 1000, .. }
            if id.as_str() == "c2"),
        "{found:?}"
    );
}

#[test]
fn a_key_on_a_modifier_layer_that_does_not_fit_is_still_warned() {
    let mut buttons: Vec<Button> = (1..=31).map(|n| command(&format!("c{n}"))).collect();
    buttons[30] = key("last-key", &["F5"]);
    let config = with_layer(Config::default(), Chord::FnCtrl, buttons);
    assert!(
        issues_of(&config).iter().any(|i| matches!(i, ConfigIssue::Layout { chord: Chord::FnCtrl, .. })),
        "precondition: 31 buttons do not fit"
    );
    assert_eq!(
        warnings_of(&config),
        vec![
            ConfigWarning::TooManyButtons { chord: Chord::FnCtrl, buttons: 31 },
            key_on_modifier_layer(Chord::FnCtrl, "last-key"),
        ]
    );
}
