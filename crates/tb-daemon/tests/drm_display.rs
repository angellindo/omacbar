//! The KMS display adapter (`DrmDisplay`) over a recording KMS boundary, plus the real `CardKms`
//! on an ordinary temp file. No test opens `/dev`, `/sys` or a GPU: `RecordingKms` scripts the
//! metadata the cited `appletbdrm` driver exposes (`.scratch/TB-41/upstream/linux/drivers/gpu/drm/
//! tiny/appletbdrm.c` L554–583, L613–622, L624–720) and records every call in order.
//!
//! Geometry (plan decision 6, driver L409–427 and L681–686): the framebuffer is 60 wide and 2170
//! tall; bar pixel (bx, by) lands at fb column `59 − by`, row `bx`. Byte order per
//! `/usr/include/drm/drm_fourcc.h`: XRGB8888 is little-endian `[31:0] x:R:G:B` (bytes B, G, R, x),
//! BGR888 is `[23:0] B:G:R` (bytes R, G, B). The X byte is not asserted (scanout ignores it).
//! Errno values: `/usr/include/asm-generic/errno-base.h`.
//!
//! Orientation: the driver sets `DRM_MODE_PANEL_ORIENTATION_RIGHT_UP` (appletbdrm.c L702–703),
//! which the kernel names "Right Side Up" in `drm_panel_orientation_enum_list` (Linux 7d0a66e4,
//! `drivers/gpu/drm/drm_connector.c` L1149–1154). Only that spelling is accepted.
//!
//! What a passing test here proves: a SETCRTC or DIRTYFB that returns success means the frame was
//! *submitted*, nothing more. The driver's plane update discards the USB transfer result
//! (appletbdrm.c at stable 497791a0, L474–505), so a lit, legible frame, its latency, and which
//! physical end of the panel is the bar's left edge stay hardware-unverified (TB-10).
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use tb_daemon::DrmDisplay;

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};

use tb_core::{BarSize, Rect};
use tb_daemon::fake::{DUMB_FILL, KmsCall, KmsLog, KmsStep, PriorCrtc, RecordingKms};
use tb_daemon::{CardKms, DirtyClip, Display, DisplayError, DrmError};
use tb_render::Frame;

const BAR: BarSize = BarSize::MACBOOK_PRO;
const W: usize = 2170;
const H: usize = 60;
const FB_W: u32 = 60;
const FB_H: u32 = 2170;

const XRGB8888: u32 = u32::from_le_bytes(*b"XR24");
const BGR888: u32 = u32::from_le_bytes(*b"BG24");
const RGB565: u32 = u32::from_le_bytes(*b"RG16");

const ENOENT: i32 = 2;
const ENOMEM: i32 = 12;
const EBUSY: i32 = 16;
const ENODEV: i32 = 19;
const EINVAL: i32 = 22;
const ENOTTY: i32 = 25;

const GRAY: [u8; 4] = [0x40, 0x40, 0x40, 0xff];
const TOP_LEFT: [u8; 4] = [0xff, 0x01, 0x02, 0xff];
const TOP_RIGHT: [u8; 4] = [0x03, 0xfe, 0x04, 0xff];
const BOTTOM_LEFT: [u8; 4] = [0x05, 0x06, 0xfd, 0xff];
const BOTTOM_RIGHT: [u8; 4] = [0xfc, 0xfb, 0x07, 0xff];

// ---------- helpers ----------

fn solid(rgba: [u8; 4]) -> Frame {
    Frame { width: u32::from(BAR.width), height: u32::from(BAR.height), rgba: rgba.repeat(W * H) }
}

fn put(frame: &mut Frame, bx: usize, by: usize, rgba: [u8; 4]) {
    let i = (by * W + bx) * 4;
    frame.rgba[i..i + 4].copy_from_slice(&rgba);
}

