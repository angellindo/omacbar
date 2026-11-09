//! The simulator window shows the simulator's screen, fed in memory on each present: a function
//! given to `SimDisplay::on_present` sees the whole screen after every non-empty present that
//! succeeded, and the window's `show_screen` puts it in the window's Rune pixels. The window's
//! view draws them in the element `sim-screen` at the bar's proportions. Frames are built by hand
//! (no GPU) and the view runs in a headless `rune::Runtime`: no test opens a window.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use rune::{Env, Runtime};
use tb_core::{BarSize, Rect};
use tb_daemon::{Display, SIM_WINDOW, SimDisplay, SimWindow, sim_window_view};
use tb_render::Frame;

const BAR: BarSize = BarSize::MACBOOK_PRO;
const WHOLE_BAR: Rect = Rect { x: 0, y: 0, width: 2170, height: 60 };
const RED: [u8; 4] = [200, 30, 40, 255];
const BLUE: [u8; 4] = [20, 60, 220, 255];
const GREEN: [u8; 4] = [30, 180, 90, 255];

/// A path with nothing at it for one test (tests run in parallel).
fn fresh_dir(name: &str) -> PathBuf {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join("sim_window_screen").join(name);
    let _ = std::fs::remove_dir_all(&dir);
    let _ = std::fs::remove_file(&dir);
    dir
}

fn bar_frame(rgba: [u8; 4]) -> Frame {
    let (w, h) = (u32::from(BAR.width), u32::from(BAR.height));
    Frame { width: w, height: h, rgba: rgba.repeat((w * h) as usize) }
}

fn frame_files(dir: &Path) -> Vec<String> {
    let mut names: Vec<String> = std::fs::read_dir(dir)
        .unwrap()
        .map(|e| e.unwrap().file_name().into_string().unwrap())
        .filter(|n| n.starts_with("frame-") && Path::new(n).extension().is_some_and(|e| e == "png"))
        .collect();
    names.sort();
    names
}

/// A simulator in `dir` whose presents are recorded (each screen `on_present` saw) in the result.
fn recorded(dir: &Path) -> (SimDisplay, Arc<Mutex<Vec<Frame>>>) {
    let seen = Arc::new(Mutex::new(Vec::new()));
    let sink = Arc::clone(&seen);
    let sim = SimDisplay::new(BAR, dir).unwrap().on_present(move |screen: &Frame| {
        sink.lock().unwrap().push(screen.clone());
    });
    (sim, seen)
}

fn seen(log: &Arc<Mutex<Vec<Frame>>>) -> Vec<Frame> {
    log.lock().unwrap().clone()
}

fn close(a: f32, b: f32, tolerance: f32) -> bool {
    (a - b).abs() <= tolerance
}

#[test]
fn each_non_empty_present_shows_the_whole_screen_in_present_order() {
    let (mut sim, log) = recorded(&fresh_dir("in_order"));
    let mut screens = Vec::new();

    sim.present(&bar_frame(RED), &[WHOLE_BAR]).unwrap();
    screens.push(sim.screen().clone());
    // Partial damage: the screen (red with a blue F5-sized patch) is not the presented frame.
    sim.present(&bar_frame(BLUE), &[Rect { x: 826, y: 8, width: 160, height: 44 }]).unwrap();
    screens.push(sim.screen().clone());
    sim.present(&bar_frame(GREEN), &[Rect { x: 0, y: 0, width: 64, height: 60 }]).unwrap();
    screens.push(sim.screen().clone());

    let got = seen(&log);
    assert_eq!(got.len(), 3, "one call per non-empty present");
    for (i, (got, want)) in got.iter().zip(&screens).enumerate() {
        assert_eq!((got.width, got.height), (2170, 60), "call {i}: the whole screen");
        assert!(got == want, "call {i}: sees the screen right after its present");
    }
    assert!(got[1] != bar_frame(BLUE), "the screen, not the presented frame");
    assert_eq!(got[1].pixel(900, 30), BLUE, "inside the damage");
    assert_eq!(got[1].pixel(100, 30), RED, "outside the damage keeps the earlier screen");
}

