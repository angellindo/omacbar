//! A digitizer's Linux multitouch reports (protocol B: `ABS_MT_SLOT`, `ABS_MT_TRACKING_ID`,
//! `ABS_MT_POSITION_X/Y`, each report closed by `SYN_REPORT`) become touches through
//! `TouchSource`, and only for a digitizer whose identity and axis range match a verified
//! profile exactly. No digitizer is verified yet, so every real one is refused; these tests pass
//! their own synthetic profile. One contact is followed: the first that lands while none is
//! followed. Any report the touch cannot be trusted after (lost sync, a position outside the
//! verified range, a slot that changes contact without a lift, the end of the stream or a read
//! error) cancels it: no action runs and nothing stays pressed. Slot and position are only
//! trusted while the stream is in sync: at the start and after `SYN_DROPPED` nothing is followed
//! until a report names the slot, a new tracking id and a fresh position.
//!
//! Every device here is synthetic: `/proc/bus/input/devices` text, a sysfs tree under cargo's
//! temp dir, a `DigitizerMeta` value and in-memory 24-byte `input_event` records. The one file
//! opened is an ordinary temp file. No test reads the machine's devices, `/proc` or `/sys`.
//! Event codes come from `linux/input-event-codes.h`, ENODEV and ENOTTY from
//! `asm-generic/errno-base.h`, the sysfs `id/*` format (`%04x`) from `drivers/input/input.c`.
//! The synthetic ranges are chosen so that every bar column is a whole number of raw units (3 per
//! column on x, 2 on y), so no case pins a rounding rule. Nothing here is evidence about the T1
//! digitizer (TB-45).
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
// The profile and metadata literals end in `..Default::default()` so that a field the implementer
// adds does not break this file; with exactly the pinned fields that update is needless.
#![allow(clippy::needless_update)]

use tb_daemon::EvdevTouches;

use std::io::{self, Cursor, Read};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, mpsc};

use tb_core::{Action, BarSize, BarTheme, Button, Chord, Group, Key, Layout, SpacingRules, place, presets};
use tb_daemon::fake::{FlatRenderer, RecordingDisplay, RecordingEmitter, RecordingSpawner};
use tb_daemon::{
    AbsAxis, ActionError, Actions, Calibration, Daemon, DigitizerMeta, DigitizerNode, DigitizerProfile,
    Event, KeyEmitter, NodeId, Report, TargetError, Touch, TouchError, TouchSource, VERIFIED_DIGITIZERS,
    calibrate, check_digitizer, confirm_digitizer_node, find_digitizer, forward_touches, open_digitizer,
};

const EV_SYN: u16 = 0;
const EV_ABS: u16 = 3;
const SYN_REPORT: u16 = 0;
const SYN_DROPPED: u16 = 3;
const ABS_MT_SLOT: u16 = 0x2f;
const ABS_MT_POSITION_X: u16 = 0x35;
const ABS_MT_POSITION_Y: u16 = 0x36;
const ABS_MT_TRACKING_ID: u16 = 0x39;
const ENODEV: i32 = 19;
const ENOTTY: i32 = 25;
/// `INPUT_MAJOR` (`include/uapi/linux/major.h`).
const INPUT_MAJOR: u32 = 13;

const WIDTH: u16 = BarSize::MACBOOK_PRO.width;
/// A synthetic name: no real device reports it.
const NAME: &str = "Synthetic multitouch digitizer";
/// Synthetic ranges, not any real Touch Bar's: x has 3 raw units per bar column, y has 2.
const X_MIN: i32 = 37;
const X_MAX: i32 = X_MIN + 3 * 2169;
const Y_MIN: i32 = 11;
const Y_MAX: i32 = Y_MIN + 2 * 2169;

// ---------- synthetic metadata and profiles ----------

fn axis(code: u16, min: i32, max: i32) -> AbsAxis {
    AbsAxis { code, min, max }
}

/// What the adapter would read from the synthetic device: `EVIOCGID`, `EVIOCGNAME` and
/// `EVIOCGABS` of each multitouch axis it reports.
fn meta() -> DigitizerMeta {
    DigitizerMeta {
        bus: 0x0003,
        vendor: 0x05ac,
        product: 0x8600,
        name: NAME.to_owned(),
        axes: vec![
            axis(ABS_MT_SLOT, 0, 9),
            axis(ABS_MT_TRACKING_ID, 0, 65535),
            axis(ABS_MT_POSITION_X, X_MIN, X_MAX),
            axis(ABS_MT_POSITION_Y, Y_MIN, Y_MAX),
        ],
        ..Default::default()
    }
}

