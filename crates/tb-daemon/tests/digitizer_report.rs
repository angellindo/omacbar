#![allow(clippy::unwrap_used, clippy::panic)]

use std::io::Cursor;
use std::sync::mpsc;

use tb_core::{BarSize, BarTheme, Chord, Key, Layout, SpacingRules, place, presets};
use tb_daemon::fake::{FlatRenderer, Presented, RecordingDisplay, RecordingEmitter, RecordingSpawner};
use tb_daemon::{
    AbsAxis, Actions, Daemon, DigitizerMeta, DigitizerProfile, EvdevTouches, calibrate, forward_touches,
};

const POSITION: AbsAxis = AbsAxis { code: 0x35, min: 0, max: 2169 };
const SLOT: u16 = 0x2f;
const TRACKING_ID: u16 = 0x39;
const REPORT: (u16, u16, i32) = (0, 0, 0);

fn abs(code: u16, value: i32) -> (u16, u16, i32) {
    (3, code, value)
}

#[test]
fn an_invalid_contact_in_the_same_slot_does_not_erase_the_previous_lift() {
    assert_only_f5_taps(0, false);
}

#[test]
fn an_invalid_contact_in_another_slot_does_not_erase_the_previous_lift() {
    assert_only_f5_taps(1, false);
}

#[test]
fn replacing_an_unreported_contact_does_not_erase_the_previous_lift() {
    assert_only_f5_taps(1, true);
}

fn assert_only_f5_taps(new_slot: i32, replace_contact: bool) {
    let layout = presets::default_layout();
    let f5 = position(&layout, "f5");
    let f6 = position(&layout, "f6");
    let mut records = vec![
        abs(SLOT, 0),
        abs(TRACKING_ID, 1),
        f5,
        REPORT,
        abs(TRACKING_ID, -1),
        abs(SLOT, new_slot),
        abs(TRACKING_ID, 2),
        f6,
    ];
    if replace_contact {
        records.push(abs(TRACKING_ID, 3));
    } else {
        records.extend([abs(POSITION.code, POSITION.max + 1), REPORT, f6, REPORT]);
    }
    records.extend([abs(TRACKING_ID, -1), REPORT]);
    let (emitted, presents) = replay(layout, records);
    assert_eq!(emitted, [vec![Key::named("F5").unwrap()]]);
    assert_eq!(presents.len(), 3, "only the initial scene and F5's press and lift are presented");
    assert_eq!(presents[1].damage, presents[2].damage, "only F5 is pressed and cleared");
    assert_ne!(presents[0].frame, presents[1].frame);
    assert_eq!(presents[0].frame, presents[2].frame);
}

fn position(layout: &Layout, id: &str) -> (u16, u16, i32) {
    let placements =
        place(layout.layer(Chord::Default).unwrap(), BarSize::MACBOOK_PRO, &SpacingRules::STANDARD).unwrap();
    let rect = placements.iter().find(|button| button.id.as_str() == id).unwrap().rect;
    abs(POSITION.code, i32::from(rect.x + rect.width / 2))
}

type Record = (u16, u16, i32);

fn replay(layout: Layout, records: impl IntoIterator<Item = Record>) -> (Vec<Vec<Key>>, Vec<Presented>) {
    replay_with(layout, records, 1, EvdevTouches::new)
}