#[test]
fn a_present_with_empty_damage_does_not_show_a_screen() {
    let (mut sim, log) = recorded(&fresh_dir("empty_damage"));
    sim.present(&bar_frame(RED), &[WHOLE_BAR]).unwrap();
    sim.present(&bar_frame(BLUE), &[]).unwrap();
    assert_eq!(seen(&log).len(), 1, "only the non-empty present");
}

#[test]
fn a_refused_present_does_not_show_a_screen() {
    let (mut sim, log) = recorded(&fresh_dir("refused"));
    sim.present(&bar_frame(RED), &[WHOLE_BAR]).unwrap();

    let small = Frame { width: 100, height: 60, rgba: BLUE.repeat(100 * 60) };
    assert!(sim.present(&small, &[Rect { x: 0, y: 0, width: 100, height: 60 }]).is_err(), "wrong size");
    let past = Rect { x: 2100, y: 0, width: 71, height: 60 };
    assert!(sim.present(&bar_frame(BLUE), &[past]).is_err(), "damage outside the bar");

    let got = seen(&log);
    assert_eq!(got.len(), 1, "refused presents are not shown");
    assert_eq!(got[0].pixel(2150, 30), RED);
}

#[test]
fn a_failed_png_write_does_not_show_a_screen() {
    let dir = fresh_dir("failed_write");
    let (mut sim, log) = recorded(&dir);
    sim.present(&bar_frame(RED), &[WHOLE_BAR]).unwrap();

    // A regular file where the frame directory was: the next PNG cannot be written.
    std::fs::remove_dir_all(&dir).unwrap();
    std::fs::write(&dir, b"not a directory").unwrap();
    assert!(sim.present(&bar_frame(BLUE), &[WHOLE_BAR]).is_err(), "the write fails");
    assert_eq!(seen(&log).len(), 1, "a failed present is not shown");

    std::fs::remove_file(&dir).unwrap();
    std::fs::create_dir_all(&dir).unwrap();
    sim.present(&bar_frame(BLUE), &[WHOLE_BAR]).unwrap();
    let got = seen(&log);
    assert_eq!(got.len(), 2, "the next good present is shown");
    assert_eq!(got[1].pixel(10, 10), BLUE);
}

#[test]
fn a_screen_is_shown_after_its_png_is_written_and_the_sequence_is_unchanged() {
    let dir = fresh_dir("png_first");
    let files_at_call = Arc::new(Mutex::new(Vec::new()));
    let sink = Arc::clone(&files_at_call);
    let watched = dir.clone();
    let mut sim = SimDisplay::new(BAR, &dir).unwrap().on_present(move |_: &Frame| {
        sink.lock().unwrap().push(frame_files(&watched));
    });

    sim.present(&bar_frame(RED), &[WHOLE_BAR]).unwrap();
    sim.present(&bar_frame(BLUE), &[]).unwrap();
    sim.present(&bar_frame(BLUE), &[Rect { x: 826, y: 8, width: 160, height: 44 }]).unwrap();

    let at_call = files_at_call.lock().unwrap().clone();
    assert_eq!(
        at_call,
        [vec!["frame-0001.png".to_owned()], vec!["frame-0001.png".to_owned(), "frame-0002.png".to_owned()]],
        "each call comes after its own PNG"
    );
    assert_eq!(frame_files(&dir), ["frame-0001.png", "frame-0002.png"]);
}

#[test]
fn a_later_on_present_replaces_the_earlier_one() {
    let first = Arc::new(Mutex::new(0_u32));
    let second = Arc::new(Mutex::new(0_u32));
    let (a, b) = (Arc::clone(&first), Arc::clone(&second));
    let mut sim = SimDisplay::new(BAR, fresh_dir("replaces"))
        .unwrap()
        .on_present(move |_: &Frame| *a.lock().unwrap() += 1)
        .on_present(move |_: &Frame| *b.lock().unwrap() += 1);

    sim.present(&bar_frame(RED), &[WHOLE_BAR]).unwrap();
    assert_eq!((*first.lock().unwrap(), *second.lock().unwrap()), (0, 1));
}

#[test]
fn a_new_simulator_window_shows_nothing_yet() {
    let (window, _touches) = SimWindow::new();
    assert_eq!(window.screen().size(), (0, 0));
}