/// A profile matching `meta()` exactly, along `along` (x or y).
fn profile_along(along: AbsAxis, reversed: bool) -> DigitizerProfile {
    DigitizerProfile {
        bus: 0x0003,
        vendor: 0x05ac,
        product: 0x8600,
        name: NAME,
        axis: along,
        reversed,
        ..Default::default()
    }
}

fn profile(reversed: bool) -> DigitizerProfile {
    profile_along(axis(ABS_MT_POSITION_X, X_MIN, X_MAX), reversed)
}

fn calibration(profile: DigitizerProfile) -> Calibration {
    match calibrate(&meta(), &[profile], WIDTH) {
        Ok(c) => c,
        Err(e) => panic!("precondition: the synthetic profile matches the synthetic digitizer: {e}"),
    }
}

// ---------- evdev records ----------

#[derive(Debug, Clone, Copy)]
enum Rec {
    Slot(i32),
    Id(i32),
    X(i32),
    Y(i32),
    Report,
    Dropped,
}

/// The raw x of bar column `x`.
fn at(x: u16) -> i32 {
    X_MIN + 3 * i32::from(x)
}

/// Each record as one `x86_64` `struct input_event`: 16 bytes of time, type, code, value.
fn bytes(records: &[Rec]) -> Vec<u8> {
    records
        .iter()
        .flat_map(|r| {
            let (kind, code, value) = match *r {
                Rec::Slot(v) => (EV_ABS, ABS_MT_SLOT, v),
                Rec::Id(v) => (EV_ABS, ABS_MT_TRACKING_ID, v),
                Rec::X(v) => (EV_ABS, ABS_MT_POSITION_X, v),
                Rec::Y(v) => (EV_ABS, ABS_MT_POSITION_Y, v),
                Rec::Report => (EV_SYN, SYN_REPORT, 0),
                Rec::Dropped => (EV_SYN, SYN_DROPPED, 0),
            };
            let mut record = vec![0u8; 16];
            record.extend(kind.to_ne_bytes());
            record.extend(code.to_ne_bytes());
            record.extend(value.to_ne_bytes());
            record
        })
        .collect()
}

/// Contact `id` lands in `slot` at bar column `x`, in one report.
fn land(slot: i32, id: i32, x: u16) -> Vec<Rec> {
    vec![Rec::Slot(slot), Rec::Id(id), Rec::X(at(x)), Rec::Report]
}

/// The current slot's contact moves to bar column `x` (the kernel sends no unchanged value).
fn slide(x: u16) -> Vec<Rec> {
    vec![Rec::X(at(x)), Rec::Report]
}

/// The current slot's contact lifts.
fn lift() -> Vec<Rec> {
    vec![Rec::Id(-1), Rec::Report]
}

/// A byte stream the touch source reads.
type Stream = Box<dyn Read>;

/// Records that end in an unplug: every byte, then ENODEV on each later read.
struct Unplugged(Cursor<Vec<u8>>);

impl Read for Unplugged {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        match self.0.read(buf)? {
            0 => Err(io::Error::from_raw_os_error(ENODEV)),
            n => Ok(n),
        }
    }
}

#[derive(Debug)]
enum End {
    Closed,
    Failed(TouchError),
}

/// Every touch until the source closes or fails.
fn replay_from(reader: impl Read, calibration: Calibration) -> (Vec<Touch>, End) {
    let mut source = EvdevTouches::new(reader, calibration);
    let mut touches = Vec::new();
    for _ in 0..1000 {
        match source.next_touch() {
            Ok(Some(t)) => touches.push(t),
            Ok(None) => return (touches, End::Closed),
            Err(e) => return (touches, End::Failed(e)),
        }
    }
    panic!("the source never ended: {touches:?}");
}

/// The touches of `records` read to their end, which must close the source cleanly.
fn replay(records: &[Rec], calibration: Calibration) -> Vec<Touch> {
    let (touches, end) = replay_from(Cursor::new(bytes(records)), calibration);
    assert!(matches!(end, End::Closed), "{records:?} → {touches:?} then {end:?}");
    touches
}

