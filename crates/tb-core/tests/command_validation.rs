//! Command validity is shared by file loading, saving and the configurator.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
use tb_core::config::{Config, parse_valid, to_toml, validate, warnings};
use tb_core::{Action, BarSize, BarTheme, Button, Chord, Group, Layer, SpacingRules};

fn config() -> Config {
    let mut config = Config::default();
    config.layout.layers = vec![
        Layer {
            chord: Chord::Default,
            group_gap: Some(28),
            groups: vec![Group {
                buttons: vec![
                    Button::new("blank", "blank", Action::Command { run: String::new() }),
                    Button::new("none", "none", Action::None),
                ],
            }],
        },
        Layer {
            chord: Chord::FnCtrl,
            group_gap: None,
            groups: vec![Group {
                buttons: vec![Button::new("spaces", "spaces", Action::Command { run: " \t\n ".into() })],
            }],
        },
    ];
    config
}

#[test]
fn empty_and_whitespace_commands_report_each_chord_and_button_in_shared_validation() {
    let issues =
        validate(&config(), BarSize::MACBOOK_PRO, &SpacingRules::STANDARD, &BarTheme::omarchy_fallback());
    assert_eq!(issues.len(), 2, "each empty command is invalid, none is valid: {issues:?}");
    let messages: Vec<String> = issues.iter().map(ToString::to_string).collect();
    for (chord, id) in [("Default", "blank"), ("FnCtrl", "spaces")] {
        assert!(
            messages
                .iter()
                .any(|s| s.contains(chord) && s.contains(id) && s.to_lowercase().contains("command")),
            "{messages:?}"
        );
    }
}

#[test]
fn version_one_parse_rejects_empty_commands_and_accepts_none_without_executing_any_action() {
    let mut config = config();
    let rejected = parse_valid(
        &to_toml(&config).unwrap(),
        BarSize::MACBOOK_PRO,
        &SpacingRules::STANDARD,
        &BarTheme::omarchy_fallback(),
    );
    let issues = rejected.expect_err("shared load path rejects blank command files");
    assert_eq!(issues.len(), 2);
    for layer in &mut config.layout.layers {
        for button in layer.groups.iter_mut().flat_map(|g| &mut g.buttons) {
            button.action = Action::None;
        }
    }
    let accepted = parse_valid(
        &to_toml(&config).unwrap(),
        BarSize::MACBOOK_PRO,
        &SpacingRules::STANDARD,
        &BarTheme::omarchy_fallback(),
    )
    .unwrap();
    assert_eq!(accepted, config);
    assert_eq!(accepted.version, 1);
}

#[test]
fn legacy_modifier_keys_remain_valid_and_report_nonblocking_shared_warnings() {
    let mut config = config();
    for layer in &mut config.layout.layers {
        for button in layer.groups.iter_mut().flat_map(|g| &mut g.buttons) {
            button.action = Action::None;
        }
    }
    let modifier = config.layout.layer_mut(Chord::FnCtrl).unwrap();
    modifier.groups[0].buttons = vec![
        Button::new("esc", "esc", Action::Key { keys: vec!["Esc".into()] }),
        Button::new("legacy", "legacy", Action::Key { keys: vec!["F5".into()] }),
    ];
    assert_eq!(
        validate(&config, BarSize::MACBOOK_PRO, &SpacingRules::STANDARD, &BarTheme::omarchy_fallback()),
        vec![]
    );
    let found = warnings(&config, BarSize::MACBOOK_PRO, &SpacingRules::STANDARD, |_| 20);
    assert_eq!(found.len(), 2);
    for id in ["esc", "legacy"] {
        assert!(found.iter().any(|w| matches!(w, tb_core::config::ConfigWarning::KeyOnModifierLayer { chord: Chord::FnCtrl, id: actual } if actual.as_str() == id)));
    }
}