/// Gray with four distinct corners, so a flip on either axis or a swap shows up.
fn corners() -> Frame {
    let mut f = solid(GRAY);
    put(&mut f, 0, 0, TOP_LEFT);
    put(&mut f, W - 1, 0, TOP_RIGHT);
    put(&mut f, 0, H - 1, BOTTOM_LEFT);
    put(&mut f, W - 1, H - 1, BOTTOM_RIGHT);
    f
}

fn whole_bar() -> Rect {
    Rect { x: 0, y: 0, width: BAR.width, height: BAR.height }
}

fn small() -> Rect {
    Rect { x: 100, y: 10, width: 20, height: 5 }
}

/// The bytes of bar pixel (bx, by) in the mapped framebuffer.
fn fb_pixel(px: &[u8], pitch: usize, bpp: usize, bx: usize, by: usize) -> &[u8] {
    let off = bx * pitch + (H - 1 - by) * bpp;
    &px[off..off + bpp]
}

/// What an RGBA bar pixel must become in the framebuffer for `format` (no alpha, no X byte).
fn expected(format: u32, [r, g, b, _]: [u8; 4]) -> Vec<u8> {
    match format {
        XRGB8888 => vec![b, g, r],
        BGR888 => vec![r, g, b],
        other => panic!("no expectation for format {other:#x}"),
    }
}

fn assert_pixel(log: &KmsLog, format: u32, pitch: usize, bx: usize, by: usize, rgba: [u8; 4]) {
    let bpp = if format == XRGB8888 { 4 } else { 3 };
    let px = log.pixels();
    let got = &fb_pixel(&px, pitch, bpp, bx, by)[..3];
    assert_eq!(got, expected(format, rgba).as_slice(), "bar pixel ({bx}, {by}) in {format:#x}");
}

/// The calls that change state somewhere (device, mapping or ownership), by name, in order.
/// Reads (`Version`, `Metadata`, `GetFb`, `SetClientCap`, …) are left out on purpose: the
/// contract pins what is changed and released, not how metadata is gathered.
fn changes(calls: &[KmsCall]) -> Vec<&'static str> {
    calls
        .iter()
        .filter_map(|c| match c {
            KmsCall::SetMaster => Some("set_master"),
            KmsCall::DropMaster => Some("drop_master"),
            KmsCall::CreateDumb { .. } => Some("create_dumb"),
            KmsCall::MapDumb => Some("map_dumb"),
            KmsCall::Unmap => Some("unmap"),
            KmsCall::DestroyDumb => Some("destroy_dumb"),
            KmsCall::AddFb2 { .. } => Some("add_fb2"),
            KmsCall::SetCrtc { .. } => Some("set_crtc"),
            KmsCall::Dirty { .. } => Some("dirty"),
            KmsCall::RmFb { .. } => Some("rm_fb"),
            KmsCall::Close => Some("close"),
            _ => None,
        })
        .collect()
}

fn added_fb(log: &KmsLog) -> (u32, u32, u32) {
    log.calls()
        .iter()
        .find_map(|c| match c {
            KmsCall::AddFb2 { fb, format, pitch, .. } => Some((*fb, *format, *pitch)),
            _ => None,
        })
        .expect("start adds a framebuffer")
}

fn started(kms: RecordingKms) -> (DrmDisplay<RecordingKms>, KmsLog) {
    let log = kms.log();
    let display = DrmDisplay::start(kms, BAR).expect("a cited appletbdrm card starts");
    (display, log)
}

/// Starts, does the first (modeset) present of `corners()`, and returns the call count so far.
fn modeset(kms: RecordingKms) -> (DrmDisplay<RecordingKms>, KmsLog, usize) {
    let (mut d, log) = started(kms);
    d.present(&corners(), &[whole_bar()]).unwrap();
    let n = log.calls().len();
    (d, log, n)
}

fn refused(kms: RecordingKms) -> (DrmError, Vec<&'static str>) {
    let log = kms.log();
    let err = DrmDisplay::start(kms, BAR).err().expect("start must refuse");
    (err, changes(&log.calls()))
}