fn down(x: u16) -> Touch {
    Touch::Down { x }
}

// ---------- calibration: verified identity and ranges only ----------

#[test]
fn no_digitizer_is_verified_yet_so_a_real_one_is_refused_naming_what_was_seen() {
    assert!(VERIFIED_DIGITIZERS.is_empty(), "the T1 entry comes with cited evidence, not before");
    let err = match calibrate(&meta(), VERIFIED_DIGITIZERS, WIDTH) {
        Ok(_) => panic!("an unverified digitizer was calibrated"),
        Err(e) => e.to_string().to_lowercase(),
    };
    for seen in ["05ac", "8600", &NAME.to_lowercase(), &X_MIN.to_string(), &X_MAX.to_string()] {
        assert!(err.contains(seen), "the refusal names the observed `{seen}`: {err}");
    }
}

#[test]
fn an_identity_or_range_unlike_every_verified_profile_is_refused() {
    assert!(calibrate(&meta(), &[profile(false)], WIDTH).is_ok(), "precondition: an exact match calibrates");
    let x = |min, max| {
        vec![axis(ABS_MT_SLOT, 0, 9), axis(ABS_MT_TRACKING_ID, 0, 65535), axis(ABS_MT_POSITION_X, min, max)]
    };
    let unlike: [(&str, DigitizerMeta); 6] = [
        ("another name", DigitizerMeta { name: "Another digitizer".to_owned(), ..meta() }),
        ("the T2 product 8302", DigitizerMeta { product: 0x8302, ..meta() }),
        ("another vendor", DigitizerMeta { vendor: 0x04f3, ..meta() }),
        ("another bus", DigitizerMeta { bus: 0x0018, ..meta() }),
        ("x min one lower", DigitizerMeta { axes: x(X_MIN - 1, X_MAX), ..meta() }),
        ("x max one higher", DigitizerMeta { axes: x(X_MIN, X_MAX + 1), ..meta() }),
    ];
    for (label, seen) in unlike {
        assert!(calibrate(&seen, &[profile(false)], WIDTH).is_err(), "{label}: must be refused");
    }
    // A verified range must span at least one raw unit; an empty or inverted one is refused, not
    // divided by.
    for (min, max) in [(X_MIN, X_MIN), (X_MAX, X_MIN)] {
        let seen = DigitizerMeta { axes: x(min, max), ..meta() };
        let along = profile_along(axis(ABS_MT_POSITION_X, min, max), false);
        assert!(calibrate(&seen, &[along], WIDTH).is_err(), "range {min}..={max}: must be refused");
    }
}

#[test]
fn a_digitizer_missing_a_needed_multitouch_axis_is_refused() {
    for missing in [ABS_MT_SLOT, ABS_MT_TRACKING_ID, ABS_MT_POSITION_X] {
        let mut seen = meta();
        seen.axes.retain(|a| a.code != missing);
        assert!(
            calibrate(&seen, &[profile(false)], WIDTH).is_err(),
            "axis {missing:#x} missing: must be refused"
        );
    }
}

// ---------- replay: press, movement, release ----------

#[test]
fn contact_reports_become_press_movement_and_release() {
    let rows: [(&str, bool, Vec<Rec>, Vec<Touch>); 6] = [
        ("press and release", false, [land(0, 1, 906), lift()].concat(), vec![down(906), Touch::Up]),
        (
            "movement slides; a move within one column or along y only sends nothing",
            false,
            [
                land(0, 1, 906),
                vec![Rec::X(at(906) + 2), Rec::Report, Rec::Y(Y_MIN + 40), Rec::Report],
                slide(1200),
                lift(),
            ]
            .concat(),
            vec![down(906), down(1200), Touch::Up],
        ),
        (
            "the range's ends are the bar's ends",
            false,
            [vec![Rec::Slot(0), Rec::Id(1), Rec::X(X_MIN), Rec::Report, Rec::X(X_MAX), Rec::Report], lift()]
                .concat(),
            vec![down(0), down(2169), Touch::Up],
        ),
        (
            "a reversed axis runs the other way",
            true,
            [vec![Rec::Slot(0), Rec::Id(1), Rec::X(X_MIN), Rec::Report, Rec::X(X_MAX), Rec::Report], lift()]
                .concat(),
            vec![down(2169), down(0), Touch::Up],
        ),
        (
            "a contact whose position comes in a later report lands then",
            false,
            [vec![Rec::Slot(0), Rec::Id(1), Rec::Report], slide(300), lift()].concat(),
            vec![down(300), Touch::Up],
        ),
        (
            "while in sync, a slot reused after a lift keeps its last position until it changes",
            false,
            [
                land(0, 1, 906),
                lift(),
                vec![Rec::Id(2), Rec::Report],
                lift(),
                vec![Rec::Id(3), Rec::X(at(1200)), Rec::Report],
                lift(),
            ]
            .concat(),
            vec![down(906), Touch::Up, down(906), Touch::Up, down(1200), Touch::Up],
        ),
    ];
    for (label, reversed, records, expected) in rows {
        assert_eq!(replay(&records, calibration(profile(reversed))), expected, "{label}");
    }
}

