//! Held modifiers from the Fn keyboard's key event stream. `EvdevModifiers` reads raw `x86_64`
//! `struct input_event` records (24 bytes, native endian: 16 bytes of time, `u16` type, `u16`
//! code, `i32` value) from any byte stream; these tests build the records in memory, so no input
//! device is ever opened. A change of the held modifiers is reported at `SYN_REPORT` only.
//! Key codes come from `/usr/include/linux/input-event-codes.h`.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::io::{Cursor, Read};

use tb_core::Modifiers;
use tb_daemon::fake::ScriptedModifiers;
use tb_daemon::{EvdevModifiers, KeyError, ModifierSource};

const EV_SYN: u16 = 0;
const EV_KEY: u16 = 1;
const EV_MSC: u16 = 4;
const SYN_REPORT: u16 = 0;
const MSC_SCAN: u16 = 4;

const KEY_FN: u16 = 464;
const KEY_LEFTCTRL: u16 = 29;
const KEY_RIGHTCTRL: u16 = 97;
const KEY_LEFTALT: u16 = 56;
const KEY_RIGHTALT: u16 = 100;
const KEY_LEFTSHIFT: u16 = 42;
const KEY_RIGHTSHIFT: u16 = 54;
const KEY_LEFTMETA: u16 = 125;
const KEY_RIGHTMETA: u16 = 126;
const KEY_A: u16 = 30;
const KEY_F5: u16 = 63;

/// One `struct input_event` as the kernel writes it on `x86_64`. The time is not zero so a reader
/// that mistakes it for the type, code or value is caught.
fn record(kind: u16, code: u16, value: i32) -> Vec<u8> {
    let mut r = Vec::with_capacity(24);
    r.extend_from_slice(&1_762_675_200_i64.to_ne_bytes());
    r.extend_from_slice(&123_456_i64.to_ne_bytes());
    r.extend_from_slice(&kind.to_ne_bytes());
    r.extend_from_slice(&code.to_ne_bytes());
    r.extend_from_slice(&value.to_ne_bytes());
    assert_eq!(r.len(), 24);
    r
}

/// A key going down, preceded by its scan code as the keyboard reports it.
fn press(code: u16) -> Vec<u8> {
    [record(EV_MSC, MSC_SCAN, 0x7_00e0), record(EV_KEY, code, 1)].concat()
}

fn release(code: u16) -> Vec<u8> {
    [record(EV_MSC, MSC_SCAN, 0x7_00e0), record(EV_KEY, code, 0)].concat()
}

/// The key's autorepeat while it stays down.
fn repeat(code: u16) -> Vec<u8> {
    record(EV_KEY, code, 2)
}

fn syn() -> Vec<u8> {
    record(EV_SYN, SYN_REPORT, 0)
}

/// The held modifiers named in `keys` (`fn`, `ctrl`, `alt`, `shift`, `super`).
fn held(keys: &[&str]) -> Modifiers {
    let mut m = Modifiers::default();
    for key in keys {
        match *key {
            "fn" => m.fn_key = true,
            "ctrl" => m.ctrl = true,
            "alt" => m.alt = true,
            "shift" => m.shift = true,
            "super" => m.super_key = true,
            other => panic!("no modifier {other}"),
        }
    }
    m
}

fn keyboard(stream: &[Vec<u8>]) -> EvdevModifiers<Cursor<Vec<u8>>> {
    EvdevModifiers::new(Cursor::new(stream.concat()))
}

/// Every change the source reports until it closes.
fn drain(source: &mut impl ModifierSource) -> Vec<Modifiers> {
    let mut seen = Vec::new();
    while let Some(m) = source.next_modifiers().unwrap() {
        seen.push(m);
        assert!(seen.len() <= 64, "the source never closes: {seen:?}");
    }
    seen
}

#[test]
fn a_fn_press_reported_by_a_sync_holds_fn() {
    let mut kb = keyboard(&[press(KEY_FN), syn()]);
    assert_eq!(drain(&mut kb), [held(&["fn"])]);
}