/// A refusal after master was taken: master released, file closed, nothing else touched.
fn assert_refused_after_master(kms: RecordingKms) -> DrmError {
    let (err, tail) = refused(kms);
    assert_eq!(tail, ["set_master", "drop_master", "close"], "{err:?}");
    assert!(!err.changed(), "{err:?} must report nothing changed");
    err
}

// ---------- startup refusals ----------

#[test]
fn a_card_whose_driver_is_not_appletbdrm_is_closed_without_taking_master() {
    let (err, tail) = refused(RecordingKms::touch_bar().with_driver("i915"));
    assert!(matches!(err, DrmError::WrongDriver { .. }), "{err:?}");
    assert_eq!(tail, ["close"]);
    assert!(!err.changed());
}

#[test]
fn a_card_another_client_holds_as_master_is_busy_and_closed_untouched() {
    let (err, tail) = refused(RecordingKms::touch_bar().fail_at(KmsStep::SetMaster, EBUSY));
    assert!(matches!(err, DrmError::Busy { .. }), "{err:?}");
    assert_eq!(tail, ["set_master", "close"]);
    assert!(!err.changed());
}

#[test]
fn not_exactly_one_connected_usb_connector_is_refused() {
    for n in [0, 2] {
        let err = assert_refused_after_master(RecordingKms::touch_bar().with_connectors(n));
        assert!(matches!(err, DrmError::Unsupported { .. }), "{n} connectors: {err:?}");
    }
}

#[test]
fn a_connector_not_marked_non_desktop_is_refused() {
    for nd in [Some(0), None] {
        let err = assert_refused_after_master(RecordingKms::touch_bar().with_non_desktop(nd));
        assert!(matches!(err, DrmError::Unsupported { .. }), "non_desktop {nd:?}: {err:?}");
    }
}

#[test]
fn a_connector_without_the_portrait_bar_mode_is_refused() {
    for modes in [vec![], vec![(2170, 60)], vec![(60, 2169)], vec![(1920, 1080)]] {
        let err = assert_refused_after_master(RecordingKms::touch_bar().with_modes(&modes));
        assert!(matches!(err, DrmError::Unsupported { .. }), "modes {modes:?}: {err:?}");
    }
}

#[test]
fn a_panel_orientation_other_than_right_side_up_is_refused() {
    for o in [Some("Normal"), Some("Left Side Up"), Some("Upside Down"), None] {
        let err = assert_refused_after_master(RecordingKms::touch_bar().with_orientation(o));
        assert!(matches!(err, DrmError::Unsupported { .. }), "orientation {o:?}: {err:?}");
    }
}

#[test]
fn a_primary_plane_without_xrgb8888_or_bgr888_is_refused() {
    for formats in [vec![], vec![RGB565]] {
        let err = assert_refused_after_master(RecordingKms::touch_bar().with_formats(&formats));
        assert!(matches!(err, DrmError::Unsupported { .. }), "formats {formats:?}: {err:?}");
    }
}

/// Only a proven inactive prior state is accepted. A foreign frame on screen (even one GETFB can
/// read, or one that vanishes before GETFB) is never replaced: no visible change is permitted.
#[test]
fn any_active_or_foreign_prior_presentation_is_refused_before_any_allocation() {
    let fb = PriorCrtc { fb: 77, mode: None };
    let mode = PriorCrtc { fb: 0, mode: Some((60, 2170)) };
    let both = PriorCrtc { fb: 77, mode: Some((60, 2170)) };
    let cases = [
        ("foreign fb, GETFB ok", RecordingKms::touch_bar().with_prior_crtc(fb)),
        ("active mode, no fb", RecordingKms::touch_bar().with_prior_crtc(mode)),
        ("foreign frame scanning out", RecordingKms::touch_bar().with_prior_crtc(both)),
        (
            "foreign frame that disappears",
            RecordingKms::touch_bar().with_prior_crtc(both).fail_at(KmsStep::GetFb, ENOENT),
        ),
        ("plane holds a foreign fb", RecordingKms::touch_bar().with_plane_fb(77)),
        ("connector bound to the crtc", RecordingKms::touch_bar().with_connector_bound(true)),
        ("crtc state unreadable", RecordingKms::touch_bar().fail_at(KmsStep::Metadata, EINVAL)),
    ];
    for (what, kms) in cases {
        let (err, tail) = refused(kms);
        assert_eq!(tail, ["set_master", "drop_master", "close"], "{what}: {err:?}");
        assert!(!err.changed(), "{what}: {err:?}");
    }
}

