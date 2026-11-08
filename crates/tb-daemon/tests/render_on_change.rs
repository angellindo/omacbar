//! The daemon renders and presents only when the scene changes, with the damage of that change;
//! otherwise it stays idle. CPU only: `FlatRenderer` paints flat rects, `RecordingDisplay`
//! records each present. Every bar rect comes from `tb_core::place` on the default preset.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::sync::mpsc;

use tb_core::{
    BarSize, BarTheme, Button, ButtonId, Chord, Layout, Modifiers, Rect, Rgba, SpacingRules, Width, place,
    presets,
};
use tb_daemon::fake::{FlatRenderer, Presented, RecordingDisplay};
use tb_daemon::{Daemon, DaemonError, Event, SceneRenderer};
use tb_render::{Frame, build_scene};

const BAR: BarSize = BarSize::MACBOOK_PRO;
const RULES: SpacingRules = SpacingRules::STANDARD;
const WHOLE_BAR: Rect = Rect { x: 0, y: 0, width: 2170, height: 60 };

fn theme() -> BarTheme {
    BarTheme::omarchy_fallback()
}

fn daemon(layout: Layout) -> Daemon<FlatRenderer, RecordingDisplay> {
    Daemon::new(layout, theme(), FlatRenderer, RecordingDisplay::new())
}

fn presents(d: &Daemon<FlatRenderer, RecordingDisplay>) -> &[Presented] {
    d.display().presents()
}

/// What `FlatRenderer` draws for `chord` of `layout` in `theme`, nothing pressed.
fn flat(layout: &Layout, chord: Chord, theme: &BarTheme) -> Frame {
    let scene = build_scene(layout.layer(chord).unwrap(), theme, BAR, &RULES, None).unwrap();
    FlatRenderer.render(&scene)
}

fn rect_of(layout: &Layout, chord: Chord, id: &str) -> Rect {
    let placed = place(layout.layer(chord).unwrap(), BAR, &RULES).unwrap();
    placed.iter().find(|p| p.id.as_str() == id).unwrap().rect
}

fn button_mut<'a>(layout: &'a mut Layout, chord: Chord, id: &str) -> &'a mut Button {
    layout
        .layer_mut(chord)
        .unwrap()
        .groups
        .iter_mut()
        .flat_map(|g| g.buttons.iter_mut())
        .find(|b| b.id.as_str() == id)
        .unwrap()
}

fn fn_held() -> Modifiers {
    Modifiers { fn_key: true, ..Modifiers::default() }
}

fn bytes(c: Rgba) -> [u8; 4] {
    [c.r, c.g, c.b, c.a]
}

#[test]
fn the_flat_renderer_paints_the_background_then_each_button_fill_as_is() {
    let layout = presets::default_layout();
    let t = theme();
    let frame = flat(&layout, Chord::Default, &t);
    let (esc, f1, f5) = (
        rect_of(&layout, Chord::Default, "esc"),
        rect_of(&layout, Chord::Default, "f1"),
        rect_of(&layout, Chord::Default, "f5"),
    );

    assert_eq!((frame.width, frame.height), (2170, 60));
    let gap_x = u32::from(esc.right() + (f1.x - esc.right()) / 2);
    assert_eq!(frame.pixel(gap_x, 30), bytes(t.background), "group gap");
    assert_eq!(frame.pixel(0, 0), bytes(t.background), "bar corner");
    assert_eq!(frame.pixel(u32::from(f5.x), 0), bytes(t.background), "above the vertical inset");
    assert_eq!(frame.pixel(u32::from(f5.x), u32::from(f5.y)), bytes(t.button_fill), "F5 top-left");
    let (last_x, last_y) = (u32::from(f5.right() - 1), u32::from(f5.y + f5.height - 1));
    assert_eq!(frame.pixel(last_x, last_y), bytes(t.button_fill), "F5 bottom-right");
    assert_eq!(frame.pixel(last_x + 1, last_y), bytes(t.background), "gap right of F5");
}

#[test]
fn a_new_daemon_presents_nothing_until_it_runs() {
    let d = daemon(presets::default_layout());
    assert_eq!(presents(&d).len(), 0, "nothing presented before run");
}

