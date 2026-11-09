//! A primary press on the simulator window's `sim-screen` is a `Touch::Down` at the matching bar
//! x (window x scaled to the 2170 px bar, floored), its release is a `Touch::Up`; moves land
//! nothing. Driven through a headless `rune::Runtime` of `sim_window_view` (no test opens a
//! window); touches are read from `WindowTouches` after the runtime (and with it the window's
//! state) is dropped, on a helper thread with a timeout, so a read never blocks a test.
//! Expected bar x values come from arithmetic: at `SIM_WINDOW` one window point is two bar px.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::sync::mpsc;
use std::time::Duration;

use rune::{Env, Runtime};
use tb_core::{BarSize, BarTheme, Chord, Rect, SpacingRules, place, presets};
use tb_daemon::fake::{FlatRenderer, Presented, RecordingDisplay};
use tb_daemon::{
    Daemon, SIM_WINDOW, SimWindow, Touch, TouchSource, WindowTouches, forward_touches, sim_window_view,
};

const WAIT: Duration = Duration::from_secs(10);

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

/// A headless runtime of the simulator window at `size`, laid out once; and its touches.
fn window(size: (f32, f32)) -> (Runtime<SimWindow>, WindowTouches) {
    let (state, touches) = SimWindow::new();
    let mut rt = Runtime::new(state, sim_window_view, size, Env::new(true, 1.0));
    rt.frame(0.0);
    (rt, touches)
}

fn press(rt: &mut Runtime<SimWindow>, x: f32, y: f32) {
    rt.pointer_down(x, y);
    rt.frame(1.0 / 60.0);
}

fn release(rt: &mut Runtime<SimWindow>, x: f32, y: f32) {
    rt.pointer_up(x, y);
    rt.frame(1.0 / 60.0);
}

fn move_to(rt: &mut Runtime<SimWindow>, x: f32, y: f32) {
    rt.pointer_move(x, y);
    rt.frame(1.0 / 60.0);
}

/// The touches of one press and release at `(x, y)` in a window of `size`.
fn click(size: (f32, f32), x: f32, y: f32) -> Vec<Touch> {
    let (mut rt, touches) = window(size);
    press(&mut rt, x, y);
    release(&mut rt, x, y);
    drop(rt);
    drain(touches)
}

fn f5() -> Rect {
    let layout = presets::default_layout();
    let placed =
        place(layout.layer(Chord::Default).unwrap(), BarSize::MACBOOK_PRO, &SpacingRules::STANDARD).unwrap();
    placed.iter().find(|p| p.id.as_str() == "f5").unwrap().rect
}

#[test]
fn a_click_at_half_scale_is_a_touch_down_at_twice_the_window_x_then_up() {
    // 453.25 pt * 2 = 906.5 bar px, floored to 906.
    assert_eq!(click(SIM_WINDOW, 453.25, 15.0), [Touch::Down { x: 906 }, Touch::Up]);
}

#[test]
fn a_click_on_a_full_size_window_lands_on_the_bar_pixel_under_it() {
    // One window point is one bar px: 1234.5 floors to 1234.
    assert_eq!(click((2170.0, 60.0), 1234.5, 30.0), [Touch::Down { x: 1234 }, Touch::Up]);
}

#[test]
fn a_click_at_the_far_left_is_bar_x_zero() {
    assert_eq!(click(SIM_WINDOW, 0.25, 15.0), [Touch::Down { x: 0 }, Touch::Up]);
}

#[test]
fn a_click_at_the_far_right_is_the_last_bar_pixel() {
    // 1084.9 pt * 2 = 2169.8 bar px, floored to 2169.
    assert_eq!(click(SIM_WINDOW, 1084.9, 15.0), [Touch::Down { x: 2169 }, Touch::Up]);
}

#[test]
fn the_vertical_position_of_a_click_does_not_change_its_bar_x() {
    assert_eq!(click(SIM_WINDOW, 100.25, 2.0), [Touch::Down { x: 200 }, Touch::Up]);
    assert_eq!(click(SIM_WINDOW, 100.25, 28.0), [Touch::Down { x: 200 }, Touch::Up]);
}

