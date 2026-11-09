//! `VirtualKeyboard::record_to(PATH)` is the virtual keyboard of `--emit PATH`: it creates or
//! truncates PATH and writes there the same records it would write to the device. Paths are under
//! cargo's temp dir; nothing opens `/dev`. Record numbers are from
//! `/usr/include/linux/input-event-codes.h`: `EV_SYN` 0, `EV_KEY` 1, `KEY_F5` 63.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::path::{Path, PathBuf};

use tb_core::Key;
use tb_daemon::{KeyEmitter, VirtualKeyboard};

const EV_SYN: u16 = 0;
const EV_KEY: u16 = 1;
const KEY_F5: u16 = 63;

fn fresh_dir(test: &str) -> PathBuf {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join("virtual_keyboard_file").join(test);
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

/// (type, code, value) of each 24-byte record whose time is zero.
fn decode(bytes: &[u8]) -> Vec<(u16, u16, i32)> {
    assert_eq!(bytes.len() % 24, 0, "whole records only, got {} bytes", bytes.len());
    bytes
        .as_chunks::<24>()
        .0
        .iter()
        .map(|r| {
            assert!(r[..16].iter().all(|b| *b == 0), "time is zero");
            (
                u16::from_ne_bytes([r[16], r[17]]),
                u16::from_ne_bytes([r[18], r[19]]),
                i32::from_ne_bytes([r[20], r[21], r[22], r[23]]),
            )
        })
        .collect()
}

#[test]
fn a_recorded_virtual_keyboard_writes_each_press_to_its_file_at_once() {
    let path = fresh_dir("writes").join("keys.bin");
    let mut keyboard = VirtualKeyboard::record_to(&path).unwrap();
    assert_eq!(std::fs::read(&path).unwrap(), Vec::<u8>::new(), "created empty");
    keyboard.emit(&[Key::named("F5").unwrap()]).unwrap();
    assert_eq!(
        decode(&std::fs::read(&path).unwrap()),
        vec![(EV_KEY, KEY_F5, 1), (EV_SYN, 0, 0), (EV_KEY, KEY_F5, 0), (EV_SYN, 0, 0)],
        "flushed before the keyboard is dropped"
    );
}

#[test]
fn a_recorded_virtual_keyboard_truncates_an_existing_file() {
    let path = fresh_dir("truncates").join("keys.bin");
    std::fs::write(&path, b"left over").unwrap();
    let _keyboard = VirtualKeyboard::record_to(&path).unwrap();
    assert_eq!(std::fs::read(&path).unwrap(), Vec::<u8>::new());
}

#[test]
fn a_recorded_virtual_keyboard_in_a_missing_directory_is_an_error_and_makes_nothing() {
    let missing = fresh_dir("missing").join("missing-dir");
    let made = VirtualKeyboard::record_to(&missing.join("keys.bin"));
    assert!(made.is_err(), "no directory is made for the file");
    assert!(!missing.exists());
}
