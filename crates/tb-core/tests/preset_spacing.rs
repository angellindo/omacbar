//! Placement contract for the photographed bar's spacing, rounded to the standard's grid.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use tb_core::config::{Config, parse, to_toml, validate};
use tb_core::hit::hit;
use tb_core::{Action, BarSize, BarTheme, Chord, LayoutError, SpacingRules, place, presets};

const TWO_GROUPS: &str = r#"
version = 1
[[layer]]
chord = "fn"
GAP
[[layer.group]]
[[layer.group.button]]
id = "a"
label = "a"
width = { px = 100 }
[[layer.group]]
[[layer.group]]
[[layer.group.button]]
id = "b"
label = "b"
width = { px = 100 }
"#;

fn measured_rules() -> SpacingRules {
    SpacingRules { edge_padding: 88, gap: 12, group_gap: 88, ..SpacingRules::STANDARD }
}

#[test]
fn standard_spacing_uses_wide_edges_and_separates_media_groups() {
    let rules = SpacingRules::STANDARD;
    assert_eq!((rules.edge_padding, rules.gap, rules.group_gap), (88, 12, 88));
    assert_eq!((rules.vertical_inset, rules.min_button_width, rules.label_padding), (8, 64, 12));
    assert_eq!(rules.touch_slop(), 6);
}

#[test]
fn the_default_preset_has_a_separate_escape_and_even_f_keys_between_equal_edges() {
    let layout = presets::default_layout();
    let placed =
        place(layout.layer(Chord::Default).unwrap(), BarSize::MACBOOK_PRO, &SpacingRules::STANDARD).unwrap();
    assert_eq!(placed.len(), 13);
    assert_eq!((placed[0].id.as_str(), placed[0].rect.x, placed[0].rect.width), ("esc", 88, 116));
    assert_eq!(placed[1].rect.x, 232);
    for (index, button) in placed[1..].iter().enumerate() {
        assert_eq!(button.id.as_str(), format!("f{}", index + 1));
        assert_eq!(button.rect.width, if index < 2 { 144 } else { 143 });
        assert_eq!((button.rect.y, button.rect.height), (8, 44));
        if index > 0 {
            assert_eq!(button.rect.x - placed[index].rect.right(), 12);
        }
    }
    assert_eq!(placed.last().unwrap().rect.right(), 2082);
    let saved = to_toml(&Config::default()).unwrap();
    assert!(saved.contains("group-gap = 28"), "the preset's gap must be saved");
    let restored = parse(&saved).unwrap();
    assert_eq!(restored, Config::default());
}

#[test]
fn media_buttons_keep_natural_widths_in_five_groups_and_leave_a_dead_trailing_space() {
    let layout = presets::default_layout();
    let layer = layout.layer(Chord::Fn).unwrap();
    assert_eq!(layer.groups.iter().map(|g| g.buttons.len()).collect::<Vec<_>>(), [1, 2, 2, 3, 3]);
    let placed = place(layer, BarSize::MACBOOK_PRO, &SpacingRules::STANDARD).unwrap();
    let expected = [88, 292, 436, 656, 800, 1020, 1164, 1308, 1528, 1672, 1816];
    assert_eq!(placed.iter().map(|p| p.rect.x).collect::<Vec<_>>(), expected);
    assert_eq!(placed[0].rect.width, 116);
    assert!(placed[1..].iter().all(|p| p.rect.width == 132));
    assert_eq!(placed.last().unwrap().rect.right(), 1948);
    for (id, key) in [("kbd-light-down", "KbdIllumDown"), ("kbd-light-up", "KbdIllumUp")] {
        let button = layer.buttons().find(|b| b.id.as_str() == id).expect("keyboard-light button");
        assert_eq!(button.action, Action::Key { keys: vec![key.to_owned()] });
    }
    for x in [0, 87, 248, 612, 976, 1484, 1948, 2000, 2169] {
        assert_eq!(hit(&placed, x, &SpacingRules::STANDARD), None, "dead space at {x}");
    }
    assert_eq!(hit(&placed, 429, &SpacingRules::STANDARD).unwrap().as_str(), "brightness-down");
    assert_eq!(hit(&placed, 431, &SpacingRules::STANDARD).unwrap().as_str(), "brightness-up");
}

#[test]
fn a_saved_layer_group_gap_is_generic_skips_empty_groups_and_round_trips() {
    let config = parse(&TWO_GROUPS.replace("GAP", "group-gap = 28"))
        .expect("the config supports a layer group-gap override");
    let layer = config.layout.layer(Chord::Fn).unwrap();
    let placed = place(layer, BarSize::MACBOOK_PRO, &measured_rules()).unwrap();
    assert_eq!((placed[0].rect.x, placed[1].rect.x), (88, 216));
    assert_eq!((placed[0].group, placed[1].group), (0, 2));
    let saved = to_toml(&config).unwrap();
    assert!(saved.contains("group-gap = 28"));
    assert_eq!(parse(&saved).unwrap(), config);
}

#[test]
fn group_gap_override_controls_width_preflight_as_well_as_placement() {
    let size = BarSize { width: 416, height: 60 };
    let small_gap =
        parse(&TWO_GROUPS.replace("GAP", "group-gap = 28")).expect("a layer can override its group gap");
    let placed = place(small_gap.layout.layer(Chord::Fn).unwrap(), size, &measured_rules()).unwrap();
    assert_eq!(placed[1].rect.right(), 316);
    let large_gap = parse(&TWO_GROUPS.replace("GAP", "group-gap = 100")).unwrap();
    assert_eq!(
        place(large_gap.layout.layer(Chord::Fn).unwrap(), size, &measured_rules()),
        Err(LayoutError::Overflow { needed: 300, available: 240 })
    );
}

#[test]
fn old_config_without_group_gap_keeps_fixed_widths_and_inherits_spacing_rules() {
    let text = TWO_GROUPS.replace("GAP", "").replace("\"fn\"", "\"default\"");
    let config = parse(&text).unwrap();
    let rules = measured_rules();
    assert_eq!(config.version, 1);
    assert_eq!(
        validate(&config, BarSize::MACBOOK_PRO, &rules, &BarTheme::omarchy_fallback()),
        Vec::<tb_core::config::ConfigIssue>::new()
    );
    let placed = place(config.layout.layer(Chord::Default).unwrap(), BarSize::MACBOOK_PRO, &rules).unwrap();
    assert_eq!(placed.iter().map(|p| (p.rect.x, p.rect.width)).collect::<Vec<_>>(), [(88, 100), (276, 100)]);
    let saved = to_toml(&config).unwrap();
    assert!(!saved.contains("group-gap"), "omitted overrides stay omitted");
    assert_eq!(parse(&saved).unwrap(), config);
}
