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
        group_gap: None,
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

#[test]
fn a_label_padding_wider_than_half_a_button_leaves_no_room_instead_of_underflowing() {
    let rules = SpacingRules { label_padding: 1000, ..SpacingRules::STANDARD };
    let config = Config::default();
    let found = warnings(&config, BarSize::MACBOOK_PRO, &rules, |_| 1);
    let buttons: usize = config.layout.layers.iter().map(|l| l.buttons().count()).sum();
    assert_eq!(found.len(), buttons, "every label warns: {found:?}");
    assert!(
        found.iter().all(|w| matches!(w, ConfigWarning::LabelTooWide { width: 1, room: 0, .. })),
        "{found:?}"
    );
}

#[test]
fn the_default_for_a_theme_without_red_drops_only_the_red_override() {
    let minimal = BarTheme::from_omarchy_colors(
        "background = \"#101418\"\nforeground = \"#d0d4d8\"\naccent = \"#4c8cf0\"\n",
    )
    .unwrap();
    let adapted = Config::default_for(&minimal);
    let mut expected = Config::default();
    for b in expected.layout.layers.iter_mut().flat_map(|l| &mut l.groups).flat_map(|g| &mut g.buttons) {
        if b.style.fill.as_deref() == Some("red") {
            b.style.fill = None;
        }
    }
    assert_ne!(expected, Config::default(), "the shipped default uses red");
    assert_eq!(adapted, expected);
    assert_eq!(validate(&adapted, BarSize::MACBOOK_PRO, &SpacingRules::STANDARD, &minimal), vec![]);
}

#[test]
fn the_default_for_a_full_theme_is_the_shipped_default() {
    assert_eq!(Config::default_for(&BarTheme::omarchy_fallback()), Config::default());
}