#[test]
fn the_verified_profile_selects_the_axis_that_runs_along_the_bar() {
    let along_y = calibration(profile_along(axis(ABS_MT_POSITION_Y, Y_MIN, Y_MAX), false));
    let y = |x: i32| Rec::Y(Y_MIN + 2 * x);
    let records = [
        vec![Rec::Slot(0), Rec::Id(1), Rec::X(X_MIN + 900), y(500), Rec::Report],
        vec![Rec::X(X_MIN + 1800), Rec::Report],
        vec![y(700), Rec::Report],
        lift(),
    ]
    .concat();
    assert_eq!(replay(&records, along_y), [down(500), down(700), Touch::Up]);
}

#[test]
fn a_second_contact_is_ignored_and_never_promoted() {
    let records = [
        land(0, 1, 906),
        land(1, 8, 1500),
        slide(1600),
        vec![Rec::Slot(0), Rec::Id(-1), Rec::Report],
        vec![Rec::Slot(1), Rec::X(at(1700)), Rec::Report],
        lift(),
    ]
    .concat();
    assert_eq!(replay(&records, calibration(profile(false))), [down(906), Touch::Up]);
}

// ---------- fail closed ----------

#[test]
fn a_touch_that_can_no_longer_be_trusted_is_cancelled_and_never_lifts() {
    let rows: [(&str, Vec<Rec>, Vec<Touch>); 4] = [
        (
            "lost sync: the stale contact's moves and lift are ignored, even in a named slot; a new contact lands",
            [
                land(0, 1, 906),
                vec![Rec::Dropped, Rec::X(at(1100)), Rec::Report],
                slide(1150),
                vec![Rec::Slot(0), Rec::X(at(1200)), Rec::Report],
                lift(),
                land(0, 5, 300),
                lift(),
            ]
            .concat(),
            vec![down(906), Touch::Cancel, down(300), Touch::Up],
        ),
        (
            "a position past the range's max",
            [land(0, 1, 906), vec![Rec::X(X_MAX + 1), Rec::Report], slide(1000), lift()].concat(),
            vec![down(906), Touch::Cancel],
        ),
        (
            "a position below the range's min",
            [land(0, 1, 906), vec![Rec::X(X_MIN - 1), Rec::Report], slide(1000), lift()].concat(),
            vec![down(906), Touch::Cancel],
        ),
        (
            "its slot changes contact without a lift",
            [land(0, 1, 906), vec![Rec::Id(4), Rec::X(at(1200)), Rec::Report], lift()].concat(),
            vec![down(906), Touch::Cancel, down(1200), Touch::Up],
        ),
    ];
    for (label, records, expected) in rows {
        assert_eq!(replay(&records, calibration(profile(false))), expected, "{label}");
    }
}

