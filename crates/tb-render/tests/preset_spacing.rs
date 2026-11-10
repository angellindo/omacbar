//! Real Rune placement follows the same measured preset geometry as the layout engine.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use rune::{Env, Runtime};
use tb_core::{BarSize, BarTheme, Chord, SpacingRules, presets};
use tb_render::{bar_view, build_scene};

#[test]
fn rune_presents_the_f_row_with_its_edge_margins_and_escape_gap() {
    let layout = presets::default_layout();
    let scene = build_scene(
        layout.layer(Chord::Default).unwrap(),
        &BarTheme::omarchy_fallback(),
        BarSize::MACBOOK_PRO,
        &SpacingRules::STANDARD,
        None,
    )
    .unwrap();
    let mut rt = Runtime::new(scene, bar_view, (2170.0, 60.0), Env::new(true, 1.0));
    rt.frame(0.0);
    assert_eq!(rt.presented("esc").unwrap().0, [88.0, 8.0, 116.0, 44.0]);
    assert_eq!(rt.presented("f1").unwrap().0, [232.0, 8.0, 144.0, 44.0]);
    assert_eq!(rt.presented("f12").unwrap().0, [1939.0, 8.0, 143.0, 44.0]);
}

#[test]
fn rune_presents_natural_media_groups_without_filling_the_right_edge() {
    let layout = presets::default_layout();
    let scene = build_scene(
        layout.layer(Chord::Fn).unwrap(),
        &BarTheme::omarchy_fallback(),
        BarSize::MACBOOK_PRO,
        &SpacingRules::STANDARD,
        None,
    )
    .unwrap();
    assert!(scene.buttons.iter().all(|button| button.rect.right() <= 1948));
    let mut rt = Runtime::new(scene, bar_view, (2170.0, 60.0), Env::new(true, 1.0));
    rt.frame(0.0);
    for (id, x) in
        [("brightness-down", 292.0), ("kbd-light-down", 656.0), ("previous", 1020.0), ("mute", 1528.0)]
    {
        assert_eq!(rt.presented(id).unwrap().0, [x, 8.0, 132.0, 44.0], "{id}");
    }
    assert_eq!(rt.presented("volume-up").unwrap().0, [1816.0, 8.0, 132.0, 44.0]);
}
