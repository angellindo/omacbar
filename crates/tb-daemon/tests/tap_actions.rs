//! A tap runs its button's action. Touches go through the real `Daemon` (CPU `FlatRenderer`,
//! `RecordingDisplay`) with `RecordingEmitter` as the virtual keyboard and `RecordingSpawner` as
//! the command runner, so no key reaches a device and no command runs. A tap is a touch that lands
//! on a button and lifts while still over it; the action runs once, on the lift. Keys are pressed
//! only on the `default` and `fn` layers; on a modifier layer they are reported instead
//! (ADR-0011). Every action failure is reported and the run goes on. Rects come from
//! `tb_core::place`.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::io;
use std::sync::{Arc, Mutex};

use tb_core::{
    Action, BarSize, BarTheme, Button, Chord, Group, Key, Layout, Modifiers, SpacingRules, place, presets,
};
use tb_daemon::fake::{FlatRenderer, RecordingDisplay, RecordingEmitter, RecordingSpawner};
use tb_daemon::{ActionError, Actions, Daemon, Event, KeyEmitter, Report, Spawner, Touch};

/// What one run did: keys pressed, commands started, failures reported.
#[derive(Debug)]
struct Outcome {
    emitted: Vec<Vec<Key>>,
    spawned: Vec<String>,
    reports: Vec<ActionError>,
    presents: usize,
}

fn collecting() -> (Report, Arc<Mutex<Vec<ActionError>>>) {
    let reports = Arc::new(Mutex::new(Vec::new()));
    let sink = Arc::clone(&reports);
    let report: Report = Arc::new(move |e: ActionError| sink.lock().unwrap().push(e));
    (report, reports)
}

/// Runs `events` on `layout` with a recording keyboard (or none) and `spawner`. The run must end
/// with Ok: an action failure is never a daemon error. `spawned` is left empty.
fn run_with(layout: Layout, events: Vec<Event>, keyboard: bool, spawner: Box<dyn Spawner + Send>) -> Outcome {
    let emitter = RecordingEmitter::new();
    let (report, reports) = collecting();
    let keys = keyboard.then(|| Box::new(emitter.clone()) as Box<dyn KeyEmitter + Send>);
    let actions = Actions::new(keys, spawner, report);
    let mut daemon = Daemon::new(layout, BarTheme::omarchy_fallback(), FlatRenderer, RecordingDisplay::new())
        .with_actions(actions);
    daemon.run(events).unwrap();
    let presents = daemon.display().presents().len();
    let reports = std::mem::take(&mut *reports.lock().unwrap());
    Outcome { emitted: emitter.emitted(), spawned: Vec::new(), reports, presents }
}

/// Runs `events` on `layout` with a recording keyboard and a recording spawner.
fn run(layout: Layout, events: Vec<Event>) -> Outcome {
    let spawner = RecordingSpawner::new();
    let mut outcome = run_with(layout, events, true, Box::new(spawner.clone()));
    outcome.spawned = spawner.spawned();
    outcome
}

fn key(name: &str) -> Key {
    Key::named(name).unwrap()
}

/// The centre x of button `id` on the `chord` layer of `layout`.
fn x_of(layout: &Layout, chord: Chord, id: &str) -> u16 {
    let placed = place(layout.layer(chord).unwrap(), BarSize::MACBOOK_PRO, &SpacingRules::STANDARD).unwrap();
    let rect = placed.iter().find(|p| p.id.as_str() == id).unwrap().rect;
    rect.x + rect.width / 2
}

fn down(x: u16) -> Event {
    Event::Touch(Touch::Down { x })
}

fn up() -> Event {
    Event::Touch(Touch::Up)
}

fn held(fn_key: bool, ctrl: bool) -> Event {
    Event::Modifiers(Modifiers { fn_key, ctrl, ..Modifiers::default() })
}

fn tap(x: u16) -> [Event; 2] {
    [down(x), up()]
}

fn preset() -> Layout {
    presets::default_layout()
}

/// The preset with `button` added as a new last group of the `chord` layer.
fn with_button(chord: Chord, button: Button) -> Layout {
    let mut layout = preset();
    layout.layer_mut(chord).unwrap().groups.push(Group { buttons: vec![button] });
    layout
}

fn key_button(id: &str, keys: &[&str]) -> Button {
    Button::new(id, id, Action::Key { keys: keys.iter().map(|k| (*k).to_owned()).collect() })
}

