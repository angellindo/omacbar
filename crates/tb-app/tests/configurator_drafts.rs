//! Safe edit-panel buffers, field diagnostics and modifier-layer policy.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod support {
    pub mod configurator;
    pub mod ui;
}
use support::configurator::*;
use tb_core::{Action, BarSize, Chord, SpacingRules, Width};

#[test]
fn label_room_uses_resolved_device_width_and_shared_font_measurement() {
    let mut rt = open("label_room");
    operate(&mut rt, "row-a");
    edit(&mut rt, "field-width", "px 180");
    edit(&mut rt, "field-label", "WWWWWWWWWWWWWWWWWWWW");
    let full = tb_render::label_width("WWWWWWWWWWWWWWWWWWWW", rt.state.theme.font_size);
    let meter = words(&rt, "label-room");
    assert!(meter.contains("156"), "180 - 24 device pixels: {meter}");
    assert!(meter.contains(&full.to_string()), "measured shared font width {full}: {meter}");
    assert!(words(&rt, "width-resolved").contains("180"));
    let warning = words(&rt, "warning-0");
    assert!(warning.contains('a') && warning.contains("156"), "{warning}");
    assert!(!disabled(&rt, "save"), "a long label is a warning");
}

#[test]
fn width_mode_numeric_fields_edit_span_and_pixels_and_keep_full_string_entry() {
    let mut rt = open("width_modes");
    operate(&mut rt, "row-a");
    operate(&mut rt, "width-span");
    edit(&mut rt, "field-width-value", "2");
    assert_eq!(button(&rt, Chord::Default, "a").width, Width::Span(2));
    assert!(words(&rt, "width-resolved").contains("638"), "1914 / 6 * 2 = 638");
    operate(&mut rt, "width-px");
    edit(&mut rt, "field-width-value", "180");
    assert_eq!(button(&rt, Chord::Default, "a").width, Width::Px(180));
    assert_eq!(words(&rt, "field-width"), "px 180");
    operate(&mut rt, "width-auto");
    assert_eq!(button(&rt, Chord::Default, "a").width, Width::Auto);
    edit(&mut rt, "field-width", "span 3");
    assert_eq!(button(&rt, Chord::Default, "a").width, Width::Span(3));
}

#[test]
fn numeric_zero_and_overflow_drafts_are_never_silently_clamped_or_saved_as_old_widths() {
    let mut rt = open("numeric_invalid");
    operate(&mut rt, "row-a");
    operate(&mut rt, "width-span");
    edit(&mut rt, "field-width-value", "0");
    assert_eq!(words(&rt, "field-width-value"), "0");
    assert_blocked(&rt, &["a"]);
    edit(&mut rt, "field-width-value", "99999999999999999999");
    assert_eq!(words(&rt, "field-width-value"), "99999999999999999999");
    assert_blocked(&rt, &["a"]);
    edit(&mut rt, "field-width", "auto");
    assert!(!disabled(&rt, "save"));
}

#[test]
fn two_buttons_and_two_layers_keep_independent_invalid_width_buffers_until_each_is_repaired() {
    let mut rt = open("independent_width_buffers");
    operate(&mut rt, "row-a");
    edit(&mut rt, "field-width", "bad-a-default");
    operate(&mut rt, "row-b");
    edit(&mut rt, "field-width", "bad-b-default");
    operate(&mut rt, "tab-fn");
    operate(&mut rt, "row-a");
    edit(&mut rt, "field-width", "bad-a-fn");
    operate(&mut rt, "tab-default");
    operate(&mut rt, "row-a");
    assert_eq!(words(&rt, "field-width"), "bad-a-default");
    edit(&mut rt, "field-width", "auto");
    operate(&mut rt, "row-b");
    assert_eq!(words(&rt, "field-width"), "bad-b-default");
    assert_blocked(&rt, &["b", "Fn"]);
    operate(&mut rt, "remove-button");
    assert_blocked(&rt, &["Fn", "a"]);
    operate(&mut rt, "tab-fn");
    operate(&mut rt, "row-a");
    assert_eq!(words(&rt, "field-width"), "bad-a-fn");
    edit(&mut rt, "field-width", "auto");
    assert!(!disabled(&rt, "save"), "all three drafts now repaired or removed");
}

#[test]
fn key_and_command_switches_keep_separate_text_buffers_and_none_keeps_the_command_memory() {
    let mut rt = open("safe_action_buffers");
    operate(&mut rt, "row-a");
    operate(&mut rt, "action-key");
    edit(&mut rt, "field-action", "LeftCtrl + F5");
    operate(&mut rt, "action-command");
    assert_eq!(words(&rt, "field-action"), "", "key names must never become a shell command");
    assert_blocked(&rt, &["a"]);
    edit(&mut rt, "field-action", "printf buffer-safe");
    operate(&mut rt, "action-key");
    assert_eq!(words(&rt, "field-action"), "LeftCtrl + F5");
    assert_eq!(
        button(&rt, Chord::Default, "a").action,
        Action::Key { keys: vec!["LeftCtrl".into(), "F5".into()] }
    );
    operate(&mut rt, "action-command");
    assert_eq!(words(&rt, "field-action"), "printf buffer-safe");
    operate(&mut rt, "action-none");
    assert_eq!(button(&rt, Chord::Default, "a").action, Action::None);
    assert!(!shown(&rt, "field-action"));
    assert!(!disabled(&rt, "save"));
    operate(&mut rt, "action-command");
    assert_eq!(words(&rt, "field-action"), "printf buffer-safe");
}

