//! A cancelled touch ends without a lift: nothing runs and nothing stays pressed. The simulator
//! window cancels a press when the pointer leaves the bar (beside it, below it, out of the
//! window), on Escape and when the window loses focus, all as Rune delivers them (a captured
//! drag's moves outside `sim-screen`, and `Drag::cancelled`); a cancelled press stays cancelled
//! even if the pointer comes back over the bar before the release. Window touches are driven
//! through a headless `rune::Runtime` of `sim_window_view` (no window opens) and read from
//! `WindowTouches` after the runtime is dropped. Daemon cases run the real `Daemon` (CPU
//! `FlatRenderer`, `RecordingDisplay`) with `RecordingEmitter` and `RecordingSpawner`, so no key
//! reaches a device and no command runs. Rects come from `tb_core::place`; at `SIM_WINDOW` one
//! window point is two bar px.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::sync::{Arc, Mutex, mpsc};
use std::time::Duration;

use rune::{Env, Mods, PhysicalInput, PhysicalKeyCode, Runtime};
use tb_core::{BarSize, BarTheme, Chord, Key, Layout, Modifiers, Rect, SpacingRules, place, presets};
use tb_daemon::fake::{
    FlatRenderer, Presented, RecordingDisplay, RecordingEmitter, RecordingSpawner, ScriptedTouches,
};
use tb_daemon::{
    ActionError, Actions, Daemon, Event, Report, SIM_WINDOW, SimWindow, Touch, TouchSource, WindowTouches,
    forward_touches, sim_window_view,
};

const WAIT: Duration = Duration::from_secs(10);
const STEP: f64 = 1.0 / 60.0;

// ---------- the window ----------

/// Every touch `source` gives until it closes. Panics if it does not close within `WAIT`.
fn drain(mut source: WindowTouches) -> Vec<Touch> {
    let (done, result) = mpsc::channel();
    std::thread::spawn(move || {
        let mut touches = Vec::new();
        while let Some(touch) = source.next_touch().unwrap() {
            touches.push(touch);
        }
        let _ = done.send(touches);
    });
    result.recv_timeout(WAIT).expect("the window's touches close once its state is dropped")
}

fn window(size: (f32, f32)) -> (Runtime<SimWindow>, WindowTouches) {
    let (state, touches) = SimWindow::new();
    let mut rt = Runtime::new(state, sim_window_view, size, Env::new(true, 1.0));
    rt.frame(0.0);
    (rt, touches)
}

fn press(rt: &mut Runtime<SimWindow>, x: f32, y: f32) {
    rt.pointer_down(x, y);
    rt.frame(STEP);
}

fn release(rt: &mut Runtime<SimWindow>, x: f32, y: f32) {
    rt.pointer_up(x, y);
    rt.frame(STEP);
}

fn move_to(rt: &mut Runtime<SimWindow>, x: f32, y: f32) {
    rt.pointer_move(x, y);
    rt.frame(STEP);
}

fn escape(rt: &mut Runtime<SimWindow>) {
    rt.physical_input(PhysicalInput::Key {
        key: PhysicalKeyCode::Escape,
        mods: Mods::default(),
        pressed: true,
        repeat: false,
    });
    rt.frame(STEP);
}

/// The touches of `gesture` on a `SIM_WINDOW`-sized window.
fn touches_of(gesture: impl FnOnce(&mut Runtime<SimWindow>)) -> Vec<Touch> {
    let (mut rt, touches) = window(SIM_WINDOW);
    gesture(&mut rt);
    drop(rt);
    drain(touches)
}

#[test]
fn moving_a_pressed_pointer_out_of_the_window_cancels_the_touch() {
    let touches = touches_of(|rt| {
        press(rt, 453.25, 15.0);
        move_to(rt, 1200.0, 50.0);
        rt.pointer_leave();
        rt.frame(STEP);
        release(rt, 1200.0, 50.0);
    });
    assert_eq!(touches, [Touch::Down { x: 906 }, Touch::Cancel]);
}

#[test]
fn a_release_outside_the_window_with_no_move_cancels_the_touch() {
    let touches = touches_of(|rt| {
        press(rt, 453.25, 15.0);
        release(rt, 1200.0, 50.0);
    });
    assert_eq!(touches, [Touch::Down { x: 906 }, Touch::Cancel]);
}