/// A spawner that can never start a command.
struct FailingSpawner;

impl Spawner for FailingSpawner {
    fn spawn(&mut self, _run: &str) -> io::Result<()> {
        Err(io::Error::other("no shell in this test"))
    }
}

#[test]
fn a_tap_on_f5_presses_f5_once() {
    let x = x_of(&preset(), Chord::Default, "f5");
    let out = run(preset(), tap(x).to_vec());
    assert_eq!(out.emitted, vec![vec![key("F5")]]);
    assert_eq!(out.spawned, Vec::<String>::new());
    assert!(out.reports.is_empty(), "{:?}", out.reports);
}

#[test]
fn two_taps_press_their_keys_in_tap_order() {
    let layout = preset();
    let (f5, f6) = (x_of(&layout, Chord::Default, "f5"), x_of(&layout, Chord::Default, "f6"));
    let out = run(layout, [tap(f5), tap(f6)].concat());
    assert_eq!(out.emitted, vec![vec![key("F5")], vec![key("F6")]]);
}

#[test]
fn a_tap_presses_every_key_of_the_action_in_one_press() {
    let layout = with_button(Chord::Default, key_button("refresh", &["LeftCtrl", "F5"]));
    let x = x_of(&layout, Chord::Default, "refresh");
    let out = run(layout, tap(x).to_vec());
    assert_eq!(out.emitted, vec![vec![key("LeftCtrl"), key("F5")]]);
}

#[test]
fn a_tap_on_the_fn_layer_presses_its_key() {
    let layout = preset();
    let x = x_of(&layout, Chord::Fn, "brightness-up");
    let out = run(layout, [vec![held(true, false)], tap(x).to_vec()].concat());
    assert_eq!(out.emitted, vec![vec![key("BrightnessUp")]]);
    assert!(out.reports.is_empty(), "{:?}", out.reports);
}

#[test]
fn a_tap_on_lock_with_fn_ctrl_held_runs_its_command_and_presses_nothing() {
    let layout = preset();
    let x = x_of(&layout, Chord::FnCtrl, "lock");
    let out = run(layout, [vec![held(true, true)], tap(x).to_vec()].concat());
    assert_eq!(out.spawned, vec!["omarchy lock".to_owned()]);
    assert_eq!(out.emitted, Vec::<Vec<Key>>::new());
    assert!(out.reports.is_empty(), "{:?}", out.reports);
}

#[test]
fn a_tap_on_a_command_on_the_default_layer_runs_it() {
    let layout = with_button(
        Chord::Default,
        Button::new("hello", "hi", Action::Command { run: "echo hi".to_owned() }),
    );
    let x = x_of(&layout, Chord::Default, "hello");
    let out = run(layout, tap(x).to_vec());
    assert_eq!(out.spawned, vec!["echo hi".to_owned()]);
    assert_eq!(out.emitted, Vec::<Vec<Key>>::new());
}

#[test]
fn a_tap_on_a_button_with_no_action_does_nothing() {
    let layout = with_button(Chord::Default, Button::new("blank", "blank", Action::None));
    let x = x_of(&layout, Chord::Default, "blank");
    let out = run(layout, tap(x).to_vec());
    assert_eq!(out.emitted, Vec::<Vec<Key>>::new());
    assert_eq!(out.spawned, Vec::<String>::new());
    assert!(out.reports.is_empty(), "{:?}", out.reports);
}

#[test]
fn a_key_on_a_modifier_layer_is_not_pressed_and_is_reported_naming_the_button() {
    let layout = with_button(Chord::FnCtrl, key_button("refresh", &["F5"]));
    let x = x_of(&layout, Chord::FnCtrl, "refresh");
    let out = run(layout, [vec![held(true, true)], tap(x).to_vec()].concat());
    assert_eq!(out.emitted, Vec::<Vec<Key>>::new());
    assert_eq!(out.reports.len(), 1, "{:?}", out.reports);
    assert!(
        matches!(&out.reports[0], ActionError::ModifierLayer { chord: Chord::FnCtrl, id } if id.as_str() == "refresh"),
        "{:?}",
        out.reports
    );
    let message = out.reports[0].to_string();
    assert!(message.contains("refresh"), "{message}");
    assert!(
        ["FnCtrl", "Fn + Ctrl", "fn-ctrl"].iter().any(|l| message.contains(l)),
        "names the layer: {message}"
    );
    assert!(message.contains("command"), "points to a command: {message}");
    assert!(!message.contains('\n'), "one line: {message}");
}

