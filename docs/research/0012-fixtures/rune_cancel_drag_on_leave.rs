//! TB-51 prototype check: the opt-in `El::cancel_drag_on_leave` on a bare Rune runtime, through
//! the public API only, and the control without it (the pin's own behaviour, as Rune's
//! `runtime/tests/pointer_drag.rs` "the deferred leave lands on release").
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use rune::{Cx, El, Env, Phase, Runtime, el};

type State = Vec<(Phase, bool)>;

fn pad(opt_in: bool) -> El {
    let pad = el().key("pad").size(100.0, 100.0);
    let pad = if opt_in { pad.cancel_drag_on_leave() } else { pad };
    pad.on_drag(|s: &mut State, d: &mut rune::Drag, _: &mut rune::Tx| s.push((d.phase, d.cancelled())))
}

fn with_option(_: &State, _: &mut Cx<State>) -> El {
    el().size(200.0, 200.0).child(pad(true))
}

fn without_option(_: &State, _: &mut Cx<State>) -> El {
    el().size(200.0, 200.0).child(pad(false))
}

fn pressed(view: fn(&State, &mut Cx<State>) -> El) -> Runtime<State> {
    let mut rt = Runtime::new(Vec::new(), view, (200.0, 200.0), Env::new(true, 1.0));
    rt.frame(0.0);
    rt.pointer_move(50.0, 50.0);
    rt.pointer_down(50.0, 50.0);
    rt.frame(0.0);
    rt
}

fn ups(rt: &Runtime<State>) -> Vec<bool> {
    rt.state.iter().filter(|d| d.0 == Phase::Up).map(|d| d.1).collect()
}

#[test]
fn with_the_option_a_leave_cancels_at_once_and_the_release_does_nothing() {
    let mut rt = pressed(with_option);
    rt.pointer_leave();
    rt.frame(0.0);
    assert_eq!(ups(&rt), [true], "Up cancelled at the leave");
    assert!(!rt.pressing(), "the capture ended at the leave");
    assert_eq!(rt.pointer(), Some((50.0, 50.0)), "the host still releases at its last position");
    rt.pointer_up(50.0, 50.0);
    rt.frame(0.0);
    assert_eq!(ups(&rt), [true], "the release after it does nothing");
    assert_eq!(rt.pointer(), None, "the deferred leave lands on release");
}

#[test]
fn with_the_option_coming_back_before_the_release_still_does_nothing() {
    let mut rt = pressed(with_option);
    rt.pointer_leave();
    rt.pointer_move(50.0, 50.0);
    rt.pointer_up(50.0, 50.0);
    rt.frame(0.0);
    assert_eq!(ups(&rt), [true]);
    assert_eq!(rt.pointer(), Some((50.0, 50.0)), "the pointer is back inside");
    // The next press is a fresh, uncancelled gesture.
    rt.pointer_down(50.0, 50.0);
    rt.pointer_up(50.0, 50.0);
    rt.frame(0.0);
    assert_eq!(ups(&rt), [true, false]);
}

#[test]
fn without_the_option_a_leave_keeps_the_press_until_the_release() {
    let mut rt = pressed(without_option);
    rt.pointer_leave();
    rt.frame(0.0);
    assert_eq!(ups(&rt), Vec::<bool>::new(), "no Up at the leave");
    assert!(rt.pressing(), "the press still holds the pointer");
    rt.pointer_up(50.0, 50.0);
    rt.frame(0.0);
    assert_eq!(ups(&rt), [false], "the release ends it uncancelled");
    assert_eq!(rt.pointer(), None, "the deferred leave lands on release");
}
