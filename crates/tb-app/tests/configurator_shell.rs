//! Literal Figcraft B geometry and layer navigation through the public Runtime.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod support {
    pub mod configurator;
    pub mod ui;
}
use rune::{Key, Modifier, Mods};
use support::configurator::*;
use tb_core::{Action, BarSize, Button, Chord, Group, SpacingRules};

#[test]
fn the_shell_matches_the_three_columns_and_half_size_footer_canvas() {
    let mut rt = open("literal_shell");
    near(rect(&rt, "sidebar"), [0.0, 0.0, 208.0, 643.0], 1.0);
    near(rect(&rt, "workspace"), [208.0, 0.0, 672.0, 643.0], 1.0);
    near(rect(&rt, "edit-panel-scroll"), [880.0, 0.0, 320.0, 643.0], 1.0);
    near(rect(&rt, "footer"), [0.0, 643.0, 1200.0, 117.0], 1.0);
    near(rect(&rt, "preview"), [57.0, 682.0, 1085.0, 30.0], 1.0);
    for (n, key) in
        ["tab-default", "tab-fn", "tab-fnctrl", "tab-fnalt", "tab-fnshift", "tab-fnsuper"].iter().enumerate()
    {
        near(rect(&rt, key), [20.0, 107.0 + 34.0 * n as f32, 168.0, 34.0], 1.0);
    }
    let header = rect(&rt, "layer-chord");
    assert!((header[0] - 232.0).abs() <= 1.0 && (header[1] - 23.0).abs() <= 3.0);
    near(rect(&rt, "row-a"), [232.0, 122.0, 309.0, 53.0], 2.0);
    operate(&mut rt, "row-a");
    near(rect(&rt, "field-label"), [901.0, 102.0, 279.0, 32.0], 2.0);
    assert_eq!(rt.state.selected.as_ref().unwrap().as_str(), "a");
}

#[test]
fn all_six_layers_are_navigable_and_visiting_an_absent_layer_does_not_create_it() {
    let mut config = fixture();
    config.layout.layers.retain(|l| l.chord == Chord::Default);
    let (mut rt, _) = open_config("absent_navigation", &config, (1200.0, 760.0));
    for key in ["tab-default", "tab-fn", "tab-fnctrl", "tab-fnalt", "tab-fnshift", "tab-fnsuper"] {
        assert!(fully_visible(&rt, key, (1200.0, 760.0)), "{key}");
    }
    operate(&mut rt, "tab-fnalt");
    assert_eq!(rt.state.chord, Chord::FnAlt);
    assert_eq!(rt.state.config, config, "visiting an absent layer is not an edit");
    assert!(words(&rt, "layer-state-fn-alt").to_lowercase().contains("empty"));
    assert!(words(&rt, "layer-counts").contains('0'));
    assert!(shown(&rt, "panel-empty"));
    operate(&mut rt, "add-button");
    let layer = rt.state.config.layout.layer(Chord::FnAlt).unwrap();
    assert_eq!(ids(layer), groups("button-1"));
    assert_eq!(layer.buttons().next().unwrap().action, Action::None);
}

#[test]
fn header_counts_and_resolved_widths_come_from_the_shown_grouped_layer() {
    let rt = open("header_counts");
    let counts = words(&rt, "layer-counts").to_lowercase();
    assert!(counts.contains('5') && counts.contains("button"), "{counts}");
    assert!(counts.contains('3') && counts.contains("group"), "{counts}");
    // 2170 - 176 edges - 56 group gaps - 24 inner gaps = 1914; /5 = 383/382.
    let widths = words(&rt, "layer-widths");
    assert!(widths.contains("382") && widths.contains("383"), "remainder must be honest: {widths}");
}

#[test]
fn selecting_cards_and_preview_keeps_the_edit_panel_and_action_description_together() {
    let mut rt = open("shared_selection");
    operate(&mut rt, "row-c");
    assert_eq!(rt.state.selected.as_ref().unwrap().as_str(), "c");
    assert_eq!(words(&rt, "panel-id"), "c");
    assert!(shown(&rt, "preview-selection-c"));
    assert_ne!(words(&rt, "action-description"), "");
    operate(&mut rt, "preview-a");
    assert_eq!(words(&rt, "panel-id"), "a");
    assert!(shown(&rt, "preview-selection-a"));
    operate(&mut rt, "tab-fn");
    assert_eq!(rt.state.selected, None);
    assert!(shown(&rt, "panel-empty"));
}

#[test]
fn modifiers_never_switch_the_manually_selected_layer_or_button() {
    let mut rt = open("manual_layer");
    operate(&mut rt, "tab-fnctrl");
    operate(&mut rt, "row-legacy");
    for modifier in [Modifier::Fn, Modifier::ControlLeft, Modifier::ShiftLeft, Modifier::AltLeft] {
        press(&mut rt, Key::Modifier { modifier, pressed: true }, Mods { ctrl: true, ..Mods::default() });
        press(&mut rt, Key::Modifier { modifier, pressed: false }, Mods::default());
    }
    assert_eq!(rt.state.chord, Chord::FnCtrl);
    assert_eq!(rt.state.selected.as_ref().unwrap().as_str(), "legacy");
}