#[test]
fn fn_and_ctrl_pressed_in_one_report_are_one_change() {
    let mut kb = keyboard(&[press(KEY_FN), press(KEY_LEFTCTRL), syn()]);
    assert_eq!(drain(&mut kb), [held(&["fn", "ctrl"])], "no Fn layer on the way to Fn + Ctrl");
}

#[test]
fn ctrl_then_fn_in_separate_reports_are_two_changes() {
    let mut kb = keyboard(&[press(KEY_LEFTCTRL), syn(), press(KEY_FN), syn()]);
    assert_eq!(drain(&mut kb), [held(&["ctrl"]), held(&["fn", "ctrl"])]);
}

#[test]
fn releasing_fn_while_ctrl_is_held_leaves_ctrl() {
    let mut kb = keyboard(&[
        press(KEY_FN),
        syn(),
        press(KEY_LEFTCTRL),
        syn(),
        release(KEY_FN),
        syn(),
        release(KEY_LEFTCTRL),
        syn(),
    ]);
    assert_eq!(drain(&mut kb), [held(&["fn"]), held(&["fn", "ctrl"]), held(&["ctrl"]), Modifiers::default()]);
}

#[test]
fn repeats_of_a_held_fn_report_nothing() {
    let mut kb = keyboard(&[
        press(KEY_FN),
        syn(),
        repeat(KEY_FN),
        syn(),
        repeat(KEY_FN),
        syn(),
        repeat(KEY_FN),
        syn(),
        release(KEY_FN),
        syn(),
    ]);
    assert_eq!(drain(&mut kb), [held(&["fn"]), Modifiers::default()]);
}

#[test]
fn a_repeat_of_a_key_not_seen_pressed_marks_it_held() {
    // Fn was already down when the reader started: its first autorepeat is the first sign of it.
    let mut kb = keyboard(&[repeat(KEY_FN), syn(), repeat(KEY_FN), syn(), release(KEY_FN), syn()]);
    assert_eq!(drain(&mut kb), [held(&["fn"]), Modifiers::default()]);
}

#[test]
fn a_flag_stays_held_while_either_key_of_its_pair_is_down() {
    let mut kb = keyboard(&[
        press(KEY_LEFTSHIFT),
        syn(),
        press(KEY_RIGHTSHIFT),
        syn(),
        release(KEY_LEFTSHIFT),
        syn(),
        release(KEY_RIGHTSHIFT),
        syn(),
    ]);
    assert_eq!(drain(&mut kb), [held(&["shift"]), Modifiers::default()]);
}

#[test]
fn each_modifier_key_holds_its_flag() {
    let keys = [
        (KEY_FN, "fn"),
        (KEY_LEFTCTRL, "ctrl"),
        (KEY_RIGHTCTRL, "ctrl"),
        (KEY_LEFTALT, "alt"),
        (KEY_RIGHTALT, "alt"),
        (KEY_LEFTSHIFT, "shift"),
        (KEY_RIGHTSHIFT, "shift"),
        (KEY_LEFTMETA, "super"),
        (KEY_RIGHTMETA, "super"),
    ];
    for (code, flag) in keys {
        let mut kb = keyboard(&[press(code), syn(), release(code), syn()]);
        assert_eq!(drain(&mut kb), [held(&[flag]), Modifiers::default()], "key code {code}");
    }
}

#[test]
fn other_keys_and_scan_codes_report_nothing() {
    let mut kb = keyboard(&[
        record(EV_MSC, MSC_SCAN, 0x7_003e),
        syn(),
        press(KEY_F5),
        syn(),
        press(KEY_A),
        syn(),
        repeat(KEY_A),
        syn(),
        release(KEY_A),
        release(KEY_F5),
        syn(),
    ]);
    assert_eq!(drain(&mut kb), Vec::<Modifiers>::new());
}

#[test]
fn releasing_a_key_never_pressed_reports_nothing() {
    let mut kb = keyboard(&[release(KEY_FN), syn(), release(KEY_RIGHTCTRL), syn()]);
    assert_eq!(drain(&mut kb), Vec::<Modifiers>::new());
}

