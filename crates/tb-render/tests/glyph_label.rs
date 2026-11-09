//! A Nerd Font glyph label (U+F028, `nf-fa-volume_up`) renders on the Fn layer's `volume-up`
//! button. These cases hold whether or not the system monospace font has the glyph: with
//! `JetBrainsMono Nerd Font` it draws the speaker icon, without it the font's missing-glyph box;
//! either way the label is presented inside its room and nothing outside the button changes.
//! Pixels need a GPU adapter (lavapipe is fine), ±2 per channel. With `TB_WRITE_SNAPSHOTS=1` the
//! render is written to `out/glyph-label.png`.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::path::Path;

use rune::{Env, Runtime};
use tb_core::{BarSize, BarTheme, Chord, Rect, SpacingRules, presets};
use tb_render::{BarScene, Frame, bar_view, build_scene, label_width, render_frame};

const GLYPH: &str = "\u{f028}";
/// The theme's font size: 20 px (button standard § Label).
const FONT: u16 = 20;
/// Room kept on each side of a label (`SpacingRules::STANDARD.label_padding`).
const PADDING: u16 = 12;

/// The Fn layer in the fallback theme with `volume-up` labelled `label`, nothing pressed.
fn scene(label: &str) -> BarScene {
    let mut layout = presets::default_layout();
    let layer = layout.layer_mut(Chord::Fn).unwrap();
    let button = layer
        .groups
        .iter_mut()
        .flat_map(|g| g.buttons.iter_mut())
        .find(|b| b.id.as_str() == "volume-up")
        .unwrap();
    label.clone_into(&mut button.label);
    let theme = BarTheme::omarchy_fallback();
    assert_eq!(theme.font_size, FONT);
    build_scene(layout.layer(Chord::Fn).unwrap(), &theme, BarSize::MACBOOK_PRO, &SpacingRules::STANDARD, None)
        .unwrap()
}

fn volume_up(s: &BarScene) -> Rect {
    s.buttons.iter().find(|b| b.id.as_str() == "volume-up").unwrap().rect
}

fn differs(a: [u8; 4], b: [u8; 4], by: u8) -> bool {
    a.iter().zip(b).take(3).any(|(x, y)| x.abs_diff(y) > by)
}

#[test]
fn a_glyph_label_is_presented_inside_its_label_room_at_its_measured_width() {
    let s = scene(GLYPH);
    let r = volume_up(&s);
    let mut rt: Runtime<BarScene> = Runtime::new(s, bar_view, (2170.0, 60.0), Env::new(true, 1.0));
    rt.frame(0.0);

    let label = rt
        .elements()
        .into_iter()
        .find(|e| e.key.as_deref() == Some("volume-up-label"))
        .expect("volume-up-label is in the tree");
    assert_eq!(label.text.as_deref(), Some(GLYPH));

    let ([x, _, w, _], _) = rt.presented("volume-up-label").expect("volume-up-label is presented");
    let measured = label_width(GLYPH, FONT) as f32;
    assert!(w > 0.0, "drawn width {w}");
    assert!((w - measured).abs() <= 1.0, "drawn width {w}, measured {measured} ± 1");
    let (room_left, room_right) = (f32::from(r.x + PADDING), f32::from(r.right() - PADDING));
    assert!(x >= room_left - 0.5, "label x {x} left of the room's {room_left}");
    assert!(x + w <= room_right + 0.5, "label right {} past the room's {room_right}", x + w);
}

#[test]
fn a_glyph_label_draws_a_mark_inside_its_button_and_nothing_outside_it() {
    let s = scene(GLYPH);
    let r = volume_up(&s);
    let glyph: Frame = render_frame(s);
    let text: Frame = render_frame(scene("vol+"));
    assert_eq!((glyph.width, glyph.height), (2170, 60));
    if std::env::var_os("TB_WRITE_SNAPSHOTS").is_some() {
        let out = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../out");
        std::fs::create_dir_all(&out).unwrap();
        glyph.write_png(&out.join("glyph-label.png")).unwrap();
    }

    // Ink is searched across the whole label room: a Nerd Font icon is wider than its advance.
    let fill = glyph.pixel(u32::from(r.x) + 4, 30);
    let (left, right) = (u32::from(r.x + PADDING), u32::from(r.right() - PADDING));
    let (top, bottom) = (u32::from(r.y) + 2, u32::from(r.y + r.height) - 2);
    let inked = (top..bottom).any(|y| (left..right).any(|x| differs(glyph.pixel(x, y), fill, 16)));
    assert!(inked, "no mark in volume-up's label room x {left}..{right}, y {top}..{bottom} (fill {fill:?})");

    let inside = |x: u32, y: u32| {
        u16::try_from(x).is_ok_and(|x| r.contains_x(x))
            && y >= u32::from(r.y)
            && y < u32::from(r.y + r.height)
    };
    for y in 0..glyph.height {
        for x in 0..glyph.width {
            if !inside(x, y) {
                let (g, t) = (glyph.pixel(x, y), text.pixel(x, y));
                assert!(!differs(g, t, 2), "({x}, {y}) outside volume-up: glyph {g:?}, `vol+` {t:?}");
            }
        }
    }
}