// ---------- format negotiation, stride and mapping bounds ----------

#[test]
fn xrgb8888_is_preferred_and_bgr888_is_the_fallback() {
    let (_d, log) = started(RecordingKms::touch_bar().with_formats(&[BGR888, XRGB8888]));
    let calls = log.calls();
    assert!(calls.contains(&KmsCall::CreateDumb { width: FB_W, height: FB_H, bpp: 32 }));
    assert_eq!(added_fb(&log).1, XRGB8888);

    let (_d, log) = started(RecordingKms::touch_bar().with_formats(&[BGR888]));
    assert!(log.calls().contains(&KmsCall::CreateDumb { width: FB_W, height: FB_H, bpp: 24 }));
    assert_eq!(added_fb(&log).1, BGR888);
}

#[test]
fn the_framebuffer_is_added_with_the_kernel_pitch_not_a_computed_one() {
    let (_d, log) = started(RecordingKms::touch_bar().with_dumb_pitch(256));
    let added = log.calls().into_iter().find(|c| matches!(c, KmsCall::AddFb2 { .. })).unwrap();
    let KmsCall::AddFb2 { width, height, pitch, .. } = added else { unreachable!() };
    assert_eq!((width, height, pitch), (FB_W, FB_H, 256));
}

/// A pitch below width × bpp, a mapping too short for pitch × height, or sizes whose product
/// overflows are refused before ADDFB2; whatever was allocated is released, nothing is shown.
#[test]
fn an_impossible_pitch_or_mapping_size_is_refused_and_unwound() {
    let cases = [
        ("pitch below 60 × 4", RecordingKms::touch_bar().with_dumb_pitch(239)),
        ("pitch zero", RecordingKms::touch_bar().with_dumb_pitch(0)),
        (
            "pitch × height overflows",
            RecordingKms::touch_bar().with_dumb_pitch(u32::MAX).with_dumb_size(240 * 2170),
        ),
        ("mapping one byte short", RecordingKms::touch_bar().with_dumb_size(240 * 2170 - 1)),
        ("mapping empty", RecordingKms::touch_bar().with_dumb_size(0)),
        ("bgr pitch below 60 × 3", RecordingKms::touch_bar().with_formats(&[BGR888]).with_dumb_pitch(179)),
    ];
    for (what, kms) in cases {
        let (err, tail) = refused(kms);
        assert!(!tail.contains(&"add_fb2"), "{what}: {tail:?}");
        assert!(!tail.contains(&"set_crtc"), "{what}: {tail:?}");
        let count = |n: &str| tail.iter().filter(|t| **t == n).count();
        assert_eq!(count("create_dumb"), count("destroy_dumb"), "{what}: {tail:?}");
        assert_eq!(count("map_dumb"), count("unmap"), "{what}: {tail:?}");
        assert_eq!(&tail[tail.len() - 2..], ["drop_master", "close"], "{what}: {tail:?}");
        assert!(!err.changed(), "{what}: {err:?}");
    }
}

// ---------- startup unwinding ----------