#[test]
fn the_simulator_window_shows_a_presented_screen_at_bar_size() {
    let (window, _touches) = SimWindow::new();
    let mut sim = SimDisplay::new(BAR, fresh_dir("window_size")).unwrap().on_present(window.show_screen());
    sim.present(&bar_frame(RED), &[WHOLE_BAR]).unwrap();
    assert_eq!(window.screen().size(), (2170, 60));
}

#[test]
fn the_simulator_window_shows_a_screen_presented_from_another_thread() {
    let (window, _touches) = SimWindow::new();
    let sim = SimDisplay::new(BAR, fresh_dir("window_thread")).unwrap().on_present(window.show_screen());
    let presenter = std::thread::spawn(move || {
        let mut sim = sim;
        sim.present(&bar_frame(BLUE), &[Rect { x: 826, y: 8, width: 160, height: 44 }]).unwrap();
    });
    presenter.join().unwrap();
    assert_eq!(window.screen().size(), (2170, 60));
}

#[test]
fn the_simulator_window_is_the_bar_at_half_scale() {
    assert_eq!(SIM_WINDOW, (1085.0, 30.0));
}

#[test]
fn the_sim_screen_fills_a_half_scale_window() {
    let (window, _touches) = SimWindow::new();
    let mut rt = Runtime::new(window, sim_window_view, SIM_WINDOW, Env::new(true, 1.0));
    rt.frame(0.0);
    let ([x, y, w, h], opacity) = rt.presented("sim-screen").expect("sim-screen is presented");
    assert!(close(x, 0.0, 0.5) && close(y, 0.0, 0.5), "origin ({x}, {y})");
    assert!(close(w, 1085.0, 0.5) && close(h, 30.0, 0.5), "size {w} x {h}");
    assert!(close(opacity, 1.0, 0.01), "opacity {opacity}");
}

#[test]
fn the_sim_screen_keeps_bar_proportions_in_a_taller_window() {
    let (window, _touches) = SimWindow::new();
    let mut rt = Runtime::new(window, sim_window_view, (1085.0, 300.0), Env::new(true, 1.0));
    rt.frame(0.0);
    let ([x, y, w, h], _) = rt.presented("sim-screen").expect("sim-screen is presented");
    assert!(close(x, 0.0, 0.5) && close(w, 1085.0, 0.5), "full width: x {x}, w {w}");
    let ratio = w / h;
    let bar = 2170.0 / 60.0;
    assert!((ratio - bar).abs() <= bar * 0.005, "aspect {ratio}, bar {bar}");
    assert!(y >= -0.5 && y + h <= 300.5, "inside the window: y {y}, h {h}");
}

#[test]
fn the_sim_screen_keeps_bar_proportions_at_full_bar_size() {
    let (window, _touches) = SimWindow::new();
    let mut rt = Runtime::new(window, sim_window_view, (2170.0, 60.0), Env::new(true, 1.0));
    rt.frame(0.0);
    let ([x, y, w, h], _) = rt.presented("sim-screen").expect("sim-screen is presented");
    assert!(close(x, 0.0, 0.5) && close(y, 0.0, 0.5), "origin ({x}, {y})");
    assert!(close(w, 2170.0, 0.5) && close(h, 60.0, 0.5), "size {w} x {h}");
}

#[test]
fn the_sim_screen_is_presented_after_a_screen_is_shown() {
    let (window, _touches) = SimWindow::new();
    let mut sim =
        SimDisplay::new(BAR, fresh_dir("view_after_present")).unwrap().on_present(window.show_screen());
    let screen = window.screen().clone();
    let mut rt = Runtime::new(window, sim_window_view, SIM_WINDOW, Env::new(true, 1.0));
    rt.frame(0.0);
    sim.present(&bar_frame(RED), &[WHOLE_BAR]).unwrap();
    rt.frame(1.0 / 60.0);
    assert_eq!(screen.size(), (2170, 60), "the window's pixels hold the screen");
    let ([_, _, w, h], _) = rt.presented("sim-screen").expect("sim-screen is presented");
    assert!(close(w, 1085.0, 0.5) && close(h, 30.0, 0.5), "size {w} x {h}");
}