#[test]
fn running_with_no_events_presents_the_default_layer_once_over_the_whole_bar() {
    let layout = presets::default_layout();
    let mut d = daemon(layout.clone());
    d.run(Vec::new()).unwrap();

    let p = presents(&d);
    assert_eq!(p.len(), 1);
    assert_eq!(p[0].damage, [WHOLE_BAR]);
    assert_eq!(p[0].frame, flat(&layout, Chord::Default, &theme()));
}

#[test]
fn holding_fn_presents_the_fn_layer_over_the_whole_bar() {
    let layout = presets::default_layout();
    let mut d = daemon(layout.clone());
    d.run(vec![Event::Modifiers(fn_held())]).unwrap();

    let p = presents(&d);
    assert_eq!(p.len(), 2);
    assert_eq!(p[1].damage, [WHOLE_BAR]);
    assert_eq!(p[1].frame, flat(&layout, Chord::Fn, &theme()));
}

#[test]
fn modifiers_that_resolve_to_the_layer_already_shown_present_nothing() {
    let mut d = daemon(presets::default_layout());
    let ctrl_alone = Modifiers { ctrl: true, ..Modifiers::default() };
    // Fn + Alt has no layer in the default preset: it falls back to the Fn layer.
    let fn_alt = Modifiers { fn_key: true, alt: true, ..Modifiers::default() };
    d.run(vec![
        Event::Modifiers(ctrl_alone),
        Event::Modifiers(fn_held()),
        Event::Modifiers(fn_alt),
        Event::Modifiers(fn_held()),
    ])
    .unwrap();
    assert_eq!(presents(&d).len(), 2, "start, then Fn; the rest show the same layer");
}

#[test]
fn releasing_fn_presents_the_default_layer_again() {
    let layout = presets::default_layout();
    let mut d = daemon(layout.clone());
    d.run(vec![Event::Modifiers(fn_held()), Event::Modifiers(Modifiers::default())]).unwrap();

    let p = presents(&d);
    assert_eq!(p.len(), 3);
    assert_eq!(p[2].damage, [WHOLE_BAR]);
    assert_eq!(p[2].frame, flat(&layout, Chord::Default, &theme()));
}

#[test]
fn a_theme_with_a_new_background_presents_the_whole_bar_in_it() {
    let layout = presets::default_layout();
    let mut new_theme = theme();
    new_theme.background = Rgba::opaque(0x12, 0x34, 0x56);
    let mut d = daemon(layout.clone());
    d.run(vec![Event::Theme(new_theme.clone())]).unwrap();

    let p = presents(&d);
    assert_eq!(p.len(), 2);
    assert_eq!(p[1].damage, [WHOLE_BAR]);
    assert_eq!(p[1].frame, flat(&layout, Chord::Default, &new_theme));
    assert_eq!(p[1].frame.pixel(0, 0), [0x12, 0x34, 0x56, 0xff]);
}

#[test]
fn the_same_theme_again_presents_nothing() {
    let mut d = daemon(presets::default_layout());
    d.run(vec![Event::Theme(theme())]).unwrap();
    assert_eq!(presents(&d).len(), 1);
}

#[test]
fn a_theme_change_that_leaves_the_scene_equal_presents_nothing() {
    // Nothing is pressed, so the pressed colour is not on the bar.
    let mut new_theme = theme();
    new_theme.pressed_fill = Rgba::opaque(1, 2, 3);
    let mut d = daemon(presets::default_layout());
    d.run(vec![Event::Theme(new_theme)]).unwrap();
    assert_eq!(presents(&d).len(), 1);
}

#[test]
fn a_layout_that_only_relabels_f5_presents_only_f5() {
    let old = presets::default_layout();
    let mut relabelled = old.clone();
    button_mut(&mut relabelled, Chord::Default, "f5").label = "F5*".to_owned();
    let mut d = daemon(old.clone());
    d.run(vec![Event::Layout(relabelled.clone())]).unwrap();

    let p = presents(&d);
    assert_eq!(p.len(), 2);
    assert_eq!(p[1].damage, [rect_of(&old, Chord::Default, "f5")]);
    assert_eq!(p[1].frame, flat(&relabelled, Chord::Default, &theme()));
}