fn replay_with(
    layout: Layout,
    records: impl IntoIterator<Item = Record>,
    last_slot: i32,
    source: impl FnOnce(Cursor<Vec<u8>>, tb_daemon::Calibration) -> EvdevTouches<Cursor<Vec<u8>>>,
) -> (Vec<Vec<Key>>, Vec<Presented>) {
    let bytes: Vec<_> = records
        .into_iter()
        .flat_map(|(kind, code, value)| {
            [0; 16].into_iter().chain(kind.to_ne_bytes()).chain(code.to_ne_bytes()).chain(value.to_ne_bytes())
        })
        .collect();
    let profile = DigitizerProfile {
        bus: 3,
        vendor: 0x05ac,
        product: 0x8600,
        name: "Synthetic MT-B",
        axis: POSITION,
        reversed: false,
    };
    let meta = DigitizerMeta {
        bus: profile.bus,
        vendor: profile.vendor,
        product: profile.product,
        name: profile.name.to_owned(),
        axes: vec![
            AbsAxis { code: SLOT, min: 0, max: last_slot },
            AbsAxis { code: TRACKING_ID, min: 0, max: 255 },
            POSITION,
        ],
    };
    let mut source =
        source(Cursor::new(bytes), calibrate(&meta, &[profile], BarSize::MACBOOK_PRO.width).unwrap());
    let (tx, rx) = mpsc::channel();
    forward_touches(&mut source, &tx).unwrap();
    drop(tx);

    let emitter = RecordingEmitter::new();
    let actions = Actions::new(
        Some(Box::new(emitter.clone())),
        Box::new(RecordingSpawner::new()),
        std::sync::Arc::new(|err| panic!("{err}")),
    );
    let mut daemon = Daemon::new(layout, BarTheme::omarchy_fallback(), FlatRenderer, RecordingDisplay::new())
        .with_actions(actions);
    daemon.run(rx).unwrap();
    (emitter.emitted(), daemon.display().presents().to_vec())
}

fn tid(id: i32) -> Record {
    abs(TRACKING_ID, id)
}

fn invalid_position() -> Record {
    abs(POSITION.code, POSITION.max + 1)
}

fn assert_taps(layout: Layout, records: &[Record], names: &[&str]) {
    let expected: Vec<_> = names.iter().map(|name| vec![Key::named(name).unwrap()]).collect();
    let (emitted, presents) = replay(layout, records.iter().copied());
    assert_eq!(emitted, expected);
    assert_eq!(presents.first().unwrap().frame, presents.last().unwrap().frame);
}

#[test]
fn an_invalid_position_before_the_tracking_id_discards_that_contact() {
    let layout = presets::default_layout();
    let f5 = position(&layout, "f5");
    let records = [abs(SLOT, 0), invalid_position(), tid(1), f5, REPORT, f5, REPORT, tid(-1), REPORT];
    assert_taps(layout, &records, &[]);
}

#[test]
fn a_replacement_without_a_lift_waits_for_position_from_the_new_contact() {
    let layout = presets::default_layout();
    let f5 = position(&layout, "f5");
    let f6 = position(&layout, "f6");
    let records = [
        abs(SLOT, 0),
        tid(1),
        f5,
        REPORT,
        f6,
        tid(2),
        REPORT,
        tid(-1),
        REPORT,
        tid(3),
        f5,
        REPORT,
        tid(-1),
        REPORT,
    ];
    assert_taps(layout, &records, &["F5"]);
}

#[test]
fn a_negative_tracking_id_other_than_lift_cancels_instead_of_leaving_the_touch_armed() {
    let layout = presets::default_layout();
    let f5 = position(&layout, "f5");
    let records = [abs(SLOT, 0), tid(1), f5, REPORT, tid(-2), f5, tid(-1), REPORT];
    assert_taps(layout, &records, &[]);
}

#[test]
fn a_tracking_id_above_the_linux_mt_limit_cannot_start_a_touch() {
    let layout = presets::default_layout();
    let f5 = position(&layout, "f5");
    let records = [
        abs(SLOT, 0),
        tid(65_536),
        f5,
        REPORT,
        tid(-1),
        REPORT,
        abs(SLOT, 0),
        tid(65_535),
        f5,
        REPORT,
        tid(-1),
        REPORT,
    ];
    assert_taps(layout, &records, &["F5"]);
}

#[test]
fn an_out_of_range_slot_cannot_start_or_preserve_a_touch() {
    let layout = presets::default_layout();
    let f5 = position(&layout, "f5");
    let records = [
        abs(SLOT, 2),
        tid(1),
        f5,
        REPORT,
        tid(-1),
        REPORT,
        abs(SLOT, 0),
        tid(2),
        f5,
        REPORT,
        abs(SLOT, 2),
        abs(SLOT, 0),
        tid(-1),
        REPORT,
    ];
    assert_taps(layout, &records, &[]);
}

