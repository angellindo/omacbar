//! End to end: a press in the simulator window runs its button's action only when it is released
//! over the button it landed on, and the bar never shows a button pressed after the gesture ends.
//! Real Rune events (`pointer_down`/`move`/`up`/`leave`, Escape, `window_focus(false)`) drive a
//! headless `rune::Runtime` of `sim_window_view`; its `WindowTouches` go through
//! `forward_touches` to the real `Daemon` (CPU `FlatRenderer`, `RecordingDisplay`) whose actions
//! are real: keys on `VirtualKeyboard::record_to` (the `--emit PATH` keyboard, a file) and
//! commands through `ShSpawner` (`sh -c`). The only command is a harmless marker,
//! `printf x >> <test dir>/mark`, on button `mark` added to the default layer beside `blank`
//! (a button with no action); every path is under cargo's temp dir. Record numbers are from
//! `/usr/include/linux/input-event-codes.h`: `EV_SYN` 0, `EV_KEY` 1, `KEY_F5` 63.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, mpsc};
use std::thread;
use std::time::{Duration, Instant};

use rune::{Env, Mods, PhysicalInput, PhysicalKeyCode, Runtime};
use tb_core::{
    Action, BarSize, BarTheme, Button, Chord, Group, Layout, Placed, SpacingRules, place, presets,
};
use tb_daemon::fake::{FlatRenderer, Presented, RecordingDisplay};
use tb_daemon::{
    ActionError, Actions, Daemon, Report, SIM_WINDOW, ShSpawner, SimWindow, VirtualKeyboard, forward_touches,
    sim_window_view,
};

/// How long a run may take to end once the window's state is dropped.
const WAIT: Duration = Duration::from_secs(10);
/// How long a started marker command may take to write (loaded laptop).
const SETTLE: Duration = Duration::from_secs(5);
/// How long to keep watching for a marker write that must not happen, or must not repeat.
const QUIET: Duration = Duration::from_secs(1);
const STEP: f64 = 1.0 / 60.0;
const FULL: (f32, f32) = (2170.0, 60.0);

const EV_SYN: u16 = 0;
const EV_KEY: u16 = 1;
const KEY_F5: u16 = 63;

/// The default preset plus a last group: `mark` (appends `x` to `mark`) and `blank` (no action).
fn layout(mark: &Path) -> Layout {
    let mut layout = presets::default_layout();
    let run = format!("printf x >> '{}'", mark.display());
    layout.layer_mut(Chord::Default).unwrap().groups.push(Group {
        buttons: vec![
            Button::new("mark", "mark", Action::Command { run }),
            Button::new("blank", "blank", Action::None),
        ],
    });
    layout
}

fn placed() -> Vec<Placed> {
    let layout = layout(Path::new("unused"));
    place(layout.layer(Chord::Default).unwrap(), BarSize::MACBOOK_PRO, &SpacingRules::STANDARD).unwrap()
}

/// Window points over the centre column of button `id` in a window of `size`, vertically centred:
/// half a bar px past the centre, so the window's floor lands back on it.
fn over(size: (f32, f32), id: &str) -> (f32, f32) {
    let r = placed().into_iter().find(|p| p.id.as_str() == id).unwrap().rect;
    let x = (f32::from(r.x + r.width / 2) + 0.5) * size.0 / f32::from(BarSize::MACBOOK_PRO.width);
    (x, size.1 / 2.0)
}

/// The button a gesture slides or releases onto instead of `id`.
fn other(id: &str) -> &'static str {
    if id == "f5" { "f6" } else { "f5" }
}

fn press(rt: &mut Runtime<SimWindow>, (x, y): (f32, f32)) {
    rt.pointer_down(x, y);
    rt.frame(STEP);
}

fn release(rt: &mut Runtime<SimWindow>, (x, y): (f32, f32)) {
    rt.pointer_up(x, y);
    rt.frame(STEP);
}