#[test]
fn the_preview_keeps_the_shared_default_and_fn_device_placements() {
    let config = tb_core::config::Config::default();
    let (mut rt, _) = open_config("shared_preview", &config, (1200.0, 760.0));
    for (tab, chord) in [("tab-default", Chord::Default), ("tab-fn", Chord::Fn)] {
        operate(&mut rt, tab);
        let scene = tb_render::build_scene(
            config.layout.layer(chord).unwrap(),
            &rt.state.theme,
            BarSize::MACBOOK_PRO,
            &SpacingRules::STANDARD,
            None,
        )
        .unwrap();
        let preview = rect(&rt, "preview");
        for b in scene.buttons {
            near(
                rect(&rt, &format!("preview-{}", b.id)),
                [
                    preview[0] + f32::from(b.rect.x) * 0.5,
                    preview[1] + 4.0,
                    f32::from(b.rect.width) * 0.5,
                    22.0,
                ],
                0.5,
            );
            assert!(shows_text_inside(&rt, &format!("preview-{}", b.id), &b.label));
        }
    }
}

#[test]
fn short_and_narrow_windows_reach_and_operate_fields_groups_and_save_by_real_scrolling() {
    for (n, size) in
        [(1200.0, 760.0), (1200.0, 500.0), (900.0, 600.0), (640.0, 400.0)].into_iter().enumerate()
    {
        let (mut rt, path) = open_config(&format!("reach_{n}"), &fixture(), size);
        for key in ["sidebar-scroll", "workspace-scroll", "edit-panel-scroll", "preview-scroll"] {
            element(&rt, key);
        }
        reveal(&mut rt, "row-e", size);
        tap(&mut rt, "row-e");
        for (key, value) in [
            ("field-label", "edited"),
            ("field-width", "px 180"),
            ("field-fill", "accent"),
            ("field-text", "foreground"),
            ("field-border", "red"),
        ] {
            reveal(&mut rt, key, size);
            type_into(&mut rt, key, value);
            assert!(element(&rt, key).focused, "{key} accepts focus at {size:?}");
        }
        reveal(&mut rt, "action-command", size);
        tap(&mut rt, "action-command");
        reveal(&mut rt, "field-action", size);
        type_into(&mut rt, "field-action", "printf reachable");
        reveal(&mut rt, "save", size);
        tap(&mut rt, "save");
        assert_eq!(disk(&path), rt.state.config, "save operable at {size:?}");
        assert_eq!(button(&rt, Chord::Default, "e").label, "edited");
        assert_eq!(rect(&rt, "preview")[2], 1085.0, "native 50% preview remains scrollable");
        reveal(&mut rt, "remove-button", size);
        tap(&mut rt, "remove-button");
        assert!(
            rt.state.config.layout.layer(Chord::Default).unwrap().buttons().all(|b| b.id.as_str() != "e")
        );
    }
}

#[test]
fn long_group_content_scrolls_to_the_last_card_without_overlapping_the_edit_panel() {
    let mut config = fixture();
    let layer = config.layout.layer_mut(Chord::Default).unwrap();
    layer.groups = (0..10)
        .map(|n| Group { buttons: vec![Button::new(&format!("long-{n}"), &"W".repeat(200), Action::None)] })
        .collect();
    let (mut rt, _) = open_config("long_workspace", &config, (1200.0, 760.0));
    reveal(&mut rt, "row-long-9", (1200.0, 760.0));
    tap(&mut rt, "row-long-9");
    assert_eq!(rt.state.selected.as_ref().unwrap().as_str(), "long-9");
    let card = element(&rt, "row-long-9");
    assert!(card.visible[0] >= 232.0 && card.visible[0] + card.visible[2] <= 857.0);
    assert_eq!(rect(&rt, "footer")[1], 643.0);
}

#[test]
fn shell_ink_and_background_follow_two_real_omarchy_themes() {
    for (n, source) in [
        include_str!("../../../fixtures/omarchy-themes/tokyo-night/colors.toml"),
        include_str!("../../../fixtures/omarchy-themes/catppuccin-latte/colors.toml"),
    ]
    .into_iter()
    .enumerate()
    {
        let theme = tb_core::BarTheme::from_omarchy_colors(source).unwrap();
        let path = fresh_dir("configurator_contract", &format!("theme_{n}")).join("config.toml");
        std::fs::write(&path, tb_core::config::to_toml(&fixture()).unwrap()).unwrap();
        let rt = runtime(tb_app::State::open(Some(path), theme.clone()), (1200.0, 760.0));
        assert_eq!(element(&rt, "editor").fill, tb_render::view::color(theme.background));
        assert_eq!(element(&rt, "layer-chord").text_color, Some(tb_render::view::color(theme.button_text)));
        assert_eq!(element(&rt, "editor").radius, 0.0);
        near(rect(&rt, "footer"), [0.0, 643.0, 1200.0, 117.0], 1.0);
    }
}