#[test]
fn a_failure_at_each_startup_step_releases_exactly_the_completed_steps_in_reverse() {
    let cases: [(KmsStep, i32, &[&str]); 6] = [
        (KmsStep::Version, ENOTTY, &["close"]),
        (KmsStep::SetMaster, EBUSY, &["set_master", "close"]),
        (KmsStep::Metadata, EINVAL, &["set_master", "drop_master", "close"]),
        (KmsStep::CreateDumb, ENOMEM, &["set_master", "create_dumb", "drop_master", "close"]),
        (
            KmsStep::MapDumb,
            ENOMEM,
            &["set_master", "create_dumb", "map_dumb", "destroy_dumb", "drop_master", "close"],
        ),
        (
            KmsStep::AddFb2,
            EINVAL,
            &[
                "set_master",
                "create_dumb",
                "map_dumb",
                "add_fb2",
                "unmap",
                "destroy_dumb",
                "drop_master",
                "close",
            ],
        ),
    ];
    for (step, errno, want) in cases {
        let (err, tail) = refused(RecordingKms::touch_bar().fail_at(step, errno));
        assert_eq!(tail, want, "failure at {step:?}: {err:?}");
        assert!(!err.changed(), "failure at {step:?}: {err:?}");
    }
}

#[test]
fn starting_validates_and_allocates_but_shows_nothing() {
    let (_d, log) = started(RecordingKms::touch_bar());
    assert_eq!(changes(&log.calls()), ["set_master", "create_dumb", "map_dumb", "add_fb2"]);
}

// ---------- presenting: rotation, format, padding ----------

#[test]
fn four_corner_colours_land_rotated_in_xrgb8888() {
    let (mut d, log) = started(RecordingKms::touch_bar().with_formats(&[XRGB8888]));
    d.present(&corners(), &[whole_bar()]).unwrap();
    let pitch = usize::try_from(added_fb(&log).2).unwrap();
    assert_pixel(&log, XRGB8888, pitch, 0, 0, TOP_LEFT);
    assert_pixel(&log, XRGB8888, pitch, W - 1, 0, TOP_RIGHT);
    assert_pixel(&log, XRGB8888, pitch, 0, H - 1, BOTTOM_LEFT);
    assert_pixel(&log, XRGB8888, pitch, W - 1, H - 1, BOTTOM_RIGHT);
    assert_pixel(&log, XRGB8888, pitch, 1, 1, GRAY);
    // The bar's top-left is fb column 59, row 0 (not column 0): spot-check the raw offset.
    let px = log.pixels();
    assert_eq!(&px[59 * 4..59 * 4 + 3], &[TOP_LEFT[2], TOP_LEFT[1], TOP_LEFT[0]]);
}

#[test]
fn four_corner_colours_land_rotated_in_bgr888() {
    let (mut d, log) = started(RecordingKms::touch_bar().with_formats(&[BGR888]));
    d.present(&corners(), &[whole_bar()]).unwrap();
    let pitch = usize::try_from(added_fb(&log).2).unwrap();
    assert_pixel(&log, BGR888, pitch, 0, 0, TOP_LEFT);
    assert_pixel(&log, BGR888, pitch, W - 1, 0, TOP_RIGHT);
    assert_pixel(&log, BGR888, pitch, 0, H - 1, BOTTOM_LEFT);
    assert_pixel(&log, BGR888, pitch, W - 1, H - 1, BOTTOM_RIGHT);
    let px = log.pixels();
    assert_eq!(&px[59 * 3..59 * 3 + 3], &TOP_LEFT[..3]);
}

#[test]
fn row_padding_beyond_the_visible_width_is_never_written() {
    for (format, bpp, pitch) in [(XRGB8888, 4usize, 256u32), (BGR888, 3, 192)] {
        let kms = RecordingKms::touch_bar().with_formats(&[format]).with_dumb_pitch(pitch);
        let (mut d, log) = started(kms);
        d.present(&corners(), &[whole_bar()]).unwrap();
        d.present(&solid(TOP_LEFT), &[whole_bar()]).unwrap();
        let px = log.pixels();
        let pitch = usize::try_from(pitch).unwrap();
        for row in 0..usize::try_from(FB_H).unwrap() {
            let pad = &px[row * pitch + 60 * bpp..(row + 1) * pitch];
            assert!(pad.iter().all(|b| *b == DUMB_FILL), "format {format:#x} row {row} padding written");
        }
    }
}