#[test]
fn ordinary_modifier_buttons_cannot_newly_select_key_but_legacy_keys_and_escape_stay_readable() {
    let mut rt = open("modifier_policy");
    operate(&mut rt, "tab-fnctrl");
    operate(&mut rt, "row-ordinary");
    assert!(disabled(&rt, "action-key"));
    let policy = words(&rt, "action-policy").to_lowercase();
    assert!(
        policy.contains("modifier") && (policy.contains("combine") || policy.contains("leak")),
        "{policy}"
    );
    operate(&mut rt, "action-key");
    assert_eq!(button(&rt, Chord::FnCtrl, "ordinary").action, Action::None);
    for (id, expected) in [("legacy", "F5"), ("esc", "Esc")] {
        operate(&mut rt, &format!("row-{id}"));
        assert_eq!(words(&rt, "field-action"), expected);
        assert!(all_words(&rt).contains("modifier"));
        assert_eq!(
            tb_core::config::validate(
                &rt.state.config,
                BarSize::MACBOOK_PRO,
                &SpacingRules::STANDARD,
                &rt.state.theme
            ),
            Vec::new()
        );
    }
    operate(&mut rt, "row-legacy");
    operate(&mut rt, "action-none");
    operate(&mut rt, "action-key");
    assert_eq!(
        button(&rt, Chord::FnCtrl, "legacy").action,
        Action::None,
        "leaving legacy keys cannot manufacture a new key action"
    );
}

#[test]
fn default_and_fn_still_allow_key_actions_with_shared_unknown_key_diagnostics() {
    let mut rt = open("normal_key_policy");
    for tab in ["tab-default", "tab-fn"] {
        operate(&mut rt, tab);
        operate(&mut rt, "row-a");
        operate(&mut rt, "action-key");
        edit(&mut rt, "field-action", "F5");
        assert_eq!(button(&rt, rt.state.chord, "a").action, Action::Key { keys: vec!["F5".into()] });
        edit(&mut rt, "field-action", "NoSuchKey");
        assert!(all_words(&rt).contains("NoSuchKey"));
        edit(&mut rt, "field-action", "F5");
    }
}

#[test]
fn selected_invalid_cards_and_preview_have_distinct_error_and_accent_decorations() {
    let mut rt = open("error_and_selection");
    operate(&mut rt, "row-a");
    edit(&mut rt, "field-label", "");
    for key in ["preview-selection-a", "preview-error-a", "card-selection-a", "card-error-a"] {
        let e = element(&rt, key);
        assert!(e.opacity > 0.0 && e.visible[2] > 0.0 && e.visible[3] > 0.0, "{key} is visible");
    }
    let selected = element(&rt, "preview-selection-a").rect;
    let invalid = element(&rt, "preview-error-a").rect;
    assert_ne!(selected, invalid, "inner error and outer selection both have their own geometry");
    let layer = rt.state.config.layout.layer(Chord::Default).unwrap();
    let scene =
        tb_render::build_scene(layer, &rt.state.theme, BarSize::MACBOOK_PRO, &SpacingRules::STANDARD, None)
            .unwrap();
    let a = scene.buttons.iter().find(|b| b.id.as_str() == "a").unwrap();
    assert_eq!(a.fill, rt.state.theme.button_fill);
    assert_eq!(a.border, rt.state.theme.button_border, "editor-only decoration never becomes daemon state");
    assert_ne!(words(&rt, "button-issues"), "");
}

#[test]
fn fill_text_and_border_diagnostics_and_hex_notices_keep_the_shared_theme_semantics() {
    let mut rt = open("color_diagnostics");
    operate(&mut rt, "row-a");
    for key in ["field-fill", "field-text", "field-border"] {
        edit(&mut rt, key, "unknown-color");
    }
    let issues = (0..3).map(|n| words(&rt, &format!("issue-{n}"))).collect::<Vec<_>>();
    assert_eq!(issues.len(), 3);
    assert!(issues.iter().all(|i| i.contains("unknown-color") && i.contains('a')));
    for (field, note) in
        [("field-fill", "hex-fill"), ("field-text", "hex-text"), ("field-border", "hex-border")]
    {
        edit(&mut rt, field, "#112233");
        assert!(shown(&rt, note));
        edit(&mut rt, field, "");
        assert!(!shown(&rt, note));
    }
    let b = button(&rt, Chord::Default, "a");
    assert_eq!(b.style, tb_core::ButtonStyle::default());
}

#[test]
fn all_shared_warning_kinds_are_visible_and_remain_nonblocking() {
    let mut config = fixture();
    config.layout.layer_mut(Chord::Default).unwrap().groups = vec![tb_core::Group {
        buttons: (0..17)
            .map(|n| {
                tb_core::Button::new(
                    &format!("many-{n}"),
                    if n == 0 { "a label far too wide for this button room" } else { "ok" },
                    Action::None,
                )
            })
            .collect(),
    }];
    let (mut rt, path) = open_config("all_warning_kinds", &config, (1200.0, 760.0));
    let warnings = (0..4).map(|n| words(&rt, &format!("warning-{n}"))).collect::<Vec<_>>().join(" ");
    assert!(warnings.contains("17") && warnings.contains("16"), "capacity warning: {warnings}");
    assert!(warnings.contains("ellips") && warnings.contains("many-0"), "measured label warning: {warnings}");
    assert!(
        warnings.contains("legacy") && warnings.contains("esc") && warnings.contains("modifier"),
        "legacy warnings: {warnings}"
    );
    assert!(!disabled(&rt, "save"));
    operate(&mut rt, "save");
    assert_eq!(disk(&path), config);
}