#[test]
fn moving_a_pressed_pointer_lands_no_touch_until_the_release() {
    let (mut rt, touches) = window(SIM_WINDOW);
    press(&mut rt, 453.25, 15.0);
    move_to(&mut rt, 600.0, 15.0);
    move_to(&mut rt, 900.0, 20.0);
    release(&mut rt, 900.0, 20.0);
    drop(rt);
    assert_eq!(drain(touches), [Touch::Down { x: 906 }, Touch::Up]);
}

#[test]
fn a_release_outside_the_window_still_lifts_the_touch() {
    let (mut rt, touches) = window(SIM_WINDOW);
    press(&mut rt, 453.25, 15.0);
    move_to(&mut rt, 1200.0, 50.0);
    rt.pointer_leave();
    rt.frame(1.0 / 60.0);
    release(&mut rt, 1200.0, 50.0);
    drop(rt);
    assert_eq!(drain(touches), [Touch::Down { x: 906 }, Touch::Up]);
}

#[test]
fn two_clicks_are_two_touches_in_order() {
    let (mut rt, touches) = window(SIM_WINDOW);
    press(&mut rt, 453.25, 15.0);
    release(&mut rt, 453.25, 15.0);
    press(&mut rt, 50.25, 15.0);
    release(&mut rt, 50.25, 15.0);
    drop(rt);
    assert_eq!(drain(touches), [Touch::Down { x: 906 }, Touch::Up, Touch::Down { x: 100 }, Touch::Up]);
}

#[test]
fn a_window_that_nobody_clicked_closes_its_touches_without_any() {
    let (rt, touches) = window(SIM_WINDOW);
    drop(rt);
    assert_eq!(drain(touches), Vec::<Touch>::new());
}

#[test]
fn a_press_beside_the_sim_screen_in_a_taller_window_lands_no_touch() {
    let (mut rt, touches) = window((1085.0, 300.0));
    let ([_, y, _, h], _) = rt.presented("sim-screen").expect("sim-screen is presented");
    // The black band above the bar shape if there is one, else the band below it.
    let band_y = if y >= 2.0 { y / 2.0 } else { (y + h + 300.0) / 2.0 };
    assert!(band_y < y || band_y > y + h, "precondition: {band_y} is outside sim-screen ({y}, {h})");
    press(&mut rt, 453.25, band_y);
    release(&mut rt, 453.25, band_y);
    drop(rt);
    assert_eq!(drain(touches), Vec::<Touch>::new());
}

#[test]
fn a_click_on_the_sim_screen_of_a_taller_window_maps_by_its_width() {
    let (mut rt, touches) = window((1085.0, 300.0));
    let ([_, y, _, h], _) = rt.presented("sim-screen").expect("sim-screen is presented");
    press(&mut rt, 453.25, y + h / 2.0);
    release(&mut rt, 453.25, y + h / 2.0);
    drop(rt);
    assert_eq!(drain(touches), [Touch::Down { x: 906 }, Touch::Up]);
}

#[test]
fn a_click_over_f5_in_the_window_presses_and_releases_f5_on_the_daemon() {
    let f5 = f5();
    let centre = f5.x + f5.width / 2;
    let (mut rt, mut touches) = window(SIM_WINDOW);

    let (done, result) = mpsc::channel::<Vec<Presented>>();
    std::thread::spawn(move || {
        let (events, received) = mpsc::channel();
        let forwarder = std::thread::spawn(move || forward_touches(&mut touches, &events).unwrap());
        let mut daemon = Daemon::new(
            presets::default_layout(),
            BarTheme::omarchy_fallback(),
            FlatRenderer,
            RecordingDisplay::new(),
        );
        daemon.run(received).unwrap();
        forwarder.join().unwrap();
        let _ = done.send(daemon.display().presents().to_vec());
    });

    // Half a bar pixel past the centre, in window points: floors back to the centre.
    let x = (f32::from(centre) + 0.5) / 2.0;
    press(&mut rt, x, 15.0);
    release(&mut rt, x, 15.0);
    drop(rt);

    let presents = result.recv_timeout(WAIT).expect("the daemon run ends once the window's state is dropped");
    assert_eq!(presents.len(), 3, "start, F5 pressed, F5 released");
    assert_eq!(presents[1].damage, [f5], "the press shows only F5");
    assert_eq!(presents[2].damage, [f5], "the release shows only F5");
    assert!(presents[2].frame == presents[0].frame, "released is the start frame again");
    assert!(presents[1].frame != presents[0].frame, "pressed differs from the start frame");
}