// ---------- presenting: phases and damage ----------

#[test]
fn the_first_present_writes_the_whole_frame_then_modesets_the_bar_connector() {
    let (mut d, log) = started(RecordingKms::touch_bar());
    let before = log.calls().len();
    d.present(&corners(), &[small()]).unwrap();
    let new = log.calls()[before..].to_vec();
    assert_eq!(changes(&new), ["set_crtc"]);
    let (fb, format, pitch) = added_fb(&log);
    let set = new.into_iter().find(|c| matches!(c, KmsCall::SetCrtc { .. })).unwrap();
    assert_eq!(
        set,
        KmsCall::SetCrtc {
            crtc: RecordingKms::CRTC,
            fb,
            connectors: vec![RecordingKms::CONNECTOR],
            mode: Some((60, 2170)),
        }
    );
    // Outside the small damage too: a first present shows the whole frame.
    let pitch = usize::try_from(pitch).unwrap();
    assert_pixel(&log, format, pitch, W - 1, H - 1, BOTTOM_RIGHT);
    assert_pixel(&log, format, pitch, 0, 0, TOP_LEFT);
}

#[test]
fn later_presents_write_only_damaged_pixels_and_send_rotated_dirty_clips() {
    let (mut d, log, n) = modeset(RecordingKms::touch_bar());
    let corner = Rect { x: 2169, y: 59, width: 1, height: 1 };
    d.present(&solid(TOP_RIGHT), &[small(), corner]).unwrap();
    let new = log.calls()[n..].to_vec();
    assert_eq!(changes(&new), ["dirty"]);
    let (fb, format, pitch) = added_fb(&log);
    assert_eq!(
        new.into_iter().find(|c| matches!(c, KmsCall::Dirty { .. })).unwrap(),
        KmsCall::Dirty {
            fb,
            clips: vec![
                DirtyClip { x1: 45, y1: 100, x2: 50, y2: 120 },
                DirtyClip { x1: 0, y1: 2169, x2: 1, y2: 2170 },
            ],
        }
    );
    let pitch = usize::try_from(pitch).unwrap();
    assert_pixel(&log, format, pitch, 100, 10, TOP_RIGHT);
    assert_pixel(&log, format, pitch, 119, 14, TOP_RIGHT);
    assert_pixel(&log, format, pitch, W - 1, H - 1, TOP_RIGHT);
    // Outside the damage the previous frame stays.
    assert_pixel(&log, format, pitch, 120, 14, GRAY);
    assert_pixel(&log, format, pitch, 100, 15, GRAY);
    assert_pixel(&log, format, pitch, 0, 0, TOP_LEFT);
}

#[test]
fn more_than_256_damage_rects_collapse_to_one_whole_framebuffer_clip() {
    let rects = |n: u16| (0..n).map(|x| Rect { x, y: 0, width: 1, height: 1 }).collect::<Vec<_>>();
    let clips = |log: &KmsLog, from: usize| {
        log.calls()[from..]
            .iter()
            .find_map(|c| match c {
                KmsCall::Dirty { clips, .. } => Some(clips.clone()),
                _ => None,
            })
            .unwrap()
    };
    let (mut d, log, n) = modeset(RecordingKms::touch_bar());
    d.present(&corners(), &rects(256)).unwrap();
    assert_eq!(clips(&log, n).len(), 256);
    let n = log.calls().len();
    d.present(&corners(), &rects(257)).unwrap();
    assert_eq!(clips(&log, n), vec![DirtyClip { x1: 0, y1: 0, x2: 60, y2: 2170 }]);
}

