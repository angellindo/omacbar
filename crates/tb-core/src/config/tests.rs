use super::*;
use crate::model::{Action, Button, Group, Layer};

fn check(config: &Config) -> Vec<ConfigIssue> {
    validate(config, BarSize::MACBOOK_PRO, &SpacingRules::STANDARD, &BarTheme::omarchy_fallback())
}

#[test]
fn the_default_config_is_valid() {
    assert_eq!(check(&Config::default()), vec![]);
}

#[test]
fn the_default_config_round_trips_through_toml() {
    let config = Config::default();
    let text = to_toml(&config).unwrap();
    assert_eq!(parse(&text).unwrap(), config, "\n{text}");
}

#[test]
fn parses_a_hand_written_config() {
    let config = parse(
        r##"
version = 1
theme = { source = "omarchy" }

[[layer]]
chord = "default"

[[layer.group]]
[[layer.group.button]]
id = "esc"
label = "esc"
width = { px = 120 }
action = { type = "key", keys = ["Esc"] }

[[layer.group]]
[[layer.group.button]]
id = "term"
label = "term"
width = { span = 2 }
style = { fill = "accent", text = "#1a1b26" }
action = { type = "command", run = "alacritty" }
"##,
    )
    .unwrap();
    assert_eq!(check(&config), vec![]);
    assert_eq!(
        config.layout.layers[0].groups[1].buttons[0].action,
        Action::Command { run: "alacritty".into() }
    );
}

#[test]
fn unknown_fields_are_rejected_not_ignored() {
    let err = parse("version = 1\n[[layer]]\nchord = \"default\"\ncolour = \"red\"").unwrap_err();
    assert!(err.to_string().contains("colour"), "{err}");
}

#[test]
fn reports_every_problem_at_once() {
    let bad = |id: &str, label: &str| {
        let mut b = Button::new(id, label, Action::None);
        b.style.fill = Some("chartreuse".into());
        b
    };
    let fn_layer = Layer {
        chord: Chord::Fn,
        groups: vec![Group { buttons: vec![bad("Bad_Id", " "), bad("Bad_Id", "x")] }],
    };
    let config = Config {
        version: 2,
        theme: ThemeSource::Omarchy,
        layout: Layout { layers: vec![fn_layer.clone(), fn_layer] },
    };
    let issues = check(&config);
    assert!(issues.contains(&ConfigIssue::Version(2)));
    assert!(issues.contains(&ConfigIssue::NoDefaultLayer));
    assert!(issues.contains(&ConfigIssue::DuplicateLayer(Chord::Fn)));
    assert!(issues.iter().any(|i| matches!(i, ConfigIssue::BadId { .. })));
    assert!(issues.iter().any(|i| matches!(i, ConfigIssue::DuplicateId { .. })));
    assert!(issues.iter().any(|i| matches!(i, ConfigIssue::EmptyLabel { .. })));
    assert!(issues.iter().any(|i| matches!(i, ConfigIssue::UnknownColor { .. })));
}
