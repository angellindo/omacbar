//! A touch at x hits a button through `tb_core::hit`; the daemon presents that button pressed with
//! damage = that button's rect only. CPU only (`FlatRenderer`). Every bar rect and x comes from
//! `tb_core::place` on the default preset.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use tb_core::hit::hit;
use tb_core::{BarSize, BarTheme, Chord, Placed, Rect, Rgba, SpacingRules, place, presets};
use tb_daemon::fake::{FlatRenderer, Presented, RecordingDisplay};
use tb_daemon::{Daemon, Event, SimDisplay, Touch};
use tb_render::Frame;

const BAR: BarSize = BarSize::MACBOOK_PRO;
const RULES: SpacingRules = SpacingRules::STANDARD;

fn placed() -> Vec<Placed> {
    place(presets::default_layout().layer(Chord::Default).unwrap(), BAR, &RULES).unwrap()
}

fn rect(id: &str) -> Rect {
    placed().iter().find(|p| p.id.as_str() == id).unwrap().rect
}

fn centre_x(r: Rect) -> u16 {
    r.x + r.width / 2
}

fn down(x: u16) -> Event {
    Event::Touch(Touch::Down { x })
}

fn bytes(c: Rgba) -> [u8; 4] {
    [c.r, c.g, c.b, c.a]
}

/// Runs the default preset with the fallback theme through `events`; returns every present.
fn run(events: Vec<Event>) -> Vec<Presented> {
    let mut d = Daemon::new(
        presets::default_layout(),
        BarTheme::omarchy_fallback(),
        FlatRenderer,
        RecordingDisplay::new(),
    );
    d.run(events).unwrap();
    d.display()
        .presents()
        .iter()
        .map(|p| Presented { frame: p.frame.clone(), damage: p.damage.clone() })
        .collect()
}

/// Every pixel inside `r` is `rgba`.
fn assert_filled(frame: &Frame, r: Rect, rgba: [u8; 4], what: &str) {
    for y in r.y..r.y + r.height {
        for x in r.x..r.right() {
            assert_eq!(frame.pixel(u32::from(x), u32::from(y)), rgba, "{what} at ({x}, {y})");
        }
    }
}

#[test]
fn a_touch_down_at_f5_centre_presents_only_f5_pressed() {
    let theme = BarTheme::omarchy_fallback();
    let (f4, f5) = (rect("f4"), rect("f5"));
    let p = run(vec![down(centre_x(f5))]);

    assert_eq!(p.len(), 2, "start, then the press");
    assert_eq!(p[1].damage, [f5]);
    assert_filled(&p[1].frame, f5, bytes(theme.pressed_fill), "F5");
    assert_filled(&p[1].frame, f4, bytes(theme.button_fill), "F4");
}

#[test]
fn touch_up_presents_f5_released_with_damage_f5() {
    let theme = BarTheme::omarchy_fallback();
    let f5 = rect("f5");
    let p = run(vec![down(centre_x(f5)), Event::Touch(Touch::Up)]);

    assert_eq!(p.len(), 3);
    assert_eq!(p[2].damage, [f5]);
    assert_filled(&p[2].frame, f5, bytes(theme.button_fill), "F5");
    assert_eq!(p[2].frame, p[0].frame, "back to the start frame");
}

#[test]
fn a_touch_in_a_dead_zone_presents_nothing() {
    let (esc, f1, f12) = (rect("esc"), rect("f1"), rect("f12"));
    let group_gap = esc.right() + (f1.x - esc.right()) / 2;
    let right_edge = f12.right() + (2170 - f12.right()) / 2;
    for x in [0, group_gap, right_edge] {
        assert_eq!(hit(&placed(), x, &RULES), None, "precondition: x {x} is dead");
    }
    let p = run(vec![down(0), down(group_gap), down(right_edge)]);
    assert_eq!(p.len(), 1, "only the start present");
}

#[test]
fn a_touch_in_the_gap_within_slop_of_f4_presses_f4() {
    let theme = BarTheme::omarchy_fallback();
    let f4 = rect("f4");
    let x = f4.right() + 3;
    assert_eq!(
        hit(&placed(), x, &RULES).map(|id| id.as_str().to_owned()).as_deref(),
        Some("f4"),
        "precondition"
    );

    let p = run(vec![down(x)]);
    assert_eq!(p.len(), 2);
    assert_eq!(p[1].damage, [f4]);
    assert_filled(&p[1].frame, f4, bytes(theme.pressed_fill), "F4");
}

#[test]
fn moving_the_press_from_f5_to_f6_presents_both_rects() {
    let theme = BarTheme::omarchy_fallback();
    let (f5, f6) = (rect("f5"), rect("f6"));
    let p = run(vec![down(centre_x(f5)), down(centre_x(f6))]);

    assert_eq!(p.len(), 3);
    let mut damage = p[2].damage.clone();
    damage.sort_by_key(|r| r.x);
    assert_eq!(damage, [f5, f6]);
    assert_filled(&p[2].frame, f5, bytes(theme.button_fill), "F5");
    assert_filled(&p[2].frame, f6, bytes(theme.pressed_fill), "F6");
}

#[test]
fn touching_the_button_already_pressed_presents_nothing() {
    let f5 = rect("f5");
    let p = run(vec![down(centre_x(f5)), down(f5.x + 1)]);
    assert_eq!(p.len(), 2);
}

#[test]
fn touch_up_with_nothing_pressed_presents_nothing() {
    let p = run(vec![Event::Touch(Touch::Up)]);
    assert_eq!(p.len(), 1);
}

#[test]
fn on_the_simulator_a_press_changes_the_screen_only_inside_f5() {
    let f5 = rect("f5");
    let dir = std::path::Path::new(env!("CARGO_TARGET_TMPDIR")).join("touch_press_sim");
    let _ = std::fs::remove_dir_all(&dir);
    let sim = SimDisplay::new(BAR, &dir).unwrap();
    let mut d = Daemon::new(presets::default_layout(), BarTheme::omarchy_fallback(), FlatRenderer, sim);
    d.run(Vec::new()).unwrap();
    let before = d.display().screen().clone();

    d.handle(down(centre_x(f5))).unwrap();
    let after = d.display().screen();
    let mut changed = 0;
    for y in 0..after.height {
        for x in 0..after.width {
            if after.pixel(x, y) != before.pixel(x, y) {
                changed += 1;
                let inside = x >= u32::from(f5.x)
                    && x < u32::from(f5.right())
                    && y >= u32::from(f5.y)
                    && y < u32::from(f5.y + f5.height);
                assert!(inside, "pixel ({x}, {y}) changed outside F5 {f5:?}");
            }
        }
    }
    assert_eq!(changed, u32::from(f5.width) * u32::from(f5.height), "all of F5 shows pressed");
    assert!(dir.join("frame-0002.png").is_file());
}