#[test]
fn empty_damage_makes_no_call_before_or_after_the_modeset() {
    let (mut d, log) = started(RecordingKms::touch_bar());
    let n = log.calls().len();
    d.present(&corners(), &[]).unwrap();
    assert_eq!(log.calls().len(), n, "no modeset for an empty present");
    d.present(&corners(), &[whole_bar()]).unwrap();
    assert_eq!(changes(&log.calls()[n..]), ["set_crtc"], "the first non-empty present modesets");
    let n = log.calls().len();
    d.present(&solid(GRAY), &[]).unwrap();
    assert_eq!(log.calls().len(), n);
}

#[test]
fn a_frame_of_the_wrong_size_or_damage_off_the_bar_writes_nothing_and_calls_nothing() {
    let (mut d, log, n) = modeset(RecordingKms::touch_bar());
    let pixels = log.pixels();

    let tall = Frame { width: 2170, height: 61, rgba: GRAY.repeat(2170 * 61) };
    let err = d.present(&tall, &[whole_bar()]).unwrap_err();
    assert!(matches!(err, DisplayError::FrameSize { .. }), "{err:?}");

    for bad in [
        Rect { x: 2160, y: 0, width: 20, height: 1 },
        Rect { x: 0, y: 59, width: 1, height: 2 },
        Rect { x: u16::MAX, y: u16::MAX, width: u16::MAX, height: u16::MAX },
    ] {
        // A valid rect first: the check must run before any byte is written.
        let err = d.present(&solid(TOP_RIGHT), &[small(), bad]).unwrap_err();
        assert!(matches!(err, DisplayError::DamageOutsideBar { .. }), "{bad:?}: {err:?}");
    }
    assert_eq!(log.calls().len(), n, "no ioctl for a refused present");
    assert!(log.pixels() == pixels, "no pixel written for a refused present");
}

#[test]
fn a_refused_first_present_does_not_modeset() {
    let (mut d, log) = started(RecordingKms::touch_bar());
    let n = log.calls().len();
    let bad = Rect { x: 2169, y: 0, width: 2, height: 1 };
    assert!(d.present(&corners(), &[bad]).is_err());
    assert_eq!(log.calls().len(), n);
}

// ---------- device loss and failures while presenting ----------

#[test]
fn a_device_gone_while_presenting_is_reported_as_gone_and_restore_reports_gone() {
    for step in [KmsStep::SetCrtc, KmsStep::Dirty] {
        let (mut d, log) = started(RecordingKms::touch_bar().fail_at(step, ENODEV));
        let mut err = d.present(&corners(), &[whole_bar()]).err();
        if err.is_none() {
            err = d.present(&solid(GRAY), &[small()]).err();
        }
        let err = err.expect("the failing present reports an error");
        assert!(matches!(err, DisplayError::Device { gone: true, .. }), "{step:?}: {err:?}");
        let n = log.calls().len();
        let restored = d.restore();
        assert!(matches!(restored, Err(DrmError::Gone { .. })), "{step:?}: {restored:?}");
        assert!(
            !changes(&log.calls()[n..]).contains(&"set_crtc"),
            "{step:?}: nothing to restore on a gone device"
        );
    }
}

#[test]
fn an_ordinary_device_error_while_presenting_is_not_gone() {
    let (mut d, _log, _n) = modeset(RecordingKms::touch_bar().fail_at(KmsStep::Dirty, EINVAL));
    let err = d.present(&solid(GRAY), &[small()]).unwrap_err();
    assert!(matches!(err, DisplayError::Device { gone: false, .. }), "{err:?}");
}

#[test]
fn a_failed_modeset_leaves_nothing_to_disable_on_restore() {
    let (mut d, log) = started(RecordingKms::touch_bar().fail_at(KmsStep::SetCrtc, EINVAL));
    assert!(d.present(&corners(), &[whole_bar()]).is_err());
    let n = log.calls().len();
    d.restore().unwrap();
    assert_eq!(changes(&log.calls()[n..]), ["rm_fb", "unmap", "destroy_dumb", "drop_master"]);
}