#[test]
fn slot_and_position_are_trusted_only_while_the_stream_is_in_sync() {
    let rows: [(&str, Vec<Rec>, Vec<Touch>); 4] = [
        (
            "a stream that starts without naming its slot follows nothing until a slot is named",
            [vec![Rec::Id(1), Rec::X(at(906)), Rec::Report], slide(1000), lift(), land(0, 2, 300), lift()]
                .concat(),
            vec![down(300), Touch::Up],
        ),
        (
            "after lost sync the current slot is unknown until a report names it",
            [
                land(0, 1, 906),
                vec![Rec::Dropped, Rec::Report],
                vec![Rec::Id(2), Rec::X(at(500)), Rec::Report],
                lift(),
                land(0, 3, 600),
                lift(),
            ]
            .concat(),
            vec![down(906), Touch::Cancel, down(600), Touch::Up],
        ),
        (
            "after lost sync a position from before the loss is never reused",
            [
                land(0, 1, 906),
                vec![Rec::Dropped, Rec::Report],
                vec![Rec::Slot(0), Rec::Id(2), Rec::Report],
                slide(400),
                lift(),
            ]
            .concat(),
            vec![down(906), Touch::Cancel, down(400), Touch::Up],
        ),
        (
            "lost sync with nothing followed cancels nothing",
            [vec![Rec::Dropped, Rec::Report], land(0, 1, 906), lift()].concat(),
            vec![down(906), Touch::Up],
        ),
    ];
    for (label, records, expected) in rows {
        assert_eq!(replay(&records, calibration(profile(false))), expected, "{label}");
    }
}

#[test]
fn the_end_of_the_stream_or_a_read_error_mid_touch_cancels_before_ending() {
    #[derive(Debug, PartialEq)]
    enum Ends {
        Closed,
        NoDevice,
        AnyError,
    }
    let pressed = bytes(&land(0, 1, 906));
    let truncated = [pressed.clone(), bytes(&[Rec::X(at(1000))])[..10].to_vec()].concat();
    let rows: [(&str, Stream, Vec<Touch>, Ends); 5] = [
        (
            "the stream ends",
            Box::new(Cursor::new(pressed.clone())),
            vec![down(906), Touch::Cancel],
            Ends::Closed,
        ),
        (
            "a lift whose report never ends",
            Box::new(Cursor::new([pressed.clone(), bytes(&[Rec::Id(-1)])].concat())),
            vec![down(906), Touch::Cancel],
            Ends::Closed,
        ),
        (
            "a record cut short",
            Box::new(Cursor::new(truncated)),
            vec![down(906), Touch::Cancel],
            Ends::AnyError,
        ),
        (
            "the device is unplugged",
            Box::new(Unplugged(Cursor::new(pressed))),
            vec![down(906), Touch::Cancel],
            Ends::NoDevice,
        ),
        (
            "the stream ends with no touch down",
            Box::new(Cursor::new(bytes(&[land(0, 1, 906), lift()].concat()))),
            vec![down(906), Touch::Up],
            Ends::Closed,
        ),
    ];
    for (label, reader, expected, ends) in rows {
        let (touches, end) = replay_from(reader, calibration(profile(false)));
        assert_eq!(touches, expected, "{label}");
        let got = match &end {
            End::Closed => Ends::Closed,
            End::Failed(TouchError::Io(e)) if e.raw_os_error() == Some(ENODEV) => Ends::NoDevice,
            End::Failed(_) => Ends::AnyError,
        };
        let ok = got == ends || (ends == Ends::AnyError && got == Ends::NoDevice);
        assert!(ok, "{label}: expected {ends:?}, got {end:?}");
    }
}

// ---------- through the daemon ----------

/// The preset with a command button as a new last group of the default layer.
fn layout() -> Layout {
    let mut layout = presets::default_layout();
    let hello = Button::new("hello", "hi", Action::Command { run: "true".to_owned() });
    layout.layer_mut(Chord::Default).unwrap().groups.push(Group { buttons: vec![hello] });
    layout
}

fn x_of(layout: &Layout, id: &str) -> u16 {
    let layer = layout.layer(Chord::Default).unwrap();
    let placed = place(layer, BarSize::MACBOOK_PRO, &SpacingRules::STANDARD).unwrap();
    let rect = placed.iter().find(|p| p.id.as_str() == id).unwrap().rect;
    rect.x + rect.width / 2
}

/// A labelled stream, the keys it must press, and the commands it must start.
type DaemonRow<'a> = (&'a str, Stream, Vec<Vec<Key>>, Vec<&'a str>);

