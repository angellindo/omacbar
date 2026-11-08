//! The recording fake display: tests and later tickets read back every present, in order.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use tb_core::Rect;
use tb_daemon::Display;
use tb_daemon::fake::RecordingDisplay;
use tb_render::Frame;

fn solid(rgba: [u8; 4]) -> Frame {
    Frame { width: 2170, height: 60, rgba: rgba.repeat(2170 * 60) }
}

/// Presents through the `Display` trait, as the daemon will.
fn present_all(display: &mut impl Display, presents: &[(Frame, Vec<Rect>)]) {
    for (frame, damage) in presents {
        display.present(frame, damage).unwrap();
    }
}

#[test]
fn a_recording_display_keeps_every_frame_and_its_damage_in_order() {
    let whole = Rect { x: 0, y: 0, width: 2170, height: 60 };
    let f5 = Rect { x: 826, y: 8, width: 160, height: 44 };
    let f6 = Rect { x: 994, y: 8, width: 160, height: 44 };
    let sent = vec![
        (solid([1, 2, 3, 255]), vec![whole]),
        (solid([40, 50, 60, 255]), vec![f5]),
        (solid([7, 8, 9, 255]), vec![f5, f6]),
    ];

    let mut display = RecordingDisplay::new();
    assert!(display.presents().is_empty(), "nothing recorded before a present");
    present_all(&mut display, &sent);

    let got: Vec<(Frame, Vec<Rect>)> =
        display.presents().iter().map(|p| (p.frame.clone(), p.damage.clone())).collect();
    assert_eq!(got, sent);
}

#[test]
fn a_recording_display_accepts_any_frame_and_damage() {
    let mut display = RecordingDisplay::new();
    let odd = Frame { width: 3, height: 1, rgba: vec![9; 12] };
    let past = Rect { x: 5000, y: 90, width: 10, height: 10 };
    display.present(&odd, &[past]).unwrap();
    assert_eq!(display.presents().len(), 1);
    assert_eq!(display.presents()[0].frame, odd);
    assert_eq!(display.presents()[0].damage, [past]);
}
