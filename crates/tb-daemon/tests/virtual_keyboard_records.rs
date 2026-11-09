//! The virtual keyboard writes a key action as `x86_64` `struct input_event` records (24 bytes:
//! 16 bytes of zero time, `u16` type, `u16` code, `i32` value, native endian): each key pressed in
//! order, then released in reverse, with a `SYN_REPORT` after each, in one write and a flush
//! (ADR-0012). The writer here is memory, never a device. Type and code numbers are from
//! `/usr/include/linux/input-event-codes.h`: `EV_SYN` 0, `EV_KEY` 1, `SYN_REPORT` 0,
//! `KEY_LEFTCTRL` 29, `KEY_F5` 63, `KEY_ESC` 1.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::io::{self, Write};
use std::sync::{Arc, Mutex};

use tb_core::{Action, Button, Chord, Key};
use tb_daemon::fake::RecordingSpawner;
use tb_daemon::{ActionError, Actions, KeyEmitter, Report, VirtualKeyboard};

const EV_SYN: u16 = 0;
const EV_KEY: u16 = 1;
const KEY_ESC: u16 = 1;
const KEY_LEFTCTRL: u16 = 29;
const KEY_F5: u16 = 63;

/// What the virtual keyboard wrote: the bytes, each write call's length, and the flush count.
#[derive(Debug, Default)]
struct Log {
    bytes: Vec<u8>,
    writes: Vec<usize>,
    flushes: usize,
}

/// A writer into a `Log` shared with the test.
#[derive(Debug, Clone, Default)]
struct Written(Arc<Mutex<Log>>);

impl Written {
    fn bytes(&self) -> Vec<u8> {
        self.0.lock().unwrap().bytes.clone()
    }

    fn writes(&self) -> Vec<usize> {
        self.0.lock().unwrap().writes.clone()
    }

    fn flushes(&self) -> usize {
        self.0.lock().unwrap().flushes
    }
}

impl Write for Written {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        let mut log = self.0.lock().unwrap();
        log.bytes.extend_from_slice(buf);
        log.writes.push(buf.len());
        Ok(buf.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        self.0.lock().unwrap().flushes += 1;
        Ok(())
    }
}

/// A writer that always fails, like a virtual keyboard that went away.
struct Broken;

impl Write for Broken {
    fn write(&mut self, _: &[u8]) -> io::Result<usize> {
        Err(io::Error::other("the device is gone"))
    }

    fn flush(&mut self) -> io::Result<()> {
        Err(io::Error::other("the device is gone"))
    }
}

/// (time is zero, type, code, value) of each 24-byte record.
fn decode(bytes: &[u8]) -> Vec<(bool, u16, u16, i32)> {
    assert_eq!(bytes.len() % 24, 0, "whole records only, got {} bytes", bytes.len());
    bytes
        .as_chunks::<24>()
        .0
        .iter()
        .map(|r| {
            (
                r[..16].iter().all(|b| *b == 0),
                u16::from_ne_bytes([r[16], r[17]]),
                u16::from_ne_bytes([r[18], r[19]]),
                i32::from_ne_bytes([r[20], r[21], r[22], r[23]]),
            )
        })
        .collect()
}

fn key(name: &str) -> Key {
    Key::named(name).unwrap()
}

fn press(code: u16) -> [(bool, u16, u16, i32); 2] {
    [(true, EV_KEY, code, 1), (true, EV_SYN, 0, 0)]
}

fn release(code: u16) -> [(bool, u16, u16, i32); 2] {
    [(true, EV_KEY, code, 0), (true, EV_SYN, 0, 0)]
}

#[test]
fn left_ctrl_and_f5_are_pressed_in_order_and_released_in_reverse() {
    let written = Written::default();
    let mut keyboard = VirtualKeyboard::new(written.clone());
    keyboard.emit(&[key("LeftCtrl"), key("F5")]).unwrap();
    let expected = [press(KEY_LEFTCTRL), press(KEY_F5), release(KEY_F5), release(KEY_LEFTCTRL)].concat();
    assert_eq!(decode(&written.bytes()), expected);
}

#[test]
fn one_key_is_one_press_and_one_release() {
    let written = Written::default();
    let mut keyboard = VirtualKeyboard::new(written.clone());
    keyboard.emit(&[key("Esc")]).unwrap();
    assert_eq!(decode(&written.bytes()), [press(KEY_ESC), release(KEY_ESC)].concat());
}

#[test]
fn each_emit_is_one_write_and_a_flush() {
    let written = Written::default();
    let mut keyboard = VirtualKeyboard::new(written.clone());
    keyboard.emit(&[key("LeftCtrl"), key("F5")]).unwrap();
    assert_eq!(written.writes(), vec![8 * 24], "all eight records in one write");
    assert!(written.flushes() >= 1, "flushed");
    keyboard.emit(&[key("F5")]).unwrap();
    assert_eq!(written.writes(), vec![8 * 24, 4 * 24]);
    let all = decode(&written.bytes());
    assert_eq!(all[8..], [press(KEY_F5), release(KEY_F5)].concat(), "the second emit follows the first");
}

#[test]
fn a_failed_write_is_reported_as_an_emit_failure_naming_the_button() {
    let reports = Arc::new(Mutex::new(Vec::new()));
    let sink = Arc::clone(&reports);
    let report: Report = Arc::new(move |e: ActionError| sink.lock().unwrap().push(e));
    let keyboard: Box<dyn KeyEmitter + Send> = Box::new(VirtualKeyboard::new(Broken));
    let mut actions = Actions::new(Some(keyboard), Box::new(RecordingSpawner::new()), report);
    let f5 = Button::new("f5", "F5", Action::Key { keys: vec!["F5".to_owned()] });
    actions.run(&f5, Chord::Default);
    let reports = reports.lock().unwrap();
    assert_eq!(reports.len(), 1, "{reports:?}");
    assert!(
        matches!(&reports[0], ActionError::Emit { chord: Chord::Default, id, .. } if id.as_str() == "f5"),
        "{reports:?}"
    );
    assert!(reports[0].to_string().contains("f5"), "{}", reports[0]);
}