#[test]
fn a_position_only_changes_the_lift_if_it_precedes_the_end_of_that_contact() {
    let layout = presets::default_layout();
    let f5 = position(&layout, "f5");
    for coordinate in [position(&layout, "f6"), invalid_position()] {
        let land = [abs(SLOT, 0), f5, tid(1), REPORT];
        let before_lift = [land.as_slice(), &[coordinate, tid(-1), REPORT]].concat();
        let after_lift = [land.as_slice(), &[tid(-1), coordinate, REPORT]].concat();
        assert_taps(layout.clone(), &before_lift, &[]);
        assert_taps(layout.clone(), &after_lift, &["F5"]);
    }
}

#[test]
fn a_lift_ends_the_invalid_contact_and_allows_a_distinct_new_landing() {
    let layout = presets::default_layout();
    let f5 = position(&layout, "f5");
    let f6 = position(&layout, "f6");
    let records =
        [abs(SLOT, 0), tid(1), f5, REPORT, invalid_position(), tid(-1), f6, tid(2), REPORT, tid(-1), REPORT];
    assert_taps(layout, &records, &["F6"]);
}

#[test]
fn after_lost_sync_a_slot_and_position_alone_cannot_resume_an_old_contact() {
    let layout = presets::default_layout();
    let f5 = position(&layout, "f5");
    let f6 = position(&layout, "f6");
    let records = [
        abs(SLOT, 0),
        tid(1),
        f5,
        REPORT,
        (0, 3, 0),
        abs(SLOT, 0),
        f5,
        REPORT,
        f5,
        tid(-1),
        REPORT,
        abs(SLOT, 0),
        f5,
        tid(-1),
        REPORT,
        abs(SLOT, 1),
        f6,
        tid(2),
        REPORT,
        tid(-1),
        REPORT,
    ];
    assert_taps(layout, &records, &["F6"]);
}

#[test]
fn position_from_a_report_without_a_contact_does_not_arm_a_later_landing() {
    let layout = presets::default_layout();
    let f5 = position(&layout, "f5");
    let records =
        [abs(SLOT, 0), f5, REPORT, tid(1), REPORT, tid(-1), REPORT, f5, tid(2), REPORT, tid(-1), REPORT];
    assert_taps(layout, &records, &["F5"]);
}

#[test]
fn the_first_new_contact_can_tap_without_a_slot_selector() {
    assert_grabbed_tap(3, |f5, _| vec![tid(20), f5, REPORT, tid(-1), REPORT], Some("F5"), 3);
}

fn assert_grabbed_tap(
    last_slot: i32,
    records: impl FnOnce(Record, Record) -> Vec<Record>,
    tap: Option<&str>,
    present_count: usize,
) {
    let layout = presets::default_layout();
    let records = records(position(&layout, "f5"), position(&layout, "f6"));
    let (emitted, presents) = replay_with(layout, records, last_slot, EvdevTouches::from_grabbed);
    let expected: Vec<_> = tap.into_iter().map(|key| vec![Key::named(key).unwrap()]).collect();
    assert_eq!(emitted, expected);
    assert_eq!(presents.len(), present_count);
    assert_eq!(presents.first().unwrap().frame, presents.last().unwrap().frame);
}

const DROPPED: Record = (0, 3, 0);

#[test]
fn a_contact_already_down_when_grabbed_cannot_tap() {
    assert_grabbed_tap(3, |f5, _| vec![f5, REPORT, tid(-1), REPORT], None, 1);
}

#[test]
fn a_later_selector_does_not_label_earlier_implicit_reports() {
    // The kernel's current selection could already be slot 1 while slot 3's reports are queued.
    // Slot 1's coordinate and lift cannot affect the preceding complete tap.
    assert_grabbed_tap(
        3,
        |f5, f6| vec![tid(20), f5, REPORT, tid(-1), abs(SLOT, 1), f6, tid(-1), REPORT],
        Some("F5"),
        3,
    );
}