#[test]
fn the_modifier_layer_rule_is_checked_before_the_key_names() {
    let layout = with_button(Chord::FnCtrl, key_button("refresh", &["F55"]));
    let x = x_of(&layout, Chord::FnCtrl, "refresh");
    let out = run(layout, [vec![held(true, true)], tap(x).to_vec()].concat());
    assert_eq!(out.reports.len(), 1, "{:?}", out.reports);
    assert!(matches!(&out.reports[0], ActionError::ModifierLayer { .. }), "{:?}", out.reports);
}

#[test]
fn holding_fn_ctrl_without_a_fn_ctrl_layer_presses_the_keys_of_the_fn_layer_shown() {
    let mut layout = preset();
    layout.layers.retain(|l| l.chord != Chord::FnCtrl);
    let x = x_of(&layout, Chord::Fn, "brightness-up");
    let out = run(layout, [vec![held(true, true)], tap(x).to_vec()].concat());
    assert_eq!(out.emitted, vec![vec![key("BrightnessUp")]]);
    assert!(out.reports.is_empty(), "{:?}", out.reports);
}

#[test]
fn unknown_or_missing_key_names_are_reported_as_bad_keys_and_nothing_is_pressed() {
    let mut layout = preset();
    for button in
        layout.layer_mut(Chord::Default).unwrap().groups.iter_mut().flat_map(|g| g.buttons.iter_mut())
    {
        match button.id.as_str() {
            "f5" => button.action = Action::Key { keys: vec!["F55".to_owned()] },
            "f6" => button.action = Action::Key { keys: vec![] },
            _ => {}
        }
    }
    let (f5, f6) = (x_of(&layout, Chord::Default, "f5"), x_of(&layout, Chord::Default, "f6"));
    let out = run(layout, [tap(f5), tap(f6)].concat());
    assert_eq!(out.emitted, Vec::<Vec<Key>>::new());
    assert_eq!(out.reports.len(), 2, "{:?}", out.reports);
    assert!(
        matches!(&out.reports[0], ActionError::BadKeys { chord: Chord::Default, id, .. } if id.as_str() == "f5"),
        "{:?}",
        out.reports
    );
    assert!(
        matches!(&out.reports[1], ActionError::BadKeys { chord: Chord::Default, id, .. } if id.as_str() == "f6"),
        "{:?}",
        out.reports
    );
}

#[test]
fn without_a_virtual_keyboard_a_key_tap_is_reported_and_the_run_goes_on() {
    let layout = preset();
    let (f5, lock) = (x_of(&layout, Chord::Default, "f5"), x_of(&layout, Chord::FnCtrl, "lock"));
    let spawner = RecordingSpawner::new();
    let events = [tap(f5).to_vec(), vec![held(true, true)], tap(lock).to_vec()].concat();
    let out = run_with(layout, events, false, Box::new(spawner.clone()));
    assert_eq!(out.reports.len(), 1, "{:?}", out.reports);
    assert!(
        matches!(&out.reports[0], ActionError::NoKeyboard { chord: Chord::Default, id } if id.as_str() == "f5"),
        "{:?}",
        out.reports
    );
    assert_eq!(spawner.spawned(), vec!["omarchy lock".to_owned()], "commands still run");
}

#[test]
fn a_command_that_cannot_start_is_reported_and_later_taps_still_run() {
    let layout = preset();
    let (f5, lock) = (x_of(&layout, Chord::Default, "f5"), x_of(&layout, Chord::FnCtrl, "lock"));
    let events =
        [vec![held(true, true)], tap(lock).to_vec(), vec![held(false, false)], tap(f5).to_vec()].concat();
    let out = run_with(layout, events, true, Box::new(FailingSpawner));
    assert_eq!(out.reports.len(), 1, "{:?}", out.reports);
    assert!(
        matches!(&out.reports[0], ActionError::Spawn { run, .. } if run == "omarchy lock"),
        "{:?}",
        out.reports
    );
    let message = out.reports[0].to_string();
    assert!(message.contains("omarchy lock"), "{message}");
    assert_eq!(out.emitted, vec![vec![key("F5")]], "the tap after the failure still runs");
}

