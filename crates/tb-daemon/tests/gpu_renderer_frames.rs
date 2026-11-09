//! touchbard's GPU renderer draws scene after scene with one kept renderer, and every frame is
//! byte-identical to `render_frame` of the same scene. Needs a Vulkan adapter (lavapipe is fine).
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use tb_core::{BarSize, BarTheme, ButtonId, Chord, SpacingRules, presets};
use tb_daemon::{GpuRenderer, SceneRenderer};
use tb_render::{BarScene, build_scene, render_frame};

fn scene(chord: Chord, pressed: Option<&str>) -> BarScene {
    let layout = presets::default_layout();
    let pressed = pressed.map(ButtonId::new);
    build_scene(
        layout.layer(chord).unwrap(),
        &BarTheme::omarchy_fallback(),
        BarSize::MACBOOK_PRO,
        &SpacingRules::STANDARD,
        pressed.as_ref(),
    )
    .unwrap()
}

fn start() -> Result<GpuRenderer, tb_render::RenderError> {
    GpuRenderer::new()
}

#[test]
fn the_gpu_renderer_draws_successive_scenes_byte_identical_to_render_frame() {
    let scenes = [
        ("default", scene(Chord::Default, None)),
        ("default, F5 pressed", scene(Chord::Default, Some("f5"))),
        ("fn", scene(Chord::Fn, None)),
        ("fn-ctrl", scene(Chord::FnCtrl, None)),
        ("default again", scene(Chord::Default, None)),
    ];
    let mut gpu = start().expect("a Vulkan adapter (lavapipe is fine)");
    for (what, s) in &scenes {
        let got = gpu.render(s);
        assert_eq!((got.width, got.height), (2170, 60), "{what}: frame size");
        assert!(got == render_frame(s.clone()), "{what}: the GPU renderer's frame differs from render_frame");
    }
}
