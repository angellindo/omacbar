#![allow(clippy::unwrap_used)]

use std::fs::File;
use std::io::{self, Write};
use std::os::fd::OwnedFd;
use std::sync::Arc;

use super::{AbsAxis, Calibration, EV_ABS, MT_TRACKING_ID, MT_X, wrap_digitizer};
use crate::evdev::{EV_SYN, SYN_REPORT};
use crate::{StopRead, Touch, TouchSource};

#[test]
fn the_interruptible_digitizer_accepts_an_implicit_slot_and_cancels_on_stop() {
    let (input, mut events) = io::pipe().unwrap();
    let (stop, shutdown) = io::pipe().unwrap();
    let calibration = Calibration {
        axis: AbsAxis { code: MT_X, min: 0, max: 2169 },
        last_column: 2169,
        last_slot: 1,
        reversed: false,
    };
    let mut touches = wrap_digitizer(File::from(OwnedFd::from(input)), calibration, |fd| {
        StopRead(fd, Some(Arc::new(stop)))
    });

    for (kind, code, value) in [
        (EV_ABS, MT_TRACKING_ID, 7_i32),
        (EV_ABS, MT_X, 906),
        (EV_SYN, SYN_REPORT, 0),
        (EV_ABS, MT_TRACKING_ID, -1),
        (EV_SYN, SYN_REPORT, 0),
    ] {
        events.write_all(&[0; 16]).unwrap();
        events.write_all(&kind.to_ne_bytes()).unwrap();
        events.write_all(&code.to_ne_bytes()).unwrap();
        events.write_all(&value.to_ne_bytes()).unwrap();
    }
    drop(events);

    assert_eq!(touches.next_touch().unwrap(), Some(Touch::Down { x: 906 }));
    drop(shutdown);
    assert_eq!(touches.next_touch().unwrap(), Some(Touch::Cancel));
    assert_eq!(touches.next_touch().unwrap(), None);
}
