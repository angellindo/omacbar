//! A pressed button takes the button standard's pressed look (accent fill, background label,
//! accent border) on that button alone, drawn at rest in the frame of the press. The element tree
//! is read from the real `bar_view` in a Rune `Runtime` (no GPU); the border is read from
//! `render_frame` pixels (GPU adapter, lavapipe is fine), ±2 per channel. Expected colours are the
//! Tokyo Night literals of `BarTheme::omarchy_fallback()`.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use rune::{Color, ElementInfo, Env, Runtime};
use tb_core::{BarSize, BarTheme, ButtonId, Chord, Rect, SpacingRules, presets};
use tb_render::{BarScene, Frame, bar_view, build_scene, render_frame};

const ACCENT: &str = "#7aa2f7";
const BACKGROUND: &str = "#1a1b26";
const FOREGROUND: &str = "#a9b1d6";
/// Idle fill: the foreground at alpha 10/255.
const IDLE_FILL: &str = "#a9b1d60a";

/// The default layer in the fallback theme, with `pressed` (if any) in its pressed state.
fn scene(pressed: Option<&str>) -> BarScene {
    let layout = presets::default_layout();
    let pressed = pressed.map(ButtonId::new);
    build_scene(
        layout.layer(Chord::Default).unwrap(),
        &BarTheme::omarchy_fallback(),
        BarSize::MACBOOK_PRO,
        &SpacingRules::STANDARD,
        pressed.as_ref(),
    )
    .unwrap()
}

fn elements(pressed: Option<&str>) -> Vec<ElementInfo> {
    let mut rt: Runtime<BarScene> =
        Runtime::new(scene(pressed), bar_view, (2170.0, 60.0), Env::new(true, 1.0));
    rt.frame(0.0);
    rt.elements()
}

fn element<'a>(all: &'a [ElementInfo], key: &str) -> &'a ElementInfo {
    all.iter().find(|e| e.key.as_deref() == Some(key)).unwrap_or_else(|| panic!("no element {key}"))
}

fn assert_linear(got: Color, want: &str, what: &str) {
    let w = rune::color::hex(want).unwrap();
    let close =
        [(got.r, w.r), (got.g, w.g), (got.b, w.b), (got.a, w.a)].iter().all(|(g, w)| (g - w).abs() <= 1e-4);
    assert!(close, "{what}: got {got:?}, want {want} = {w:?}");
}

fn text_color(all: &[ElementInfo], key: &str) -> Color {
    element(all, key).text_color.unwrap_or_else(|| panic!("{key} has no text colour"))
}

fn rect(s: &BarScene, id: &str) -> Rect {
    s.buttons.iter().find(|b| b.id.as_str() == id).unwrap_or_else(|| panic!("no button {id}")).rect
}

fn near(got: [u8; 4], want: [u8; 3]) -> bool {
    got.iter().zip(want).all(|(g, w)| g.abs_diff(w) <= 2)
}

fn rgb(hex: &str) -> [u8; 3] {
    let h = hex.trim_start_matches('#');
    let p = |i: usize| u8::from_str_radix(&h[i..i + 2], 16).unwrap();
    [p(0), p(2), p(4)]
}

#[test]
fn a_pressed_button_takes_the_accent_fill_and_a_background_label() {
    let pressed = elements(Some("f5"));
    assert_linear(element(&pressed, "f5").fill, ACCENT, "pressed f5 fill");
    assert_linear(text_color(&pressed, "f5-label"), BACKGROUND, "pressed f5 label");

    let idle = elements(None);
    assert_linear(element(&idle, "f5").fill, IDLE_FILL, "idle f5 fill");
    assert_linear(text_color(&idle, "f5-label"), FOREGROUND, "idle f5 label");
}

#[test]
fn pressing_a_button_changes_no_other_elements_colours() {
    let idle = elements(None);
    let pressed = elements(Some("f5"));
    let mut compared = 0;
    for e in &idle {
        if matches!(e.key.as_deref(), Some("f5" | "f5-label")) {
            continue;
        }
        let other = pressed
            .iter()
            .find(|p| p.path == e.path)
            .unwrap_or_else(|| panic!("pressed tree has no element at {}", e.path));
        assert_eq!(other.fill, e.fill, "fill of {} ({:?})", e.path, e.key);
        assert_eq!(other.text_color, e.text_color, "text colour of {} ({:?})", e.path, e.key);
        compared += 1;
    }
    assert!(compared >= 24, "compared only {compared} elements");

    let others =
        std::iter::once("esc".to_owned()).chain((1..=12).filter(|n| *n != 5).map(|n| format!("f{n}")));
    for id in others {
        assert_linear(element(&pressed, &id).fill, IDLE_FILL, &format!("{id} fill while f5 is pressed"));
        assert_linear(text_color(&pressed, &format!("{id}-label")), FOREGROUND, &format!("{id}-label"));
    }
}

#[test]
fn a_pressed_button_is_bordered_in_accent_and_its_neighbours_keep_their_borders() {
    let s = scene(Some("f5"));
    let (f4, f5, f6) = (rect(&s, "f4"), rect(&s, "f5"), rect(&s, "f6"));
    let pressed: Frame = render_frame(s);
    let idle: Frame = render_frame(scene(None));
    let accent = rgb(ACCENT);

    let (left, right) = (u32::from(f5.x), u32::from(f5.right()) - 1);
    let (top, bottom) = (u32::from(f5.y), u32::from(f5.y + f5.height) - 1);
    let (mid_x, mid_y) = (u32::midpoint(left, right), u32::midpoint(top, bottom));
    for (what, x, y) in
        [("left", left, mid_y), ("right", right, mid_y), ("top", mid_x, top), ("bottom", mid_x, bottom)]
    {
        let got = pressed.pixel(x, y);
        assert!(near(got, accent), "f5 {what} edge at ({x}, {y}): got {got:?}, want {ACCENT} ± 2");
    }

    for (id, r) in [("f4", f4), ("f6", f6)] {
        let x = u32::from(r.x);
        let (got, was) = (pressed.pixel(x, 30), idle.pixel(x, 30));
        assert!(
            near(got, [was[0], was[1], was[2]]),
            "{id} left edge at ({x}, 30): pressed {got:?}, idle {was:?}"
        );
        assert!(!near(got, accent), "{id} left edge at ({x}, 30) is accent {got:?} while only f5 is pressed");
    }
}
