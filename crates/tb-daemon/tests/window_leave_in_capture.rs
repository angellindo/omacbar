//! Regression (TB-44 review 1): the pointer leaves the simulator window while a press holds it,
//! with no move outside first. The host's only signal is `CursorLeft` → `Runtime::pointer_leave`;
//! its release (`MouseInput Released`) comes at `Runtime::pointer()`, the last position inside
//! (rune `window/events.rs`). The tests send exactly those calls: no coordinates outside the window
//! and no signal the host does not deliver. Coming back is a host `CursorMoved` inside.
//! End to end as in `window_press_release.rs`: real Rune runtime → `WindowTouches` →
//! `forward_touches` → `Daemon` with keys on `VirtualKeyboard::record_to` and `ShSpawner` running
//! `printf x >> <test dir>/mark`; presents are watched live so the highlight is checked when it
//! should change, not only at the end. `EV_KEY` 1, `KEY_F5` 63, `EV_SYN` 0
//! (`/usr/include/linux/input-event-codes.h`).
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, mpsc};
use std::thread;
use std::time::{Duration, Instant};

use rune::{Env, Runtime};
use tb_core::{
    Action, BarSize, BarTheme, Button, Chord, Group, Layout, Placed, Rect, SpacingRules, place, presets,
};
use tb_daemon::fake::FlatRenderer;
use tb_daemon::{
    ActionError, Actions, Daemon, Display, DisplayError, Report, SIM_WINDOW, ShSpawner, SimWindow,
    VirtualKeyboard, forward_touches, sim_window_view,
};
use tb_render::Frame;

const WAIT: Duration = Duration::from_secs(10);
/// How long a present or a marker write may take (loaded laptop).
const SETTLE: Duration = Duration::from_secs(5);
/// How long to keep watching for something that must not happen.
const QUIET: Duration = Duration::from_secs(1);
const STEP: f64 = 1.0 / 60.0;

const EV_SYN: u16 = 0;
const EV_KEY: u16 = 1;
const KEY_F5: u16 = 63;

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

fn over(id: &str) -> (f32, f32) {
    let r = placed().into_iter().find(|p| p.id.as_str() == id).unwrap().rect;
    let x = (f32::from(r.x + r.width / 2) + 0.5) * SIM_WINDOW.0 / f32::from(BarSize::MACBOOK_PRO.width);
    (x, SIM_WINDOW.1 / 2.0)
}

/// A display whose presents the test sees as they happen.
#[derive(Clone, Default)]
struct LiveDisplay(Arc<Mutex<Vec<Frame>>>);

impl Display for LiveDisplay {
    fn present(&mut self, frame: &Frame, _damage: &[Rect]) -> Result<(), DisplayError> {
        self.0.lock().unwrap().push(frame.clone());
        Ok(())
    }
}

impl LiveDisplay {
    /// The ids each present so far shows pressed (`pressed_fill` at their centre).
    fn history(&self) -> Vec<Vec<String>> {
        let p = BarTheme::omarchy_fallback().pressed_fill;
        let p = [p.r, p.g, p.b, p.a];
        let placed = placed();
        let frames = self.0.lock().unwrap();
        frames
            .iter()
            .map(|f| {
                placed
                    .iter()
                    .filter(|b| {
                        f.pixel(
                            u32::from(b.rect.x + b.rect.width / 2),
                            u32::from(b.rect.y + b.rect.height / 2),
                        ) == p
                    })
                    .map(|b| b.id.as_str().to_owned())
                    .collect()
            })
            .collect()
    }

    /// The history once it has `n` presents, or as it stands after `SETTLE`.
    fn history_at(&self, n: usize) -> Vec<Vec<String>> {
        let start = Instant::now();
        while self.0.lock().unwrap().len() < n && start.elapsed() < SETTLE {
            thread::sleep(Duration::from_millis(5));
        }
        self.history()
    }
}

/// One window run: the gesture gets the runtime and the live display.
struct Outcome {
    keys: PathBuf,
    mark: PathBuf,
    display: LiveDisplay,
    reports: Arc<Mutex<Vec<String>>>,
}

fn run(test: &str, gesture: impl FnOnce(&mut Runtime<SimWindow>, &LiveDisplay)) -> Outcome {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join("window_leave_in_capture").join(test);
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let (keys, mark) = (dir.join("keys.bin"), dir.join("mark"));
    let reports = Arc::new(Mutex::new(Vec::new()));
    let display = LiveDisplay::default();
    let (state, mut touches) = SimWindow::new();
    let mut rt = Runtime::new(state, sim_window_view, SIM_WINDOW, Env::new(true, 1.0));
    rt.frame(0.0);

    let (done, ended) = mpsc::channel::<()>();
    let (keys_at, mark_at, sink, shown) = (keys.clone(), mark.clone(), Arc::clone(&reports), display.clone());
    thread::spawn(move || {
        let (events, received) = mpsc::channel();
        let forwarder = thread::spawn(move || forward_touches(&mut touches, &events).unwrap());
        let report: Report = Arc::new(move |e: ActionError| sink.lock().unwrap().push(e.to_string()));
        let keyboard = VirtualKeyboard::record_to(&keys_at).unwrap();
        let actions =
            Actions::new(Some(Box::new(keyboard)), Box::new(ShSpawner::new(Arc::clone(&report))), report);
        let mut daemon = Daemon::new(layout(&mark_at), BarTheme::omarchy_fallback(), FlatRenderer, shown)
            .with_actions(actions);
        daemon.run(received).unwrap();
        forwarder.join().unwrap();
        let _ = done.send(());
    });

    gesture(&mut rt, &display);
    drop(rt);
    ended.recv_timeout(WAIT).expect("the daemon run ends once the window's state is dropped");
    Outcome { keys, mark, display, reports }
}