// ---------- restore ----------

#[test]
fn restore_after_a_modeset_disables_the_crtc_then_releases_in_order() {
    let (mut d, log, n) = modeset(RecordingKms::touch_bar());
    let (fb, _, _) = added_fb(&log);
    d.restore().unwrap();
    let new = log.calls()[n..].to_vec();
    assert_eq!(changes(&new), ["set_crtc", "rm_fb", "unmap", "destroy_dumb", "drop_master"]);
    assert!(new.contains(&KmsCall::SetCrtc {
        crtc: RecordingKms::CRTC,
        fb: 0,
        connectors: vec![],
        mode: None
    }));
    assert!(new.contains(&KmsCall::RmFb { fb }));
    drop(d);
    assert_eq!(changes(&log.calls()[n..]).last(), Some(&"close"));
}

#[test]
fn restore_before_any_present_only_releases() {
    let (mut d, log) = started(RecordingKms::touch_bar());
    let n = log.calls().len();
    d.restore().unwrap();
    assert_eq!(changes(&log.calls()[n..]), ["rm_fb", "unmap", "destroy_dumb", "drop_master"]);
}

#[test]
fn a_second_restore_does_nothing() {
    let (mut d, log, _) = modeset(RecordingKms::touch_bar());
    d.restore().unwrap();
    let n = log.calls().len();
    d.restore().unwrap();
    assert_eq!(log.calls().len(), n);
    drop(d);
    assert_eq!(changes(&log.calls()[n..]), ["close"]);
}

#[test]
fn dropping_without_restore_restores_exactly_once() {
    let (d, log, n) = modeset(RecordingKms::touch_bar());
    drop(d);
    assert_eq!(
        changes(&log.calls()[n..]),
        ["set_crtc", "rm_fb", "unmap", "destroy_dumb", "drop_master", "close"]
    );
}

#[test]
fn a_failing_restore_step_still_attempts_every_later_step_and_reports_it() {
    let (mut d, log, n) = modeset(RecordingKms::touch_bar().fail_at(KmsStep::RmFb, EINVAL));
    let err = d.restore().unwrap_err();
    assert!(err.changed(), "the modeset happened: {err:?}");
    assert_eq!(changes(&log.calls()[n..]), ["set_crtc", "rm_fb", "unmap", "destroy_dumb", "drop_master"]);
    let m = log.calls().len();
    let _ = d.restore();
    assert_eq!(log.calls().len(), m, "restore runs once even after a failure");
}

// ---------- the real adapter, offline ----------

fn fresh_dir(name: &str) -> PathBuf {
    static RUNS: AtomicUsize = AtomicUsize::new(0);
    let n = RUNS.fetch_add(1, Ordering::Relaxed);
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR"))
        .join("drm_display")
        .join(format!("{name}-{}-{n}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

/// The production adapter, offline: it opens only what exists (no `O_CREAT`), and on an ordinary
/// file the first ioctl (`DRM_IOCTL_VERSION`) fails with ENOTTY before anything is taken or written.
#[test]
fn the_real_card_adapter_never_creates_a_node_and_an_ordinary_file_fails_at_the_version_query() {
    let missing = fresh_dir("missing").join("card0");
    assert!(CardKms::open(&missing).is_err());
    assert!(!missing.exists());

    let path = fresh_dir("card").join("card0");
    let content = b"not a drm device, 32 bytes long.";
    std::fs::write(&path, content).unwrap();
    let kms = CardKms::open(&path).unwrap();
    let err = DrmDisplay::start(kms, BAR).err().expect("an ordinary file is not a KMS card");
    assert!(!matches!(err, DrmError::Busy { .. } | DrmError::WrongDriver { .. }), "{err:?}");
    assert!(!err.changed(), "{err:?}");
    assert_eq!(std::fs::read(&path).unwrap(), content);
}
