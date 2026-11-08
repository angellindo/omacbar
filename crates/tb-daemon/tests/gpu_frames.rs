//! Functional: the real Rune renderer behind the daemon. A scripted touch at F5's centre presents
//! F5 pressed, and the damage covers every pixel the real renderer changed. Needs a Vulkan adapter
//! (lavapipe is fine). Pixels compare with ±2 per channel. Rects come from `tb_core::place`.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::sync::mpsc;

use tb_core::{BarSize, BarTheme, Chord, Rect, Rgba, SpacingRules, place, presets};
use tb_daemon::fake::{RecordingDisplay, ScriptedTouches};
use tb_daemon::{Daemon, GpuRenderer, Touch, forward_touches};
use tb_render::Frame;

const BAR: BarSize = BarSize::MACBOOK_PRO;

fn f5() -> Rect {
    let layout = presets::default_layout();
    let placed = place(layout.layer(Chord::Default).unwrap(), BAR, &SpacingRules::STANDARD).unwrap();
    placed.iter().find(|p| p.id.as_str() == "f5").unwrap().rect
}

fn close(a: [u8; 4], b: [u8; 4]) -> bool {
    a.iter().zip(b).all(|(x, y)| x.abs_diff(y) <= 2)
}

fn inside(r: Rect, x: u32, y: u32) -> bool {
    x >= u32::from(r.x) && x < u32::from(r.right()) && y >= u32::from(r.y) && y < u32::from(r.y + r.height)
}

/// A point of F5 away from its border and its centred label.
fn f5_fill_point(r: Rect) -> (u32, u32) {
    (u32::from(r.x + 12), u32::from(r.y + r.height / 2))
}

/// Start + a touch down at F5's centre, delivered as the binary does: touch source → channel → run.
fn press_f5() -> Vec<(Frame, Vec<Rect>)> {
    let f5 = f5();
    let (tx, rx) = mpsc::channel();
    let mut touches = ScriptedTouches::new([Touch::Down { x: f5.x + f5.width / 2 }]);
    forward_touches(&mut touches, &tx).unwrap();
    drop(tx);

    let mut d = Daemon::new(
        presets::default_layout(),
        BarTheme::omarchy_fallback(),
        GpuRenderer::new(),
        RecordingDisplay::new(),
    );
    d.run(rx).unwrap();
    d.display().presents().iter().map(|p| (p.frame.clone(), p.damage.clone())).collect()
}

fn bytes(c: Rgba) -> [u8; 4] {
    [c.r, c.g, c.b, c.a]
}

#[test]
fn a_touch_at_f5_centre_presents_a_real_frame_with_f5_pressed() {
    let theme = BarTheme::omarchy_fallback();
    let f5 = f5();
    let p = press_f5();

    assert_eq!(p.len(), 2, "start, then the press");
    assert_eq!(p[1].1, [f5]);
    let (frame, (x, y)) = (&p[1].0, f5_fill_point(f5));
    assert_eq!((frame.width, frame.height), (2170, 60));
    let got = frame.pixel(x, y);
    assert!(close(got, bytes(theme.pressed_fill)), "F5 at ({x}, {y}) = {got:?}, want pressed_fill");
    let before = p[0].0.pixel(x, y);
    assert!(!close(before, got), "F5 was not pressed in the start frame ({before:?})");
}

#[test]
fn every_pixel_the_press_changes_lies_inside_its_damage() {
    let p = press_f5();
    assert_eq!(p.len(), 2);
    let (first, second, damage) = (&p[0].0, &p[1].0, &p[1].1);
    let mut changed = 0u32;
    for y in 0..second.height {
        for x in 0..second.width {
            if !close(first.pixel(x, y), second.pixel(x, y)) {
                changed += 1;
                assert!(
                    damage.iter().any(|&r| inside(r, x, y)),
                    "({x}, {y}) changed outside damage {damage:?}"
                );
            }
        }
    }
    assert!(changed > 0, "the press changed some pixels");
}