impl Outcome {
    fn key_records(&self) -> Vec<(u16, u16, i32)> {
        let bytes = std::fs::read(&self.keys).unwrap();
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

    fn marker(&self, expect_write: bool) -> String {
        let read = || std::fs::read_to_string(&self.mark).unwrap_or_default();
        let start = Instant::now();
        while expect_write && read().is_empty() && start.elapsed() < SETTLE {
            thread::sleep(Duration::from_millis(10));
        }
        thread::sleep(QUIET);
        read()
    }
}

fn press(rt: &mut Runtime<SimWindow>, (x, y): (f32, f32)) {
    rt.pointer_move(x, y);
    rt.pointer_down(x, y);
    rt.frame(STEP);
}

/// The host's `CursorLeft` during the press: `pointer_leave` and a redraw.
fn leave(rt: &mut Runtime<SimWindow>) {
    rt.pointer_leave();
    rt.frame(STEP);
}

/// The host's left-button release: at the pointer Rune last saw, as `window/events.rs` does.
fn host_release(rt: &mut Runtime<SimWindow>) {
    let (x, y) = rt.pointer().expect("the press keeps the last position inside");
    rt.pointer_up(x, y);
    rt.frame(STEP);
}

fn none() -> Vec<String> {
    Vec::new()
}

fn assert_nothing_ran(out: &Outcome, id: &str) {
    assert_eq!(out.key_records(), Vec::new(), "no key record after pressing {id}");
    assert_eq!(out.marker(false), "", "no marker after pressing {id}");
    assert_eq!(*out.reports.lock().unwrap(), Vec::<String>::new());
}

#[test]
fn leaving_the_window_while_pressing_clears_the_highlight_at_the_leave() {
    for id in ["f5", "mark"] {
        let mut at_leave = Vec::new();
        run(&format!("highlight-{id}"), |rt, shown| {
            press(rt, over(id));
            assert_eq!(shown.history_at(2), [none(), vec![id.to_owned()]], "{id} shows pressed");
            leave(rt);
            at_leave = shown.history_at(3);
            host_release(rt);
        });
        assert_eq!(
            at_leave,
            [none(), vec![id.to_owned()], none()],
            "leaving the window while pressing {id} shows nothing pressed before the release"
        );
    }
}

#[test]
fn leaving_the_window_while_pressing_then_releasing_runs_nothing() {
    for id in ["f5", "mark"] {
        let out = run(&format!("leave-release-{id}"), |rt, _| {
            press(rt, over(id));
            leave(rt);
            host_release(rt);
        });
        assert_nothing_ran(&out, id);
        assert_eq!(out.display.history().last().unwrap(), &none(), "nothing stays pressed");
    }
}

#[test]
fn leaving_and_coming_back_over_the_same_button_then_releasing_runs_nothing() {
    for id in ["f5", "mark"] {
        let out = run(&format!("leave-back-{id}"), |rt, _| {
            let (x, y) = over(id);
            press(rt, (x, y));
            leave(rt);
            rt.pointer_move(x, y);
            rt.frame(STEP);
            host_release(rt);
        });
        assert_nothing_ran(&out, id);
        assert_eq!(out.display.history().last().unwrap(), &none(), "nothing stays pressed");
    }
}

#[test]
fn the_click_after_a_press_that_left_the_window_runs_exactly_once() {
    for id in ["f5", "mark"] {
        let out = run(&format!("leave-then-click-{id}"), |rt, _| {
            press(rt, over(id));
            leave(rt);
            host_release(rt);
            press(rt, over(id));
            host_release(rt);
        });
        if id == "f5" {
            assert_eq!(
                out.key_records(),
                [(EV_KEY, KEY_F5, 1), (EV_SYN, 0, 0), (EV_KEY, KEY_F5, 0), (EV_SYN, 0, 0)],
                "F5 pressed and released exactly once"
            );
            assert_eq!(out.marker(false), "");
        } else {
            assert_eq!(out.key_records(), Vec::new());
            assert_eq!(out.marker(true), "x", "the marker command ran exactly once");
        }
        assert_eq!(*out.reports.lock().unwrap(), Vec::<String>::new());
        assert_eq!(out.display.history().last().unwrap(), &none());
    }
}