/// Reads `reader` with the synthetic calibration, forwards its touches, runs them through the
/// real `Daemon`, and returns the keys pressed, the commands started, and whether the last frame
/// shown is the first one (nothing pressed).
fn through_daemon(layout: Layout, reader: Stream) -> (Vec<Vec<Key>>, Vec<String>, bool) {
    let mut source = EvdevTouches::new(reader, calibration(profile(false)));
    let (tx, rx) = mpsc::channel();
    let _ended = forward_touches(&mut source, &tx);
    drop(tx);
    let events: Vec<Event> = rx.iter().collect();

    let emitter = RecordingEmitter::new();
    let spawner = RecordingSpawner::new();
    let report: Report = Arc::new(|e: ActionError| panic!("no action fails here: {e}"));
    let keys = Some(Box::new(emitter.clone()) as Box<dyn KeyEmitter + Send>);
    let actions = Actions::new(keys, Box::new(spawner.clone()), report);
    let mut daemon = Daemon::new(layout, BarTheme::omarchy_fallback(), FlatRenderer, RecordingDisplay::new())
        .with_actions(actions);
    daemon.run(events).unwrap();
    let presents = daemon.display().presents();
    let unpressed = presents.first().unwrap().frame == presents.last().unwrap().frame;
    (emitter.emitted(), spawner.spawned(), unpressed)
}

#[test]
fn through_the_daemon_each_touch_runs_its_action_at_most_once_and_leaves_nothing_pressed() {
    let layout = layout();
    let (f5, f6, hello) = (x_of(&layout, "f5"), x_of(&layout, "f6"), x_of(&layout, "hello"));
    let f5_key = vec![vec![Key::named("F5").unwrap()]];
    let f6_key = vec![vec![Key::named("F6").unwrap()]];
    let none: Vec<Vec<Key>> = Vec::new();
    let replayed = |records: Vec<Rec>| -> Stream { Box::new(Cursor::new(bytes(&records))) };
    let unplugged = |records: Vec<Rec>| -> Stream { Box::new(Unplugged(Cursor::new(bytes(&records)))) };

    let rows: [DaemonRow<'_>; 10] = [
        ("a tap on F5", replayed([land(0, 1, f5), lift()].concat()), f5_key.clone(), vec![]),
        (
            "a tap on the command button",
            replayed([land(0, 1, hello), lift()].concat()),
            none.clone(),
            vec!["true"],
        ),
        (
            "two fingers on F5 are one tap",
            replayed(
                [
                    land(0, 1, f5),
                    land(1, 2, f5),
                    vec![Rec::Slot(0), Rec::Id(-1), Rec::Report],
                    vec![Rec::Slot(1)],
                    lift(),
                ]
                .concat(),
            ),
            f5_key.clone(),
            vec![],
        ),
        (
            "pressed on F5, lifted on F6",
            replayed([land(0, 1, f5), slide(f6), lift()].concat()),
            none.clone(),
            vec![],
        ),
        (
            "lost sync on F5, then the stale lift",
            replayed([land(0, 1, f5), vec![Rec::Dropped, Rec::Report], lift()].concat()),
            none.clone(),
            vec![],
        ),
        (
            "out of range on the command button, then the lift",
            replayed([land(0, 1, hello), vec![Rec::X(X_MAX + 1), Rec::Report], lift()].concat()),
            none.clone(),
            vec![],
        ),
        ("pressed on F5 when the stream ends", replayed(land(0, 1, f5)), none.clone(), vec![]),
        ("pressed on F5 when the device is unplugged", unplugged(land(0, 1, f5)), none.clone(), vec![]),
        (
            "F5's slot changes contact without a lift; the new contact taps F6",
            replayed([land(0, 1, f5), vec![Rec::Id(4), Rec::X(at(f6)), Rec::Report], lift()].concat()),
            f6_key,
            vec![],
        ),
        (
            "a stream joined mid-touch on the command button, slot never named",
            replayed([vec![Rec::Id(1), Rec::X(at(hello)), Rec::Report], lift()].concat()),
            none.clone(),
            vec![],
        ),
    ];
    for (label, reader, keys, commands) in rows {
        let (emitted, spawned, unpressed) = through_daemon(layout.clone(), reader);
        assert_eq!(emitted, keys, "{label}: keys");
        assert_eq!(spawned, commands, "{label}: commands");
        assert!(unpressed, "{label}: something stays pressed");
    }
}

// ---------- discovery over /proc/bus/input/devices text ----------

/// One device block. `abs` is the ABS capability bitmap of a multitouch device; without it the
/// block is a keyboard (no `EV_ABS`).
fn device(id: &str, name: &str, event: &str, abs: Option<&str>) -> String {
    let caps = match abs {
        Some(abs) => format!("B: EV=1b\nB: KEY=400 0 0 0 0 0\nB: ABS={abs}\nB: MSC=20\n"),
        None => "B: EV=120013\nB: KEY=1006300001000 3800000000 10e00000180001f f800000000000002\n".to_owned(),
    };
    format!("I: {id} Version=0110\nN: Name=\"{name}\"\nH: Handlers={event} \nB: PROP=0\n{caps}\n")
}

