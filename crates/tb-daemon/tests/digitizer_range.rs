//! Synthetic calibration at the limits of the evdev coordinate type.
#![allow(clippy::unwrap_used)]

use std::io::Cursor;

use tb_daemon::{AbsAxis, DigitizerMeta, DigitizerProfile, EvdevTouches, Touch, TouchSource, calibrate};

#[test]
fn the_full_signed_axis_range_maps_to_bar_columns_without_overflow() {
    let axis = AbsAxis { code: 0x35, min: i32::MIN, max: i32::MAX };
    let profile = DigitizerProfile { name: "Synthetic full-range digitizer", axis, ..Default::default() };
    let meta = DigitizerMeta {
        name: profile.name.to_owned(),
        axes: vec![axis, AbsAxis { code: 0x2f, min: 0, max: 9 }, AbsAxis { code: 0x39, min: 0, max: 65535 }],
        ..Default::default()
    };
    let calibration = calibrate(&meta, &[profile], u16::MAX).unwrap();
    let mut bytes = Vec::new();
    for (kind, code, value) in [
        (3u16, 0x2fu16, 0i32),
        (3, 0x39, 1),
        (3, axis.code, i32::MIN),
        (0, 0, 0),
        (3, axis.code, 0),
        (0, 0, 0),
        (3, axis.code, i32::MAX),
        (0, 0, 0),
        (3, 0x39, -1),
        (0, 0, 0),
    ] {
        bytes.extend([0; 16]);
        bytes.extend(kind.to_ne_bytes());
        bytes.extend(code.to_ne_bytes());
        bytes.extend(value.to_ne_bytes());
    }
    let mut touches = EvdevTouches::new(Cursor::new(bytes), calibration);
    for x in [0, 32767, 65534] {
        assert_eq!(touches.next_touch().unwrap(), Some(Touch::Down { x }));
    }
    assert_eq!(touches.next_touch().unwrap(), Some(Touch::Up));
    assert_eq!(touches.next_touch().unwrap(), None);
}