fn move_to(rt: &mut Runtime<SimWindow>, (x, y): (f32, f32)) {
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

fn lose_focus(rt: &mut Runtime<SimWindow>) {
    rt.window_focus(false);
    rt.frame(STEP);
}

/// What one window run did.
struct Outcome {
    keys: PathBuf,
    mark: PathBuf,
    presents: Vec<Presented>,
    reports: Arc<Mutex<Vec<String>>>,
}

/// Runs `gesture` on a window of `size` wired to a daemon with real actions, in a fresh dir
/// named `test`; returns once the daemon run has ended.
fn run(test: &str, size: (f32, f32), gesture: impl FnOnce(&mut Runtime<SimWindow>)) -> Outcome {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join("window_press_release").join(test);
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let (keys, mark) = (dir.join("keys.bin"), dir.join("mark"));
    let reports = Arc::new(Mutex::new(Vec::new()));
    let (state, mut touches) = SimWindow::new();
    let mut rt = Runtime::new(state, sim_window_view, size, Env::new(true, 1.0));
    rt.frame(0.0);

    let (done, result) = mpsc::channel::<Vec<Presented>>();
    let (keys_at, mark_at, sink) = (keys.clone(), mark.clone(), Arc::clone(&reports));
    thread::spawn(move || {
        let (events, received) = mpsc::channel();
        let forwarder = thread::spawn(move || forward_touches(&mut touches, &events).unwrap());
        let report: Report = Arc::new(move |e: ActionError| sink.lock().unwrap().push(e.to_string()));
        let keyboard = VirtualKeyboard::record_to(&keys_at).unwrap();
        let actions =
            Actions::new(Some(Box::new(keyboard)), Box::new(ShSpawner::new(Arc::clone(&report))), report);
        let mut daemon = Daemon::new(
            layout(&mark_at),
            BarTheme::omarchy_fallback(),
            FlatRenderer,
            RecordingDisplay::new(),
        )
        .with_actions(actions);
        daemon.run(received).unwrap();
        forwarder.join().unwrap();
        let _ = done.send(daemon.display().presents().to_vec());
    });

    gesture(&mut rt);
    drop(rt);
    let presents = result.recv_timeout(WAIT).expect("the daemon run ends once the window's state is dropped");
    Outcome { keys, mark, presents, reports }
}

impl Outcome {
    /// (type, code, value) of every record in the key file.
    fn key_records(&self) -> Vec<(u16, u16, i32)> {
        let bytes = std::fs::read(&self.keys).unwrap();
        assert_eq!(bytes.len() % 24, 0, "whole records only, got {} bytes", bytes.len());
        bytes
            .as_chunks::<24>()
            .0
            .iter()
            .map(|r| {
                (
                    u16::from_ne_bytes([r[16], r[17]]),
                    u16::from_ne_bytes([r[18], r[19]]),
                    i32::from_ne_bytes([r[20], r[21], r[22], r[23]]),
                )
            })
            .collect()
    }

    /// The marker's text once `QUIET` has passed with no change (after it first appears, if
    /// `expect_write`, waiting up to `SETTLE` for that).
    fn marker(&self, expect_write: bool) -> String {
        let read = || std::fs::read_to_string(&self.mark).unwrap_or_default();
        if expect_write {
            let start = Instant::now();
            while read().is_empty() && start.elapsed() < SETTLE {
                thread::sleep(Duration::from_millis(10));
            }
        }
        thread::sleep(QUIET);
        read()
    }

    /// For every present, the ids of the buttons it shows pressed (`pressed_fill` at their centre).
    fn pressed_history(&self) -> Vec<Vec<String>> {
        let pressed = BarTheme::omarchy_fallback().pressed_fill;
        let pressed = [pressed.r, pressed.g, pressed.b, pressed.a];
        let placed = placed();
        self.presents
            .iter()
            .map(|p| {
                placed
                    .iter()
                    .filter(|b| {
                        let r = b.rect;
                        p.frame.pixel(u32::from(r.x + r.width / 2), u32::from(r.y + r.height / 2)) == pressed
                    })
                    .map(|b| b.id.as_str().to_owned())
                    .collect()
            })
            .collect()
    }

    fn reports(&self) -> Vec<String> {
        self.reports.lock().unwrap().clone()
    }
}

fn f5_pressed_and_released() -> Vec<(u16, u16, i32)> {
    vec![(EV_KEY, KEY_F5, 1), (EV_SYN, 0, 0), (EV_KEY, KEY_F5, 0), (EV_SYN, 0, 0)]
}

/// The bar showed `id` pressed once, then nothing: start, pressed, released.
fn assert_pressed_once_then_released(out: &Outcome, id: &str) {
    let none = Vec::<String>::new();
    assert_eq!(out.pressed_history(), [none.clone(), vec![id.to_owned()], none], "pressed per present");
}

/// Nothing ran (no key record, no marker, no report) and nothing stayed pressed.
fn assert_nothing_ran(out: &Outcome, id: &str) {
    assert_eq!(out.key_records(), Vec::new(), "no key record after pressing {id}");
    assert_eq!(out.marker(false), "", "no marker after pressing {id}");
    assert_eq!(out.reports(), Vec::<String>::new());
    assert_pressed_once_then_released(out, id);
}

/// `id`'s action ran exactly once and nothing else did.
fn assert_ran_once(out: &Outcome, id: &str) {
    if id == "f5" {
        assert_eq!(out.key_records(), f5_pressed_and_released());
        assert_eq!(out.marker(false), "");
    } else {
        assert_eq!(out.key_records(), Vec::new());
        assert_eq!(out.marker(true), "x", "the marker command ran exactly once");
    }
    assert_eq!(out.reports(), Vec::<String>::new());
}

/// Runs `gesture(rt, pressed_id)` for F5 (a key) and for `mark` (a command); nothing may run.
fn neither_runs(test: &str, gesture: impl Fn(&mut Runtime<SimWindow>, &str)) {
    for id in ["f5", "mark"] {
        let out = run(&format!("{test}-{id}"), SIM_WINDOW, |rt| gesture(rt, id));
        assert_nothing_ran(&out, id);
    }
}

// ---------- a normal click ----------

#[test]
fn a_click_on_f5_presses_and_releases_f5_once_at_half_and_full_scale() {
    for (name, size) in [("half", SIM_WINDOW), ("full", FULL)] {
        let out = run(&format!("click-f5-{name}"), size, |rt| {
            press(rt, over(size, "f5"));
            release(rt, over(size, "f5"));
        });
        assert_ran_once(&out, "f5");
        assert_pressed_once_then_released(&out, "f5");
    }
}

#[test]
fn a_click_on_the_command_button_runs_its_command_once() {
    let out = run("click-mark", SIM_WINDOW, |rt| {
        press(rt, over(SIM_WINDOW, "mark"));
        release(rt, over(SIM_WINDOW, "mark"));
    });
    assert_ran_once(&out, "mark");
    assert_pressed_once_then_released(&out, "mark");
}

#[test]
fn small_moves_within_the_button_before_the_release_still_run_it_once() {
    for id in ["f5", "mark"] {
        let out = run(&format!("small-moves-{id}"), SIM_WINDOW, |rt| {
            let (x, y) = over(SIM_WINDOW, id);
            press(rt, (x, y));
            move_to(rt, (x + 3.0, y - 4.0));
            move_to(rt, (x - 3.0, y + 4.0));
            release(rt, (x - 3.0, y + 4.0));
        });
        assert_ran_once(&out, id);
        assert_pressed_once_then_released(&out, id);
    }
}

#[test]
fn a_click_on_a_button_with_no_action_runs_nothing_and_leaves_nothing_pressed() {
    let out = run("click-blank", SIM_WINDOW, |rt| {
        press(rt, over(SIM_WINDOW, "blank"));
        release(rt, over(SIM_WINDOW, "blank"));
    });
    assert_eq!(out.key_records(), Vec::new());
    assert_eq!(out.marker(false), "");
    assert_eq!(out.reports(), Vec::<String>::new());
    assert_eq!(out.pressed_history().last().unwrap(), &Vec::<String>::new());
}

// ---------- no action: released elsewhere or cancelled ----------

#[test]
fn releasing_over_another_button_after_moving_there_runs_nothing() {
    neither_runs("moved-to-other", |rt, id| {
        press(rt, over(SIM_WINDOW, id));
        move_to(rt, over(SIM_WINDOW, other(id)));
        release(rt, over(SIM_WINDOW, other(id)));
    });
}

#[test]
fn releasing_over_another_button_with_no_move_runs_nothing() {
    neither_runs("released-on-other", |rt, id| {
        press(rt, over(SIM_WINDOW, id));
        release(rt, over(SIM_WINDOW, other(id)));
    });
}

#[test]
fn leaving_the_window_below_the_bar_and_releasing_there_runs_nothing() {
    neither_runs("left-below", |rt, id| {
        let (x, _) = over(SIM_WINDOW, id);
        press(rt, over(SIM_WINDOW, id));
        move_to(rt, (x, SIM_WINDOW.1 + 20.0));
        rt.pointer_leave();
        rt.frame(STEP);
        release(rt, (x, SIM_WINDOW.1 + 20.0));
    });
}

#[test]
fn leaving_the_window_beside_the_bar_and_releasing_there_runs_nothing() {
    neither_runs("left-beside", |rt, id| {
        let beside = (SIM_WINDOW.0 + 40.0, SIM_WINDOW.1 / 2.0);
        press(rt, over(SIM_WINDOW, id));
        move_to(rt, beside);
        rt.pointer_leave();
        rt.frame(STEP);
        release(rt, beside);
    });
}

#[test]
fn coming_back_over_the_button_after_leaving_the_window_runs_nothing() {
    neither_runs("left-and-back", |rt, id| {
        let (x, y) = over(SIM_WINDOW, id);
        press(rt, (x, y));
        move_to(rt, (x, SIM_WINDOW.1 + 20.0));
        rt.pointer_leave();
        rt.frame(STEP);
        move_to(rt, (x, y));
        release(rt, (x, y));
    });
}

#[test]
fn escape_while_pressing_runs_nothing() {
    neither_runs("escape", |rt, id| {
        press(rt, over(SIM_WINDOW, id));
        escape(rt);
        release(rt, over(SIM_WINDOW, id));
    });
}

#[test]
fn losing_window_focus_while_pressing_runs_nothing() {
    neither_runs("focus-lost", |rt, id| {
        press(rt, over(SIM_WINDOW, id));
        lose_focus(rt);
        release(rt, over(SIM_WINDOW, id));
    });
}

// ---------- highlight follows the landing button ----------

#[test]
fn sliding_off_and_back_shows_the_button_released_while_away_and_runs_it_once() {
    for id in ["f5", "mark"] {
        let out = run(&format!("off-and-back-{id}"), SIM_WINDOW, |rt| {
            press(rt, over(SIM_WINDOW, id));
            move_to(rt, over(SIM_WINDOW, other(id)));
            move_to(rt, over(SIM_WINDOW, id));
            release(rt, over(SIM_WINDOW, id));
        });
        assert_ran_once(&out, id);
        let (none, landed) = (Vec::<String>::new(), vec![id.to_owned()]);
        assert_eq!(
            out.pressed_history(),
            [none.clone(), landed.clone(), none.clone(), landed, none],
            "start, pressed, released while over {}, pressed again, released",
            other(id)
        );
    }
}

#[test]
fn a_cancelled_press_then_a_click_runs_the_command_once() {
    let out = run("cancel-then-click", SIM_WINDOW, |rt| {
        press(rt, over(SIM_WINDOW, "mark"));
        escape(rt);
        release(rt, over(SIM_WINDOW, "mark"));
        press(rt, over(SIM_WINDOW, "mark"));
        release(rt, over(SIM_WINDOW, "mark"));
    });
    assert_ran_once(&out, "mark");
    assert_eq!(out.pressed_history().last().unwrap(), &Vec::<String>::new());
}
