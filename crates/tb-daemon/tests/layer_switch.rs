//! A modifier change reaches the daemon as `Event::Modifiers` and is presented as the new layer
//! within 50 ms. `forward_modifiers` runs a modifier source against the daemon's event channel;
//! when the source ends, the bar goes back to the default layer (held keys are unknown once the
//! keyboard is gone). The latency case feeds raw `x86_64` `struct input_event` records (24 bytes)
//! through a pipe into `EvdevModifiers`, runs the real `Daemon` with the CPU `FlatRenderer`, and
//! stamps each present; no input device is opened.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::io::{Cursor, Write};
use std::sync::mpsc;
use std::thread;
use std::time::{Duration, Instant};

use tb_core::{BarSize, BarTheme, Chord, Modifiers, Rect, SpacingRules, presets};
use tb_daemon::fake::{FlatRenderer, ScriptedModifiers};
use tb_daemon::{
    Daemon, Display, DisplayError, EvdevModifiers, Event, KeyError, SceneRenderer, forward_modifiers,
};
use tb_render::{Frame, build_scene};

/// How fast a modifier change must be presented (ticket Acceptance).
const WITHIN: Duration = Duration::from_millis(50);
/// Time between key reports in the latency case, like a person pressing keys.
const BETWEEN_KEYS: Duration = Duration::from_millis(100);

const EV_SYN: u16 = 0;
const EV_KEY: u16 = 1;
const EV_MSC: u16 = 4;
const KEY_FN: u16 = 464;
const KEY_LEFTCTRL: u16 = 29;

fn record(kind: u16, code: u16, value: i32) -> Vec<u8> {
    let mut r = Vec::with_capacity(24);
    r.extend_from_slice(&1_762_675_200_i64.to_ne_bytes());
    r.extend_from_slice(&123_456_i64.to_ne_bytes());
    r.extend_from_slice(&kind.to_ne_bytes());
    r.extend_from_slice(&code.to_ne_bytes());
    r.extend_from_slice(&value.to_ne_bytes());
    r
}

/// One keyboard report: each key's scan code and state, then `SYN_REPORT`.
fn report(keys: &[(u16, i32)]) -> Vec<u8> {
    let mut bytes = Vec::new();
    for &(code, value) in keys {
        bytes.extend(record(EV_MSC, 4, 0x7_00e0));
        bytes.extend(record(EV_KEY, code, value));
    }
    bytes.extend(record(EV_SYN, 0, 0));
    bytes
}

fn held(fn_key: bool, ctrl: bool) -> Modifiers {
    Modifiers { fn_key, ctrl, ..Modifiers::default() }
}

/// The modifiers of every event on the channel, in order; any other event fails the test.
fn modifier_events(rx: &mpsc::Receiver<Event>) -> Vec<Modifiers> {
    rx.try_iter()
        .map(|e| match e {
            Event::Modifiers(m) => m,
            other => panic!("only modifier events are forwarded, got {other:?}"),
        })
        .collect()
}

#[test]
fn forwarded_modifier_changes_arrive_as_modifier_events_in_order() {
    let script = [held(true, false), held(true, true), Modifiers::default()];
    let (tx, rx) = mpsc::channel::<Event>();
    forward_modifiers(&mut ScriptedModifiers::new(script), &tx).unwrap();
    drop(tx);
    assert_eq!(modifier_events(&rx), script, "a source ending on the default modifiers adds nothing");
}

#[test]
fn a_source_that_closes_while_fn_is_held_is_followed_by_the_default_modifiers() {
    let (tx, rx) = mpsc::channel::<Event>();
    forward_modifiers(&mut ScriptedModifiers::new([held(true, false)]), &tx).unwrap();
    drop(tx);
    assert_eq!(modifier_events(&rx), [held(true, false), Modifiers::default()]);
}

#[test]
fn a_failing_keyboard_sends_the_default_modifiers_then_returns_its_error() {
    let mut bytes = report(&[(KEY_FN, 1)]);
    bytes.extend_from_slice(&record(EV_KEY, KEY_LEFTCTRL, 1)[..7]);
    let (tx, rx) = mpsc::channel::<Event>();
    let ended = forward_modifiers(&mut EvdevModifiers::new(Cursor::new(bytes)), &tx);
    drop(tx);
    match ended {
        Err(KeyError::Io { .. }) => {}
        other => panic!("the record cut short is returned as KeyError::Io, got {other:?}"),
    }
    assert_eq!(modifier_events(&rx), [held(true, false), Modifiers::default()]);
}

#[test]
fn forwarding_to_a_dropped_receiver_ends_with_ok() {
    let (tx, rx) = mpsc::channel::<Event>();
    drop(rx);
    let mut src = ScriptedModifiers::new([held(true, false), Modifiers::default()]);
    forward_modifiers(&mut src, &tx).unwrap();
}

/// A display that keeps each frame with the instant it was presented.
#[derive(Default)]
struct StampingDisplay {
    presents: Vec<(Frame, Instant)>,
}

impl Display for StampingDisplay {
    fn present(&mut self, frame: &Frame, _damage: &[Rect]) -> Result<(), DisplayError> {
        self.presents.push((frame.clone(), Instant::now()));
        Ok(())
    }
}

/// The whole bar as `FlatRenderer` draws the `chord` layer of the default preset, nothing pressed.
fn flat_layer(chord: Chord) -> Frame {
    let layout = presets::default_layout();
    let scene = build_scene(
        layout.layer(chord).unwrap(),
        &BarTheme::omarchy_fallback(),
        BarSize::MACBOOK_PRO,
        &SpacingRules::STANDARD,
        None,
    )
    .unwrap();
    FlatRenderer.render(&scene)
}

#[test]
fn a_modifier_change_is_presented_as_its_layer_within_50_ms() {
    let steps = [
        (report(&[(KEY_FN, 1)]), Chord::Fn),
        (report(&[(KEY_LEFTCTRL, 1)]), Chord::FnCtrl),
        (report(&[(KEY_FN, 0), (KEY_LEFTCTRL, 0)]), Chord::Default),
    ];
    let (reader, mut writer) = std::io::pipe().unwrap();
    let (tx, rx) = mpsc::channel::<Event>();
    let keys = thread::spawn(move || forward_modifiers(&mut EvdevModifiers::new(reader), &tx));
    let records: Vec<Vec<u8>> = steps.iter().map(|(bytes, _)| bytes.clone()).collect();
    let typist = thread::spawn(move || {
        let mut written = Vec::new();
        for bytes in records {
            thread::sleep(BETWEEN_KEYS);
            written.push(Instant::now());
            writer.write_all(&bytes).unwrap();
        }
        written
    });

    let mut daemon = Daemon::new(
        presets::default_layout(),
        BarTheme::omarchy_fallback(),
        FlatRenderer,
        StampingDisplay::default(),
    );
    daemon.run(rx).unwrap();
    let written = typist.join().unwrap();
    keys.join().unwrap().unwrap();

    let presents = &daemon.display().presents;
    assert_eq!(presents.len(), 4, "start, then one present per modifier change");
    assert!(presents[0].0 == flat_layer(Chord::Default), "present 1 is the default layer");
    for (i, ((_, chord), wrote)) in steps.iter().zip(&written).enumerate() {
        let (frame, shown) = &presents[i + 1];
        assert!(*frame == flat_layer(*chord), "present {} is the {chord:?} layer", i + 2);
        let took = shown.saturating_duration_since(*wrote);
        assert!(took <= WITHIN, "the {chord:?} layer came {took:?} after its key report, want ≤ {WITHIN:?}");
    }
}