#[test]
fn moving_from_f5_to_f6_before_the_lift_runs_nothing() {
    let layout = preset();
    let (f5, f6) = (x_of(&layout, Chord::Default, "f5"), x_of(&layout, Chord::Default, "f6"));
    let out = run(layout, vec![down(f5), down(f6), up()]);
    assert_eq!(out.emitted, Vec::<Vec<Key>>::new());
    assert!(out.reports.is_empty(), "{:?}", out.reports);
}

#[test]
fn sliding_off_f5_and_back_before_the_lift_is_a_tap_on_f5() {
    let layout = preset();
    let (f5, f6) = (x_of(&layout, Chord::Default, "f5"), x_of(&layout, Chord::Default, "f6"));
    let out = run(layout, vec![down(f5), down(f6), down(f5), up()]);
    assert_eq!(out.emitted, vec![vec![key("F5")]]);
}

#[test]
fn a_touch_that_never_lifts_runs_nothing() {
    let x = x_of(&preset(), Chord::Default, "f5");
    let out = run(preset(), vec![down(x)]);
    assert_eq!(out.emitted, Vec::<Vec<Key>>::new());
}

#[test]
fn a_lift_with_no_touch_runs_nothing() {
    let out = run(preset(), vec![up(), up()]);
    assert_eq!(out.emitted, Vec::<Vec<Key>>::new());
    assert!(out.reports.is_empty(), "{:?}", out.reports);
}

#[test]
fn a_touch_that_lands_on_no_button_runs_nothing_even_if_it_slides_onto_f5() {
    let layout = preset();
    let placed =
        place(layout.layer(Chord::Default).unwrap(), BarSize::MACBOOK_PRO, &SpacingRules::STANDARD).unwrap();
    let (esc, f1) = (placed[0].rect, placed[1].rect);
    let gap = esc.right() + (f1.x - esc.right()) / 2;
    let f5 = x_of(&layout, Chord::Default, "f5");
    let landed_in_gap = run(layout.clone(), vec![down(gap), up()]);
    assert_eq!(landed_in_gap.emitted, Vec::<Vec<Key>>::new());
    let slid_onto_f5 = run(layout, vec![down(gap), down(f5), up()]);
    assert_eq!(slid_onto_f5.emitted, Vec::<Vec<Key>>::new());
}

#[test]
fn a_modifier_change_during_a_touch_runs_the_action_of_the_layer_it_landed_on() {
    let layout = preset();
    let (f5, lock) = (x_of(&layout, Chord::Default, "f5"), x_of(&layout, Chord::FnCtrl, "lock"));
    let landed_on_f5 = run(layout.clone(), vec![down(f5), held(true, true), up()]);
    assert_eq!(landed_on_f5.emitted, vec![vec![key("F5")]]);
    assert_eq!(landed_on_f5.spawned, Vec::<String>::new());
    assert!(landed_on_f5.reports.is_empty(), "{:?}", landed_on_f5.reports);

    let landed_on_lock = run(layout, vec![held(true, true), down(lock), held(false, false), up()]);
    assert_eq!(landed_on_lock.spawned, vec!["omarchy lock".to_owned()]);
    assert_eq!(landed_on_lock.emitted, Vec::<Vec<Key>>::new());
}

#[test]
fn a_layout_change_during_a_touch_runs_the_button_it_landed_on() {
    let layout = preset();
    let f5 = x_of(&layout, Chord::Default, "f5");
    let mut changed = preset();
    for button in
        changed.layer_mut(Chord::Default).unwrap().groups.iter_mut().flat_map(|g| g.buttons.iter_mut())
    {
        if button.id.as_str() == "f5" {
            button.action = Action::Key { keys: vec!["F6".to_owned()] };
        }
    }
    let out = run(layout, vec![down(f5), Event::Layout(changed), up()]);
    assert_eq!(out.emitted, vec![vec![key("F5")]]);
}

#[test]
fn a_tap_still_presents_the_press_and_the_release() {
    let x = x_of(&preset(), Chord::Default, "f5");
    let out = run(preset(), tap(x).to_vec());
    assert_eq!(out.presents, 3, "start, press, release");
}

#[test]
fn actions_can_be_moved_to_the_daemon_thread() {
    fn movable<T: Send>(_: &T) {}
    let (report, _) = collecting();
    let actions = Actions::new(None, Box::new(RecordingSpawner::new()), report);
    movable(&actions);
    movable(&Actions::none());
}
