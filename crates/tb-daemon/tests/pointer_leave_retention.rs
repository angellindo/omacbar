//! Without `El::hover_tracks_bounds`, a Rune element that holds a primary press keeps hearing the
//! pointer as inside after the host reports a pointer leave, until the release: Rune's default
//! held-press retention, which the simulator window relies on. Public `rune` API only, on a
//! headless `rune::Runtime` (no window).
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use rune::{Cx, El, Env, Runtime, col, el};

const STEP: f64 = 1.0 / 60.0;

#[derive(Default)]
struct Retained {
    inside: Vec<bool>,
}

fn retained_view(_: &Retained, _: &mut Cx<Retained>) -> El {
    col().size(200.0, 100.0).child(
        el().key("button")
            .size(80.0, 40.0)
            .on_press(|_: &mut Retained| {})
            .on_hover_change(|s: &mut Retained, inside, _| s.inside.push(inside)),
    )
}

#[test]
fn a_held_press_without_bounds_tracking_keeps_hearing_inside_after_the_pointer_leaves() {
    let mut rt = Runtime::new(Retained::default(), retained_view, (200.0, 100.0), Env::new(true, 1.0));
    rt.frame(0.0);
    rt.pointer_move(20.0, 20.0);
    rt.pointer_down(20.0, 20.0);
    rt.frame(STEP);
    assert_eq!(rt.state.inside, [true], "precondition: the press starts inside");

    rt.pointer_leave();
    rt.frame(STEP);

    assert_eq!(rt.state.inside, [true], "no outside is heard while the press is held");
    assert!(rt.pressing(), "the leave does not end the held press");
}
