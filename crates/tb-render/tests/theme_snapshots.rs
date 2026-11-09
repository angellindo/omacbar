//! Snapshots of the default layer, F5 pressed, in two Omarchy themes read from the fixtures
//! (copied from Omarchy's `themes/<name>/colors.toml`; never the live system theme). Pixels need a
//! GPU adapter (lavapipe is fine) and compare opaque colours with ±2 per channel. With
//! `TB_WRITE_SNAPSHOTS=1` they write `out/tokyo-night.png` and `out/catppuccin-latte.png`.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::path::Path;

use tb_core::{BarSize, BarTheme, ButtonId, Chord, SpacingRules, presets};
use tb_render::{Frame, build_scene, render_frame};

fn fixture_theme(theme: &str) -> BarTheme {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../fixtures/omarchy-themes")
        .join(theme)
        .join("colors.toml");
    let text = std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
    BarTheme::from_omarchy_colors(&text).unwrap()
}

/// The default layer in `theme` with F5 pressed, and F5's left edge x.
fn snapshot(theme: &str) -> (Frame, u32) {
    let layout = presets::default_layout();
    let f5 = ButtonId::new("f5");
    let scene = build_scene(
        layout.layer(Chord::Default).unwrap(),
        &fixture_theme(theme),
        BarSize::MACBOOK_PRO,
        &SpacingRules::STANDARD,
        Some(&f5),
    )
    .unwrap();
    let left = u32::from(scene.buttons.iter().find(|b| b.id == f5).unwrap().rect.x);
    let frame = render_frame(scene);
    assert_eq!((frame.width, frame.height), (2170, 60));
    if std::env::var_os("TB_WRITE_SNAPSHOTS").is_some() {
        let out = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../out");
        std::fs::create_dir_all(&out).unwrap();
        frame.write_png(&out.join(format!("{theme}.png"))).unwrap();
    }
    (frame, left)
}

fn assert_colour(frame: &Frame, (x, y): (u32, u32), want: [u8; 3], what: &str) {
    let got = frame.pixel(x, y);
    assert!(
        got.iter().zip(want).all(|(g, w)| g.abs_diff(w) <= 2),
        "{what} at ({x}, {y}): got {got:?}, want {want:?} ± 2"
    );
}

#[test]
fn the_tokyo_night_snapshot_paints_its_background_and_the_pressed_accent() {
    let (frame, f5) = snapshot("tokyo-night");
    assert_colour(&frame, (2, 30), [0x1a, 0x1b, 0x26], "bar background");
    assert_colour(&frame, (f5 + 4, 30), [0x7a, 0xa2, 0xf7], "pressed F5 fill (accent)");
}

#[test]
fn the_catppuccin_latte_snapshot_paints_its_background_and_the_pressed_accent() {
    let (frame, f5) = snapshot("catppuccin-latte");
    assert_colour(&frame, (2, 30), [0xef, 0xf1, 0xf5], "bar background");
    assert_colour(&frame, (f5 + 4, 30), [0x1e, 0x66, 0xf5], "pressed F5 fill (accent)");
}