#[test]
fn a_position_from_an_unknown_contact_cannot_arm_the_next_id() {
    assert_grabbed_tap(3, |f5, _| vec![f5, tid(20), REPORT, tid(-1), REPORT], None, 1);
}

#[test]
fn a_lift_clears_the_old_position_but_proves_the_slot_is_idle() {
    assert_grabbed_tap(
        3,
        |f5, f6| {
            vec![f5, tid(-1), REPORT, tid(20), REPORT, tid(-1), REPORT, f6, tid(21), REPORT, tid(-1), REPORT]
        },
        Some("F6"),
        3,
    );
}

#[test]
fn a_new_id_without_a_position_does_not_reuse_the_previous_taps_position() {
    assert_grabbed_tap(
        3,
        |f5, _| vec![tid(20), f5, REPORT, tid(-1), REPORT, tid(21), REPORT, tid(-1), REPORT],
        Some("F5"),
        3,
    );
}

#[test]
fn a_startup_queue_reset_never_arms_the_contact_in_its_discarded_report() {
    assert_grabbed_tap(
        3,
        |f5, f6| {
            vec![
                DROPPED,
                abs(SLOT, 1),
                tid(19),
                f5,
                REPORT,
                f5,
                tid(-1),
                REPORT,
                tid(20),
                f6,
                REPORT,
                tid(-1),
                REPORT,
            ]
        },
        Some("F6"),
        3,
    );
}

#[test]
fn overflow_cancels_the_old_touch_and_a_new_implicit_contact_can_tap_after_sync() {
    assert_grabbed_tap(
        3,
        |f5, f6| {
            vec![
                tid(20),
                f5,
                REPORT,
                tid(-1),
                DROPPED,
                abs(SLOT, 2),
                tid(21),
                f5,
                REPORT,
                f5,
                tid(-1),
                REPORT,
                tid(22),
                f6,
                REPORT,
                tid(-1),
                REPORT,
            ]
        },
        Some("F6"),
        5,
    );
}

#[test]
fn both_possible_first_selectors_leave_the_implicit_contact_in_the_other_of_two_slots() {
    for next in [0, 1] {
        assert_grabbed_tap(
            1,
            |f5, f6| {
                vec![
                    tid(20),
                    f5,
                    REPORT,
                    abs(SLOT, next),
                    f6,
                    tid(-1),
                    REPORT,
                    abs(SLOT, 1 - next),
                    tid(-1),
                    REPORT,
                ]
            },
            Some("F5"),
            3,
        );
    }
}

#[test]
fn a_lift_in_an_ambiguous_slot_cannot_finish_the_implicit_contact() {
    assert_grabbed_tap(
        3,
        |f5, f6| vec![tid(20), f5, REPORT, abs(SLOT, 1), f6, REPORT, abs(SLOT, 2), tid(-1), REPORT],
        None,
        3,
    );
}

#[test]
fn an_invalid_selector_requires_an_explicit_valid_selector_before_the_next_tap() {
    assert_grabbed_tap(
        3,
        |f5, f6| {
            vec![
                abs(SLOT, 4),
                tid(20),
                f5,
                REPORT,
                tid(-1),
                REPORT,
                abs(SLOT, 3),
                tid(21),
                f6,
                REPORT,
                tid(-1),
                REPORT,
            ]
        },
        Some("F6"),
        3,
    );
}

#[test]
fn an_implicit_contact_cannot_tap_from_a_partial_report() {
    assert_grabbed_tap(3, |f5, _| vec![tid(20), f5, REPORT, tid(-1)], None, 3);
    assert_grabbed_tap(3, |f5, _| vec![tid(20), f5], None, 1);
}

#[test]
fn the_first_explicit_slot_cannot_replace_the_implicit_contact() {
    assert_grabbed_tap(
        3,
        |f5, f6| vec![tid(20), f5, REPORT, abs(SLOT, 1), tid(21), f6, REPORT, tid(-1), REPORT],
        None,
        3,
    );
}