const T1_ID: &str = "Bus=0003 Vendor=05ac Product=8600";
/// `ABS_MT_SLOT`, `ABS_MT_POSITION_X/Y`, `ABS_MT_TRACKING_ID`, plus `ABS_X/Y`.
const MT_ABS: &str = "260800000000003";

fn t1_keyboard() -> String {
    device(T1_ID, "Synthetic keyboard", "sysrq kbd leds event15", None)
}

fn t1_digitizer(event: &str) -> String {
    device(T1_ID, NAME, event, Some(MT_ABS))
}

/// Same-looking devices that must never be chosen.
fn decoys() -> String {
    [
        t1_keyboard(),
        device("Bus=0003 Vendor=05ac Product=8302", "Synthetic T2 digitizer", "event7", Some(MT_ABS)),
        device(T1_ID, "Synthetic digitizer without slots", "event8", Some("260000000000003")),
        device(T1_ID, "Synthetic digitizer without tracking ids", "event9", Some("60800000000003")),
        device("Bus=0018 Vendor=04f3 Product=8600", "Synthetic touchpad", "mouse0 event10", Some(MT_ABS)),
    ]
    .concat()
}

#[test]
fn the_digitizer_is_found_by_identity_and_multitouch_axes_among_same_looking_devices() {
    let devices = [decoys(), t1_digitizer("event16")].concat();
    assert_eq!(find_digitizer(&devices).unwrap(), Path::new("/dev/input/event16"));
    let first = [t1_digitizer("event3"), decoys()].concat();
    assert_eq!(find_digitizer(&first).unwrap(), Path::new("/dev/input/event3"), "position does not matter");
}

#[test]
fn auto_needs_exactly_one_candidate() {
    assert!(matches!(find_digitizer(&decoys()), Err(TargetError::NotFound)), "only decoys");
    assert!(matches!(find_digitizer(""), Err(TargetError::NotFound)), "no devices");
    let two = [t1_digitizer("event16"), decoys(), t1_digitizer("event17")].concat();
    match find_digitizer(&two) {
        Err(TargetError::Ambiguous(mut seen)) => {
            seen.sort();
            assert_eq!(seen, [PathBuf::from("/dev/input/event16"), PathBuf::from("/dev/input/event17")]);
        }
        other => panic!("two digitizers must be ambiguous, got {other:?}"),
    }
}

#[test]
fn an_explicit_node_must_be_the_event_handler_of_a_candidate() {
    let devices = [decoys(), t1_digitizer("event16")].concat();
    assert_eq!(
        check_digitizer(&devices, Path::new("/dev/input/event16")).unwrap(),
        Path::new("/dev/input/event16")
    );
    for refused in [
        "/dev/input/event15",
        "/dev/input/event7",
        "/dev/input/event99",
        "/dev/input/../input/event16",
        "/tmp/x/event16",
        "event16",
    ] {
        assert!(check_digitizer(&devices, Path::new(refused)).is_err(), "{refused} must be refused");
    }
}

// ---------- the opened node is still the discovered digitizer (fake sysfs) ----------

/// A fake sysfs root under cargo's temp dir, with the kernel's shape for an evdev node:
/// `class/input/eventN` → `devices/…/inputM/eventN` holding `dev` (`major:minor`), and its
/// `device` link → `inputM` holding `id/bustype`, `id/vendor`, `id/product` (`%04x`).
struct Sysfs {
    root: PathBuf,
}