#[test]
fn a_press_and_release_in_one_report_is_no_change() {
    let mut kb = keyboard(&[press(KEY_FN), release(KEY_FN), syn()]);
    assert_eq!(drain(&mut kb), Vec::<Modifiers>::new());
}

#[test]
fn a_key_value_other_than_up_down_or_repeat_is_ignored() {
    let mut kb = keyboard(&[record(EV_KEY, KEY_FN, 3), syn(), record(EV_KEY, KEY_FN, -1), syn()]);
    assert_eq!(drain(&mut kb), Vec::<Modifiers>::new());
}

#[test]
fn key_changes_without_a_trailing_sync_are_not_reported() {
    let mut kb = keyboard(&[press(KEY_FN), syn(), press(KEY_LEFTCTRL)]);
    assert_eq!(drain(&mut kb), [held(&["fn"])]);
}

#[test]
fn the_end_of_the_stream_on_a_record_boundary_closes_the_source_and_it_stays_closed() {
    let mut kb = keyboard(&[press(KEY_FN), syn()]);
    assert_eq!(kb.next_modifiers().unwrap(), Some(held(&["fn"])));
    assert_eq!(kb.next_modifiers().unwrap(), None);
    assert_eq!(kb.next_modifiers().unwrap(), None, "stays closed");
}

#[test]
fn the_end_of_the_stream_inside_a_record_is_an_io_error() {
    let mut bytes = [press(KEY_FN), syn()].concat();
    bytes.extend_from_slice(&record(EV_KEY, KEY_LEFTCTRL, 1)[..10]);
    let mut kb = EvdevModifiers::new(Cursor::new(bytes));
    assert_eq!(kb.next_modifiers().unwrap(), Some(held(&["fn"])), "whole records before it still count");
    match kb.next_modifiers() {
        Err(KeyError::Io { .. }) => {}
        other => panic!("a record cut short is KeyError::Io, got {other:?}"),
    }
}

/// A byte stream that reports its end once, then has more records, like a file appended to
/// after it was read to the end.
struct EndsThenGrows {
    first: Cursor<Vec<u8>>,
    ended: bool,
    later: Cursor<Vec<u8>>,
}

impl Read for EndsThenGrows {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        if !self.ended {
            let n = self.first.read(buf)?;
            if n > 0 {
                return Ok(n);
            }
            self.ended = true;
            return Ok(0);
        }
        self.later.read(buf)
    }
}

#[test]
fn a_closed_source_never_reports_records_that_arrive_after_its_end() {
    let mut kb = EvdevModifiers::new(EndsThenGrows {
        first: Cursor::new([press(KEY_FN), syn()].concat()),
        ended: false,
        later: Cursor::new([press(KEY_LEFTCTRL), syn(), release(KEY_FN), syn()].concat()),
    });
    assert_eq!(kb.next_modifiers().unwrap(), Some(held(&["fn"])));
    assert_eq!(kb.next_modifiers().unwrap(), None, "the end of the stream closes the source");
    for call in 0..3 {
        assert_eq!(kb.next_modifiers().unwrap(), None, "call {call} after the end: stays closed");
    }
}

/// A byte stream that hands out at most 5 bytes per read, as a pipe may.
struct Trickle(Cursor<Vec<u8>>);

impl Read for Trickle {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        let n = buf.len().min(5);
        self.0.read(&mut buf[..n])
    }
}

#[test]
fn records_split_across_reads_are_read_whole() {
    let stream = [press(KEY_FN), syn(), press(KEY_RIGHTALT), syn(), release(KEY_FN), syn()].concat();
    let mut kb = EvdevModifiers::new(Trickle(Cursor::new(stream)));
    assert_eq!(drain(&mut kb), [held(&["fn"]), held(&["fn", "alt"]), held(&["alt"])]);
}

#[test]
fn scripted_modifiers_yield_each_change_then_close() {
    let script = [held(&["fn"]), held(&["fn", "ctrl"]), Modifiers::default()];
    let mut src = ScriptedModifiers::new(script);
    assert_eq!(drain(&mut src), script);
    assert_eq!(src.next_modifiers().unwrap(), None, "stays closed");
}