#[test]
fn moving_a_pressed_pointer_left_of_the_bar_cancels_the_touch() {
    let touches = touches_of(|rt| {
        press(rt, 453.25, 15.0);
        move_to(rt, -5.0, 15.0);
        release(rt, -5.0, 15.0);
    });
    assert_eq!(touches, [Touch::Down { x: 906 }, Touch::Cancel]);
}

#[test]
fn moving_a_pressed_pointer_below_the_sim_screen_of_a_taller_window_cancels_the_touch() {
    let (mut rt, touches) = window((1085.0, 300.0));
    let ([_, y, _, h], _) = rt.presented("sim-screen").expect("sim-screen is presented");
    let below = (y + h + 300.0) / 2.0;
    assert!(below > y + h && below < 300.0, "precondition: {below} is in the window, below sim-screen");
    press(&mut rt, 453.25, y + h / 2.0);
    move_to(&mut rt, 453.25, below);
    release(&mut rt, 453.25, below);
    drop(rt);
    assert_eq!(drain(touches), [Touch::Down { x: 906 }, Touch::Cancel]);
}

#[test]
fn coming_back_over_the_bar_after_leaving_it_does_not_revive_the_touch() {
    let touches = touches_of(|rt| {
        press(rt, 453.25, 15.0);
        move_to(rt, 453.25, 50.0);
        move_to(rt, 453.25, 15.0);
        move_to(rt, 600.0, 15.0);
        release(rt, 453.25, 15.0);
    });
    assert_eq!(touches, [Touch::Down { x: 906 }, Touch::Cancel]);
}

#[test]
fn escape_during_a_press_cancels_the_touch() {
    let touches = touches_of(|rt| {
        press(rt, 453.25, 15.0);
        escape(rt);
        release(rt, 453.25, 15.0);
    });
    assert_eq!(touches, [Touch::Down { x: 906 }, Touch::Cancel]);
}

#[test]
fn losing_window_focus_during_a_press_cancels_the_touch() {
    let touches = touches_of(|rt| {
        press(rt, 453.25, 15.0);
        rt.window_focus(false);
        rt.frame(STEP);
        release(rt, 453.25, 15.0);
    });
    assert_eq!(touches, [Touch::Down { x: 906 }, Touch::Cancel]);
}

#[test]
fn escape_after_sliding_cancels_the_slid_touch_once() {
    let touches = touches_of(|rt| {
        press(rt, 453.25, 15.0);
        move_to(rt, 600.0, 15.0);
        escape(rt);
        escape(rt);
        release(rt, 600.0, 15.0);
    });
    assert_eq!(touches, [Touch::Down { x: 906 }, Touch::Down { x: 1200 }, Touch::Cancel]);
}

#[test]
fn a_cancelled_press_does_not_spoil_the_next_click() {
    let touches = touches_of(|rt| {
        press(rt, 453.25, 15.0);
        escape(rt);
        release(rt, 453.25, 15.0);
        press(rt, 50.25, 15.0);
        release(rt, 50.25, 15.0);
    });
    assert_eq!(touches, [Touch::Down { x: 906 }, Touch::Cancel, Touch::Down { x: 100 }, Touch::Up]);
}

// ---------- the daemon ----------

struct Outcome {
    emitted: Vec<Vec<Key>>,
    spawned: Vec<String>,
    reports: usize,
    presents: Vec<Presented>,
}

/// Runs `events` on the default preset with a recording keyboard and spawner.
fn run(events: Vec<Event>) -> Outcome {
    let (emitter, spawner) = (RecordingEmitter::new(), RecordingSpawner::new());
    let reports = Arc::new(Mutex::new(0_usize));
    let sink = Arc::clone(&reports);
    let report: Report = Arc::new(move |_: ActionError| *sink.lock().unwrap() += 1);
    let actions = Actions::new(Some(Box::new(emitter.clone())), Box::new(spawner.clone()), report);
    let mut daemon =
        Daemon::new(preset(), BarTheme::omarchy_fallback(), FlatRenderer, RecordingDisplay::new())
            .with_actions(actions);
    daemon.run(events).unwrap();
    let reports = *reports.lock().unwrap();
    Outcome {
        emitted: emitter.emitted(),
        spawned: spawner.spawned(),
        reports,
        presents: daemon.display().presents().to_vec(),
    }
}

