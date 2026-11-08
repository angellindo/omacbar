//! Functional: the real Rune renderer behind the daemon. A scripted touch at F5's centre presents
//! F5 pressed, and the damage covers every pixel the real renderer changed. Needs a Vulkan adapter
//! (lavapipe is fine). Pixels compare with ±2 per channel. Rects come from `tb_core::place`.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::sync::mpsc;

use tb_core::{BarSize, BarTheme, Chord, Layout, Rect, Rgba, SpacingRules, place, presets};
use tb_daemon::fake::{RecordingDisplay, ScriptedTouches};
use tb_daemon::{Daemon, Event, GpuRenderer, Touch, forward_touches};
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

/// A label far wider than F5 (160 px holds about 11 mono glyphs at 20 px).
const LONG_LABEL: &str = "F5 with a label far too long to fit inside its button on the bar";

fn with_f5_label(label: &str) -> Layout {
    let mut layout = presets::default_layout();
    let f5 = layout
        .layer_mut(Chord::Default)
        .unwrap()
        .groups
        .iter_mut()
        .flat_map(|g| g.buttons.iter_mut())
        .find(|b| b.id.as_str() == "f5")
        .unwrap();
    label.clone_into(&mut f5.label);
    layout
}

/// Runs `layout` through `events` with the real renderer; returns every present.
fn presents(layout: Layout, events: Vec<Event>) -> Vec<(Frame, Vec<Rect>)> {
    let mut d =
        Daemon::new(layout, BarTheme::omarchy_fallback(), GpuRenderer::new(), RecordingDisplay::new());
    d.run(events).unwrap();
    d.display().presents().iter().map(|p| (p.frame.clone(), p.damage.clone())).collect()
}

/// Between each pair of consecutive presents, every pixel that changed (beyond ±2) lies inside
/// the later present's damage, so a panel that shows only the damage matches the frame.
fn assert_every_change_inside_damage(p: &[(Frame, Vec<Rect>)]) {
    for i in 1..p.len() {
        let (before, after, damage) = (&p[i - 1].0, &p[i].0, &p[i].1);
        for y in 0..after.height {
            for x in 0..after.width {
                if !close(before.pixel(x, y), after.pixel(x, y)) {
                    assert!(
                        damage.iter().any(|&r| inside(r, x, y)),
                        "present {i}: ({x}, {y}) changed outside damage {damage:?}"
                    );
                }
            }
        }
    }
}

#[test]
fn pressing_and_releasing_a_button_with_an_overflowing_label_changes_nothing_outside_the_damage() {
    let f5 = f5();
    let p = presents(
        with_f5_label(LONG_LABEL),
        vec![Event::Touch(Touch::Down { x: f5.x + f5.width / 2 }), Event::Touch(Touch::Up)],
    );
    assert_eq!(p.len(), 3, "start, press, release");
    assert_every_change_inside_damage(&p);
}

#[test]
fn shortening_an_overflowing_label_changes_nothing_outside_the_damage() {
    let p = presents(with_f5_label(LONG_LABEL), vec![Event::Layout(with_f5_label("F5"))]);
    assert_eq!(p.len(), 2, "start, then the relabel");
    assert_every_change_inside_damage(&p);
}