impl Sysfs {
    fn new(name: &str) -> Self {
        static RUNS: AtomicUsize = AtomicUsize::new(0);
        let n = RUNS.fetch_add(1, Ordering::Relaxed);
        let root = Path::new(env!("CARGO_TARGET_TMPDIR"))
            .join("digitizer_touches")
            .join(format!("{name}-{}-{n}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(root.join("class/input")).unwrap();
        Sysfs { root }
    }

    fn write(&self, rel: &str, text: &str) {
        let p = self.root.join(rel);
        std::fs::create_dir_all(p.parent().unwrap()).unwrap();
        std::fs::write(p, text).unwrap();
    }

    /// `eventN` of input device `input` with `dev` = `13:<minor>` and the given ids.
    fn evdev(&self, event: &str, input: &str, minor: u32, (bus, vendor, product): (&str, &str, &str)) {
        let dir = format!("devices/usb1/1-3/1-3:1.2/{input}");
        self.write(&format!("{dir}/id/bustype"), &format!("{bus}\n"));
        self.write(&format!("{dir}/id/vendor"), &format!("{vendor}\n"));
        self.write(&format!("{dir}/id/product"), &format!("{product}\n"));
        self.write(&format!("{dir}/{event}/dev"), &format!("{INPUT_MAJOR}:{minor}\n"));
        std::os::unix::fs::symlink("..", self.root.join(format!("{dir}/{event}/device"))).unwrap();
        std::os::unix::fs::symlink(
            format!("../../{dir}/{event}"),
            self.root.join(format!("class/input/{event}")),
        )
        .unwrap();
    }
}

impl Drop for Sysfs {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.root);
    }
}

const T1_IDS: (&str, &str, &str) = ("0003", "05ac", "8600");

fn evdev_node(minor: u32) -> NodeId {
    NodeId { major: INPUT_MAJOR, minor, char_device: true }
}

#[test]
fn the_opened_node_must_be_the_discovered_digitizer_in_sysfs() {
    let s = Sysfs::new("confirm");
    s.evdev("event16", "input23", 80, T1_IDS);
    s.evdev("event7", "input9", 71, ("0003", "05ac", "8302"));
    s.evdev("event10", "input11", 74, ("0018", "04f3", "8600"));
    let node = Path::new("/dev/input/event16");
    assert_eq!(confirm_digitizer_node(&s.root, node, evdev_node(80)).unwrap(), node);

    let wrong_node: [(&str, NodeId); 4] = [
        ("another minor (renumbered since discovery)", evdev_node(81)),
        ("another major", NodeId { major: 226, minor: 80, char_device: true }),
        ("not a character device", NodeId { major: INPUT_MAJOR, minor: 80, char_device: false }),
        ("nothing at all", NodeId { major: 0, minor: 0, char_device: false }),
    ];
    for (label, opened) in wrong_node {
        assert!(confirm_digitizer_node(&s.root, node, opened).is_err(), "{label}: must be refused");
    }
    let wrong_device: [(&str, &str, NodeId); 4] = [
        ("the T2 digitizer's node", "/dev/input/event7", evdev_node(71)),
        ("another bus and vendor", "/dev/input/event10", evdev_node(74)),
        ("a node sysfs does not list", "/dev/input/event17", evdev_node(81)),
        ("a path outside /dev/input", "/tmp/x/event16", evdev_node(80)),
    ];
    for (label, path, opened) in wrong_device {
        assert!(
            confirm_digitizer_node(&s.root, Path::new(path), opened).is_err(),
            "{label}: must be refused"
        );
    }
}

// ---------- the real adapter, offline ----------

#[test]
fn opening_an_ordinary_file_fails_at_the_identity_read_and_changes_nothing() {
    let s = Sysfs::new("open");
    s.evdev("event16", "input23", 80, T1_IDS);
    let node = s.root.join("event0");
    let content = bytes(&[land(0, 1, 906), lift()].concat());
    std::fs::write(&node, &content).unwrap();

    let opened = DigitizerNode::open(&node).unwrap();
    let id = opened.node_id().unwrap();
    assert!(!id.char_device, "an ordinary file is not a character device: {id:?}");
    match opened.metadata() {
        Ok(seen) => panic!("an ordinary file has no evdev identity, got {seen:?}"),
        Err(e) => assert_eq!(e.raw_os_error(), Some(ENOTTY), "the identity ioctl itself fails: {e}"),
    }
    drop(opened);
    assert!(open_digitizer(&s.root, &node, &[profile(false)]).is_err(), "only a /dev/input node is opened");
    assert_eq!(std::fs::read(&node).unwrap(), content, "the file is unchanged");

    let missing = s.root.join("event1");
    assert!(DigitizerNode::open(&missing).is_err());
    assert!(open_digitizer(&s.root, &missing, VERIFIED_DIGITIZERS).is_err());
    assert!(!missing.exists(), "a missing node is never created");
}
