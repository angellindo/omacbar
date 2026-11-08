//! Touch sources: the line protocol (`down <x>` | `up`), the scripted fake, and forwarding
//! touches to the daemon's event channel.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::io::Cursor;
use std::sync::mpsc;

use tb_daemon::fake::ScriptedTouches;
use tb_daemon::{Event, LineTouches, Touch, TouchError, TouchSource, forward_touches};

fn describe(t: &Touch) -> String {
    match t {
        Touch::Down { x } => format!("down {x}"),
        Touch::Up => "up".to_owned(),
    }
}

/// Every touch from `source` until it closes.
fn drain(source: &mut impl TouchSource) -> Vec<String> {
    let mut seen = Vec::new();
    while let Some(t) = source.next_touch().unwrap() {
        seen.push(describe(&t));
    }
    seen
}

#[test]
fn line_touches_read_down_and_up_and_skip_empty_lines() {
    let mut src = LineTouches::new(Cursor::new("down 906\n\nup\n"));
    assert_eq!(drain(&mut src), ["down 906", "up"]);
    assert!(src.next_touch().unwrap().is_none(), "stays closed");
}

#[test]
fn line_touches_read_the_bar_edges() {
    let mut src = LineTouches::new(Cursor::new("down 0\ndown 2169\nup"));
    assert_eq!(drain(&mut src), ["down 0", "down 2169", "up"]);
}

#[test]
fn an_unknown_line_is_a_bad_line_error_naming_it() {
    let mut src = LineTouches::new(Cursor::new("press 3\n"));
    match src.next_touch() {
        Err(TouchError::BadLine { line }) => assert_eq!(line.trim(), "press 3"),
        Err(other) => panic!("expected BadLine, got {other:?}"),
        Ok(t) => panic!("expected BadLine, got {:?}", t.as_ref().map(describe)),
    }
}

#[test]
fn a_down_without_a_valid_x_is_a_bad_line() {
    for bad in ["down\n", "down -4\n", "down abc\n", "down 70000\n"] {
        let mut src = LineTouches::new(Cursor::new(bad));
        match src.next_touch() {
            Err(TouchError::BadLine { line }) => assert_eq!(line.trim(), bad.trim()),
            Err(other) => panic!("{bad:?}: expected BadLine, got {other:?}"),
            Ok(t) => panic!("{bad:?}: expected BadLine, got {:?}", t.as_ref().map(describe)),
        }
    }
}

#[test]
fn scripted_touches_yield_each_touch_then_close() {
    let mut src = ScriptedTouches::new([Touch::Down { x: 12 }, Touch::Up, Touch::Down { x: 40 }]);
    assert_eq!(drain(&mut src), ["down 12", "up", "down 40"]);
}

#[test]
fn forwarded_touches_arrive_as_touch_events_in_order() {
    let mut src = ScriptedTouches::new([Touch::Down { x: 906 }, Touch::Up, Touch::Down { x: 5 }]);
    let (tx, rx) = mpsc::channel();
    forward_touches(&mut src, &tx).unwrap();
    drop(tx);

    let got: Vec<String> = rx
        .iter()
        .map(|e| match e {
            Event::Touch(t) => describe(&t),
            _ => panic!("only touch events are forwarded"),
        })
        .collect();
    assert_eq!(got, ["down 906", "up", "down 5"]);
}

#[test]
fn forwarding_stops_with_the_source_error() {
    let mut src = LineTouches::new(Cursor::new("down 10\npress 3\ndown 20\n"));
    let (tx, rx) = mpsc::channel();
    let err = forward_touches(&mut src, &tx).unwrap_err();
    drop(tx);

    assert!(matches!(err, TouchError::BadLine { .. }), "got {err:?}");
    let got: Vec<String> = rx
        .iter()
        .map(|e| match e {
            Event::Touch(t) => describe(&t),
            _ => panic!("only touch events are forwarded"),
        })
        .collect();
    assert_eq!(got, ["down 10"], "touches before the bad line still arrive");
}

#[test]
fn forwarding_to_a_dropped_receiver_ends_with_ok() {
    let mut src = ScriptedTouches::new([Touch::Down { x: 1 }, Touch::Up]);
    let (tx, rx) = mpsc::channel();
    drop(rx);
    forward_touches(&mut src, &tx).unwrap();
}
