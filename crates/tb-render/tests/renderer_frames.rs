//! A kept renderer: one GPU device, pipeline and offscreen target draw scene after scene, and
//! every frame is byte-identical to `render_frame` of the same scene (both drawn in this process,
//! on the same adapter, so this is no golden image). Needs a Vulkan adapter (lavapipe is fine).
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::path::Path;

use tb_core::{BarSize, BarTheme, ButtonId, Chord, SpacingRules, presets};
use tb_render::{BarScene, Renderer, build_scene, render_frame};

fn fixture_theme(theme: &str) -> BarTheme {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../fixtures/omarchy-themes")
        .join(theme)
        .join("colors.toml");
    let text = std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
    BarTheme::from_omarchy_colors(&text).unwrap()
}

fn scene(chord: Chord, theme: &BarTheme, pressed: Option<&str>) -> BarScene {
    let layout = presets::default_layout();
    let pressed = pressed.map(ButtonId::new);
    build_scene(
        layout.layer(chord).unwrap(),
        theme,
        BarSize::MACBOOK_PRO,
        &SpacingRules::STANDARD,
        pressed.as_ref(),
    )
    .unwrap()
}

#[test]
fn one_renderer_draws_successive_scenes_byte_identical_to_render_frame() {
    let omarchy = BarTheme::omarchy_fallback();
    let latte = fixture_theme("catppuccin-latte");
    let scenes = [
        ("default", scene(Chord::Default, &omarchy, None)),
        ("fn", scene(Chord::Fn, &omarchy, None)),
        ("fn-ctrl", scene(Chord::FnCtrl, &omarchy, None)),
        ("default, F5 pressed", scene(Chord::Default, &omarchy, Some("f5"))),
        ("default, catppuccin-latte", scene(Chord::Default, &latte, None)),
        ("default again", scene(Chord::Default, &omarchy, None)),
    ];
    let mut renderer = Renderer::new().expect("a Vulkan adapter (lavapipe is fine)");
    let mut previous = None;
    for (what, s) in &scenes {
        let got = renderer.render(s);
        assert_eq!((got.width, got.height), (2170, 60), "{what}: frame size");
        let want = render_frame(s.clone());
        if got != want {
            let differing = got.rgba.iter().zip(&want.rgba).filter(|(a, b)| a != b).count();
            panic!("{what}: kept renderer differs from render_frame in {differing} bytes");
        }
        if let Some(before) = previous.replace(got.clone()) {
            assert_ne!(got, before, "{what}: same frame as the scene before it (stale frame?)");
        }
    }
}

#[test]
fn a_scene_of_another_size_gets_a_frame_of_its_size() {
    let full = scene(Chord::Default, &BarTheme::omarchy_fallback(), None);
    let mut small = full.clone();
    small.size = BarSize { width: 1000, height: 40 };
    let mut renderer = Renderer::new().expect("a Vulkan adapter (lavapipe is fine)");

    let first = renderer.render(&full);
    assert_eq!((first.width, first.height), (2170, 60));
    assert!(first == render_frame(full.clone()), "2170 x 60 frame differs from render_frame");

    let shrunk = renderer.render(&small);
    assert_eq!((shrunk.width, shrunk.height), (1000, 40));
    assert_eq!(shrunk.rgba.len(), 1000 * 40 * 4);
    assert!(shrunk == render_frame(small.clone()), "1000 x 40 frame differs from render_frame");

    let back = renderer.render(&full);
    assert_eq!((back.width, back.height), (2170, 60));
    assert!(
        back == render_frame(full.clone()),
        "2170 x 60 frame after the small one differs from render_frame"
    );
}
