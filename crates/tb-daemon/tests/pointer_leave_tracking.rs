//! A Rune element that opts into `El::hover_tracks_bounds` hears, through `on_hover_change`, that
//! a held primary press lost the pointer when the host reports a pointer leave, and that it came
//! back before the release; the press itself is not ended. Public `rune` API only, on a headless
//! `rune::Runtime` (no window). The simulator does not opt in, so its touches keep their behaviour.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use rune::{Cx, El, Env, Runtime, col, el};

const STEP: f64 = 1.0 / 60.0;

#[derive(Default)]
struct Tracked {
    inside: Vec<bool>,
    presses: usize,
}

fn tracked_view(_: &Tracked, _: &mut Cx<Tracked>) -> El {
    col().size(200.0, 100.0).child(
        el().key("button")
            .size(80.0, 40.0)
            .hover_tracks_bounds()
            .on_press(|s: &mut Tracked| s.presses += 1)
            .on_hover_change(|s: &mut Tracked, inside, _| s.inside.push(inside)),
    )
}

/// A runtime whose button holds a primary press at (20, 20).
fn held() -> Runtime<Tracked> {
    let mut rt = Runtime::new(Tracked::default(), tracked_view, (200.0, 100.0), Env::new(true, 1.0));
    rt.frame(0.0);
    rt.pointer_move(20.0, 20.0);
    rt.pointer_down(20.0, 20.0);
    rt.frame(STEP);
    rt
}

#[test]
fn a_held_press_that_tracks_bounds_hears_the_pointer_leave_the_window() {
    let mut rt = held();
    assert_eq!(rt.state.inside, [true], "precondition: the press starts inside");

    rt.pointer_leave();
    rt.frame(STEP);

    assert_eq!(rt.state.inside, [true, false], "the leave during the press is heard as outside");
    assert!(rt.pressing(), "the leave does not end the held press");
}

#[test]
fn a_held_press_that_tracks_bounds_hears_the_pointer_come_back_before_the_release() {
    let mut rt = held();
    rt.pointer_leave();
    rt.frame(STEP);

    rt.pointer_move(30.0, 20.0);
    rt.frame(STEP);
    assert_eq!(rt.state.inside, [true, false, true], "coming back during the press is heard as inside");

    rt.pointer_up(30.0, 20.0);
    rt.frame(STEP);
    assert_eq!(rt.state.presses, 1, "the release back inside still presses the button");
}
