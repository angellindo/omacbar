//! Damage through the daemon's public seam: which rects a change presents, recorded by the fake
//! display. Complements the frozen contract (render_on_change, touch_press) with the cases where
//! the bar's look changes but its geometry does not, and the reverse.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use tb_core::{BarSize, BarTheme, Button, ButtonId, Chord, Layout, Rect, Rgba, SpacingRules, place, presets};
use tb_daemon::fake::{FlatRenderer, RecordingDisplay};
use tb_daemon::{Daemon, Event};

const WHOLE_BAR: Rect = Rect { x: 0, y: 0, width: 2170, height: 60 };

fn rects(layout: &Layout) -> Vec<Rect> {
    let layer = layout.layer(Chord::Default).unwrap();
    place(layer, BarSize::MACBOOK_PRO, &SpacingRules::STANDARD).unwrap().into_iter().map(|p| p.rect).collect()
}

fn rect_of(layout: &Layout, id: &str) -> Rect {
    let layer = layout.layer(Chord::Default).unwrap();
    let placed = place(layer, BarSize::MACBOOK_PRO, &SpacingRules::STANDARD).unwrap();
    placed.iter().find(|p| p.id.as_str() == id).unwrap().rect
}

fn f5_mut(layout: &mut Layout) -> &mut Button {
    let layer = layout.layer_mut(Chord::Default).unwrap();
    layer.groups.iter_mut().flat_map(|g| g.buttons.iter_mut()).find(|b| b.id.as_str() == "f5").unwrap()
}

/// The damage of the present that `event` causes after the start present; `None` if none.
fn damage_of(event: Event) -> Option<Vec<Rect>> {
    let mut d = Daemon::new(
        presets::default_layout(),
        BarTheme::omarchy_fallback(),
        FlatRenderer,
        RecordingDisplay::new(),
    );
    d.run(vec![event]).unwrap();
    let p = d.display().presents();
    assert!(p.len() <= 2, "one event presents at most once, got {}", p.len());
    p.get(1).map(|p| p.damage.clone())
}

#[test]
fn a_text_colour_override_on_f5_damages_only_f5() {
    let mut layout = presets::default_layout();
    f5_mut(&mut layout).style.text = Some("red".to_owned());
    assert_eq!(damage_of(Event::Layout(layout.clone())), Some(vec![rect_of(&layout, "f5")]));
}

#[test]
fn a_border_override_on_f5_damages_only_f5() {
    let mut layout = presets::default_layout();
    f5_mut(&mut layout).style.border = Some("red".to_owned());
    assert_eq!(damage_of(Event::Layout(layout.clone())), Some(vec![rect_of(&layout, "f5")]));
}

#[test]
fn a_new_button_text_colour_damages_every_button_but_not_the_gaps() {
    let mut theme = BarTheme::omarchy_fallback();
    theme.button_text = Rgba::opaque(1, 2, 3);
    assert_eq!(damage_of(Event::Theme(theme)), Some(rects(&presets::default_layout())));
}

#[test]
fn a_new_border_width_damages_the_whole_bar() {
    let mut theme = BarTheme::omarchy_fallback();
    theme.border_width += 1;
    assert_eq!(damage_of(Event::Theme(theme)), Some(vec![WHOLE_BAR]));
}

#[test]
fn a_new_font_size_damages_the_whole_bar() {
    let mut theme = BarTheme::omarchy_fallback();
    theme.font_size -= 2;
    assert_eq!(damage_of(Event::Theme(theme)), Some(vec![WHOLE_BAR]));
}

#[test]
fn a_new_corner_radius_damages_the_whole_bar() {
    let mut theme = BarTheme::omarchy_fallback();
    theme.radius += 4;
    assert_eq!(damage_of(Event::Theme(theme)), Some(vec![WHOLE_BAR]));
}

#[test]
fn renaming_a_button_in_place_damages_the_whole_bar() {
    let mut layout = presets::default_layout();
    f5_mut(&mut layout).id = ButtonId::new("f5-renamed");
    assert_eq!(damage_of(Event::Layout(layout)), Some(vec![WHOLE_BAR]));
}

#[test]
fn removing_a_button_damages_the_whole_bar() {
    let mut layout = presets::default_layout();
    layout.layer_mut(Chord::Default).unwrap().groups[1].buttons.pop();
    assert_eq!(damage_of(Event::Layout(layout)), Some(vec![WHOLE_BAR]));
}

#[test]
fn an_equal_layout_presents_nothing() {
    assert_eq!(damage_of(Event::Layout(presets::default_layout())), None);
}
