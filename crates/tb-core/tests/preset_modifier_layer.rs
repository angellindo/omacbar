//! The default preset follows the modifier-layer rule (ADR-0011): its modifier layers press no
//! keys, so the default config has no warning, and an exact key there is a `send_shortcut`
//! command. It still follows the button standard (§ Escape): every layer starts with `esc`.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use tb_core::config::{Config, parse_valid, to_toml};
use tb_core::{Action, BarSize, BarTheme, Button, Chord, Group, Layer, SpacingRules, Width, presets};

const MODIFIER_LAYERS: [Chord; 4] = [Chord::FnCtrl, Chord::FnAlt, Chord::FnShift, Chord::FnSuper];

fn modifier_layers_of_the_preset() -> Vec<Layer> {
    let layout = presets::default_layout();
    let found: Vec<Layer> =
        layout.layers.into_iter().filter(|l| MODIFIER_LAYERS.contains(&l.chord)).collect();
    assert!(!found.is_empty(), "the preset has a modifier layer (fn-ctrl)");
    found
}

#[test]
fn every_layer_of_the_default_preset_starts_with_esc_alone_at_116_px() {
    for layer in presets::default_layout().layers {
        let first = &layer.groups[0].buttons;
        assert_eq!(first.len(), 1, "{:?}: esc is alone in the first group", layer.chord);
        assert_eq!(first[0].id.as_str(), "esc", "{:?}", layer.chord);
        assert_eq!(first[0].width, Width::Px(116), "{:?}", layer.chord);
    }
}

#[test]
fn no_modifier_layer_of_the_default_preset_presses_keys() {
    for layer in modifier_layers_of_the_preset() {
        for button in layer.buttons() {
            assert!(
                !matches!(button.action, Action::Key { .. }),
                "{:?}: `{}` presses keys on a modifier layer",
                layer.chord,
                button.id
            );
        }
    }
}

#[test]
fn the_esc_of_a_modifier_layer_in_the_default_preset_sends_escape_through_a_command() {
    for layer in modifier_layers_of_the_preset() {
        let esc: &Button = layer.buttons().find(|b| b.id.as_str() == "esc").unwrap();
        match &esc.action {
            Action::Command { run } => {
                assert!(run.contains("send_shortcut"), "{:?}: {run}", layer.chord);
                assert!(run.to_lowercase().contains("escape"), "{:?}: {run}", layer.chord);
            }
            other => panic!("{:?}: esc must be a command, got {other:?}", layer.chord),
        }
    }
}

#[test]
fn a_key_action_on_a_modifier_layer_is_a_warning_and_still_validates() {
    let mut config = Config::default();
    let key = Button::new("refresh", "F5", Action::Key { keys: vec!["F5".to_owned()] });
    config.layout.layers.retain(|l| l.chord != Chord::FnCtrl);
    config.layout.layers.push(Layer {
        group_gap: None,
        chord: Chord::FnCtrl,
        groups: vec![Group { buttons: vec![key] }],
    });
    let text = to_toml(&config).unwrap();
    let checked =
        parse_valid(&text, BarSize::MACBOOK_PRO, &SpacingRules::STANDARD, &BarTheme::omarchy_fallback());
    assert_eq!(checked, Ok(config));
}
