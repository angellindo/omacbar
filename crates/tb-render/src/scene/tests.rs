use super::*;
use tb_core::{Chord, presets};

fn scene(pressed: Option<&ButtonId>) -> BarScene {
    let layout = presets::default_layout();
    let theme = BarTheme::omarchy_fallback();
    build_scene(
        layout.layer(Chord::FnCtrl).unwrap(),
        &theme,
        BarSize::MACBOOK_PRO,
        &SpacingRules::STANDARD,
        pressed,
    )
    .unwrap()
}

#[test]
fn idle_buttons_use_theme_roles_unless_overridden() {
    let theme = BarTheme::omarchy_fallback();
    let s = scene(None);
    let shot = s.buttons.iter().find(|b| b.id.as_str() == "screenshot").unwrap();
    assert_eq!((shot.fill, shot.text), (theme.button_fill, theme.button_text));
    let lock = s.buttons.iter().find(|b| b.id.as_str() == "lock").unwrap();
    assert_eq!(lock.fill, theme.resolve("red").unwrap(), "lock overrides its fill with the `red` token");
}

#[test]
fn the_pressed_button_uses_the_pressed_roles_even_when_overridden() {
    let theme = BarTheme::omarchy_fallback();
    let lock = ButtonId::new("lock");
    let s = scene(Some(&lock));
    let b = s.buttons.iter().find(|b| b.id == lock).unwrap();
    assert_eq!((b.fill, b.text), (theme.pressed_fill, theme.pressed_text));
}

#[test]
fn a_label_with_line_breaks_or_tabs_is_shown_on_one_line() {
    let mut layout = presets::default_layout();
    let layer = layout.layer_mut(Chord::Default).unwrap();
    let f5 =
        layer.groups.iter_mut().flat_map(|g| g.buttons.iter_mut()).find(|b| b.id.as_str() == "f5").unwrap();
    f5.label = "F5\nF5\r\nF5\tF5\rF5".to_owned();
    let s = build_scene(
        layout.layer(Chord::Default).unwrap(),
        &BarTheme::omarchy_fallback(),
        BarSize::MACBOOK_PRO,
        &SpacingRules::STANDARD,
        None,
    )
    .unwrap();
    let shown = s.buttons.iter().find(|b| b.id.as_str() == "f5").unwrap();
    assert_eq!(shown.label, "F5 F5  F5 F5 F5");
}