fn preset() -> Layout {
    presets::default_layout()
}

fn rect(chord: Chord, id: &str) -> Rect {
    let placed =
        place(preset().layer(chord).unwrap(), BarSize::MACBOOK_PRO, &SpacingRules::STANDARD).unwrap();
    placed.iter().find(|p| p.id.as_str() == id).unwrap().rect
}

fn x_of(chord: Chord, id: &str) -> u16 {
    let r = rect(chord, id);
    r.x + r.width / 2
}

fn down(x: u16) -> Event {
    Event::Touch(Touch::Down { x })
}

fn cancel() -> Event {
    Event::Touch(Touch::Cancel)
}

fn up() -> Event {
    Event::Touch(Touch::Up)
}

#[test]
fn a_cancelled_touch_on_f5_runs_nothing_and_shows_f5_released() {
    let f5 = rect(Chord::Default, "f5");
    let out = run(vec![down(x_of(Chord::Default, "f5")), cancel()]);
    assert_eq!(out.emitted, Vec::<Vec<Key>>::new());
    assert_eq!(out.reports, 0);
    assert_eq!(out.presents.len(), 3, "start, F5 pressed, F5 released");
    assert_eq!(out.presents[2].damage, [f5]);
    assert_eq!(out.presents[2].frame, out.presents[0].frame, "nothing stays pressed");
}

#[test]
fn a_lift_after_a_cancel_runs_nothing() {
    let out = run(vec![down(x_of(Chord::Default, "f5")), cancel(), up()]);
    assert_eq!(out.emitted, Vec::<Vec<Key>>::new());
    assert_eq!(out.presents.len(), 3, "the lift has nothing left to release");
}

#[test]
fn a_cancelled_touch_on_lock_starts_no_command() {
    let held = Event::Modifiers(Modifiers { fn_key: true, ctrl: true, ..Modifiers::default() });
    let out = run(vec![held, down(x_of(Chord::FnCtrl, "lock")), cancel()]);
    assert_eq!(out.spawned, Vec::<String>::new());
    assert_eq!(out.emitted, Vec::<Vec<Key>>::new());
    let last = out.presents.last().unwrap();
    let start_of_layer = &out.presents[1];
    assert_eq!(last.frame, start_of_layer.frame, "the Fn + Ctrl layer with nothing pressed");
}

#[test]
fn a_cancel_after_sliding_back_onto_f5_runs_nothing() {
    let (f5, f6) = (x_of(Chord::Default, "f5"), x_of(Chord::Default, "f6"));
    let out = run(vec![down(f5), down(f6), down(f5), cancel()]);
    assert_eq!(out.emitted, Vec::<Vec<Key>>::new());
    assert_eq!(out.presents.last().unwrap().frame, out.presents[0].frame);
}

#[test]
fn a_cancel_with_no_touch_presents_and_runs_nothing() {
    let out = run(vec![cancel(), cancel()]);
    assert_eq!(out.presents.len(), 1, "only the start present");
    assert_eq!(out.emitted, Vec::<Vec<Key>>::new());
    assert_eq!(out.reports, 0);
}

#[test]
fn after_a_cancel_the_next_tap_on_f5_presses_f5_once() {
    let f5 = x_of(Chord::Default, "f5");
    let out = run(vec![down(f5), cancel(), down(f5), up()]);
    assert_eq!(out.emitted, vec![vec![Key::named("F5").unwrap()]]);
}

#[test]
fn forwarding_passes_a_cancel_through_in_order() {
    let mut source = ScriptedTouches::new([Touch::Down { x: 906 }, Touch::Cancel]);
    let (events, received) = mpsc::channel();
    forward_touches(&mut source, &events).unwrap();
    drop(events);
    let touches: Vec<Touch> = received
        .iter()
        .map(|e| match e {
            Event::Touch(t) => t,
            other => panic!("only touches are forwarded, got {other:?}"),
        })
        .collect();
    assert_eq!(touches, [Touch::Down { x: 906 }, Touch::Cancel]);
}
