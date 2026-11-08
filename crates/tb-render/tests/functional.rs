//! Functional tests: the real Rune runtime, headless, on a virtual clock.
//! Element-tree assertions run everywhere; pixel assertions need a GPU adapter (lavapipe is fine).

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)] // test-only helpers

use rune::{Env, Runtime};
use tb_core::{BarSize, BarTheme, Chord, SpacingRules, presets};
use tb_render::{BarScene, bar_view, build_scene, render_frame};

fn scene(chord: Chord) -> BarScene {
    let layout = presets::default_layout();
    build_scene(
        layout.layer(chord).unwrap(),
        &BarTheme::omarchy_fallback(),
        BarSize::MACBOOK_PRO,
        &SpacingRules::STANDARD,
        None,
    )
    .unwrap()
}

#[test]
fn the_default_layer_presents_every_f_key_where_the_layout_engine_put_it() {
    let s = scene(Chord::Default);
    let mut rt = Runtime::new(s.clone(), bar_view, (2170.0, 60.0), Env::new(true, 1.0));
    rt.frame(0.0);
    for b in &s.buttons {
        let (rect, opacity) = rt.presented(b.id.as_str()).unwrap_or_else(|| panic!("{} not presented", b.id));
        assert_eq!(
            rect,
            [f32::from(b.rect.x), f32::from(b.rect.y), f32::from(b.rect.width), f32::from(b.rect.height)]
        );
        assert!((opacity - 1.0).abs() < f32::EPSILON);
    }
    assert!(rt.presented("f12").is_some() && rt.presented("esc").is_some());
}

#[test]
fn the_offscreen_frame_is_the_panel_size_and_paints_theme_colours() {
    let s = scene(Chord::Default);
    let theme = BarTheme::omarchy_fallback();
    let frame = render_frame(s.clone());
    assert_eq!((frame.width, frame.height), (2170, 60));
    assert_eq!(frame.rgba.len(), 2170 * 60 * 4);
    let bg = frame.pixel(2, 30);
    let want = theme.background;
    for (got, want) in bg.iter().zip([want.r, want.g, want.b]) {
        assert!(got.abs_diff(want) <= 2, "background pixel {bg:?} vs theme {want:?}");
    }
    if std::env::var_os("TB_WRITE_SNAPSHOTS").is_some() {
        frame.write_png(std::path::Path::new("../../out/default-layer.png")).unwrap();
    }
}

#[test]
fn a_label_too_long_for_its_button_stays_inside_the_padded_button() {
    let mut layout = presets::default_layout();
    let f5 = layout
        .layer_mut(Chord::Default)
        .unwrap()
        .groups
        .iter_mut()
        .flat_map(|g| g.buttons.iter_mut())
        .find(|b| b.id.as_str() == "f5")
        .unwrap();
    f5.label = "a label far too long to fit inside one button".to_owned();
    let rules = SpacingRules::STANDARD;
    let s = build_scene(
        layout.layer(Chord::Default).unwrap(),
        &BarTheme::omarchy_fallback(),
        BarSize::MACBOOK_PRO,
        &rules,
        None,
    )
    .unwrap();
    let mut rt = Runtime::new(s.clone(), bar_view, (2170.0, 60.0), Env::new(true, 1.0));
    rt.frame(0.0);

    let button = s.buttons.iter().find(|b| b.id.as_str() == "f5").unwrap().rect;
    let ([x, _, width, _], _) = rt.presented("f5-label").unwrap();
    let pad = f32::from(rules.label_padding);
    assert!(width > 0.0, "the label is shown");
    assert!(x >= f32::from(button.x) + pad - 0.5, "label starts at {x}, button {button:?}");
    assert!(
        x + width <= f32::from(button.right()) - pad + 0.5,
        "label ends at {}, button {button:?}",
        x + width
    );
}