#[test]
fn a_layout_that_changes_widths_presents_the_whole_bar() {
    let mut wider = presets::default_layout();
    button_mut(&mut wider, Chord::Default, "f5").width = Width::Span(2);
    let mut d = daemon(presets::default_layout());
    d.run(vec![Event::Layout(wider.clone())]).unwrap();

    let p = presents(&d);
    assert_eq!(p.len(), 2);
    assert_eq!(p[1].damage, [WHOLE_BAR]);
    assert_eq!(p[1].frame, flat(&wider, Chord::Default, &theme()));
}

#[test]
fn a_layout_change_is_resolved_with_the_modifiers_still_held() {
    let old = presets::default_layout();
    let mut relabelled = old.clone();
    button_mut(&mut relabelled, Chord::Fn, "play-pause").label = "pause".to_owned();
    let mut d = daemon(old.clone());
    d.run(vec![Event::Modifiers(fn_held()), Event::Layout(relabelled.clone())]).unwrap();

    let p = presents(&d);
    assert_eq!(p.len(), 3);
    assert_eq!(p[2].damage, [rect_of(&old, Chord::Fn, "play-pause")]);
    assert_eq!(p[2].frame, flat(&relabelled, Chord::Fn, &theme()));
}

#[test]
fn a_layout_whose_shown_layer_does_not_fit_is_refused_and_the_old_layout_stays() {
    let old = presets::default_layout();
    let mut crowded = old.clone();
    let f_keys = &mut crowded.layer_mut(Chord::Default).unwrap().groups[1].buttons;
    for n in 13..=40 {
        let mut extra = f_keys[0].clone();
        extra.id = ButtonId::new(format!("f{n}"));
        f_keys.push(extra);
    }
    assert!(place(crowded.layer(Chord::Default).unwrap(), BAR, &RULES).is_err(), "precondition");

    let mut d = daemon(old.clone());
    d.run(Vec::new()).unwrap();
    let err = d.handle(Event::Layout(crowded)).unwrap_err();
    assert!(matches!(err, DaemonError::Layout(_)), "got {err:?}");
    assert_eq!(presents(&d).len(), 1, "nothing presented on error");

    let mut new_theme = theme();
    new_theme.background = Rgba::opaque(0x21, 0x43, 0x65);
    d.handle(Event::Theme(new_theme.clone())).unwrap();
    let p = presents(&d);
    assert_eq!(p.len(), 2);
    assert_eq!(p[1].frame, flat(&old, Chord::Default, &new_theme), "rendered from the old layout");
}

#[test]
fn a_layout_without_a_layer_for_the_held_keys_is_refused_with_no_layer() {
    let old = presets::default_layout();
    let mut only_fn_ctrl = old.clone();
    only_fn_ctrl.layers.retain(|l| l.chord == Chord::FnCtrl);

    let mut d = daemon(old.clone());
    d.run(Vec::new()).unwrap();
    let err = d.handle(Event::Layout(only_fn_ctrl)).unwrap_err();
    assert!(matches!(err, DaemonError::NoLayer), "got {err:?}");
    assert_eq!(presents(&d).len(), 1);

    d.handle(Event::Modifiers(fn_held())).unwrap();
    assert_eq!(presents(&d)[1].frame, flat(&old, Chord::Fn, &theme()), "old layout kept");
}

#[test]
fn run_stops_at_the_first_error_and_returns_it() {
    let mut only_fn_ctrl = presets::default_layout();
    only_fn_ctrl.layers.retain(|l| l.chord == Chord::FnCtrl);
    let mut d = daemon(presets::default_layout());
    let err = d.run(vec![Event::Layout(only_fn_ctrl), Event::Modifiers(fn_held())]).unwrap_err();
    assert!(matches!(err, DaemonError::NoLayer), "got {err:?}");
    assert_eq!(presents(&d).len(), 1, "the Fn event after the error is not handled");
}

#[test]
fn run_over_a_channel_returns_after_the_sender_is_dropped() {
    let layout = presets::default_layout();
    let (tx, rx) = mpsc::channel::<Event>();
    tx.send(Event::Modifiers(fn_held())).unwrap();
    drop(tx);
    let mut d = daemon(layout.clone());
    d.run(rx).unwrap();

    let p = presents(&d);
    assert_eq!(p.len(), 2);
    assert_eq!(p[1].frame, flat(&layout, Chord::Fn, &theme()));
}
