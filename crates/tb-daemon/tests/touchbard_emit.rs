//! Functional: `touchbard --emit PATH` presses the keys of tapped buttons on a virtual keyboard
//! that writes its records to PATH, the simulator twin of `--emit uinput`. A virtual keyboard that
//! cannot be made, or no `--emit`, is logged and key actions do nothing, while the bar keeps
//! running. PATH is always under cargo's temp dir: no test passes `--emit uinput` or a `/dev`
//! path, and no test taps a command button. Each run gets an empty `HOME` (bundled Tokyo Night).
//! Needs a Vulkan adapter (lavapipe is fine). Record numbers are from
//! `/usr/include/linux/input-event-codes.h`: `EV_SYN` 0, `EV_KEY` 1, `KEY_ESC` 1, `KEY_F5` 63.
//!
//! The binary path is read with `option_env!`, so this file compiles without the binary target.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};
use std::sync::atomic::{AtomicUsize, Ordering};

use tb_core::{BarSize, Chord, SpacingRules, place, presets};

const EV_SYN: u16 = 0;
const EV_KEY: u16 = 1;
const KEY_ESC: u16 = 1;
const KEY_F5: u16 = 63;

fn touchbard() -> Command {
    match option_env!("CARGO_BIN_EXE_touchbard") {
        Some(bin) => Command::new(bin),
        None => panic!("the tb-daemon package has no `touchbard` binary target"),
    }
}

/// A new empty directory for one use, under cargo's temp dir.
fn fresh_dir(name: &str) -> PathBuf {
    static RUNS: AtomicUsize = AtomicUsize::new(0);
    let n = RUNS.fetch_add(1, Ordering::Relaxed);
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR"))
        .join("touchbard_emit")
        .join(format!("{name}-{}-{n}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn run(args: &[&str], stdin: &str) -> Output {
    let home = fresh_dir("home");
    let mut child = touchbard()
        .args(args)
        .env("HOME", &home)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    child.stdin.take().unwrap().write_all(stdin.as_bytes()).unwrap();
    child.wait_with_output().unwrap()
}

fn stderr(out: &Output) -> String {
    String::from_utf8_lossy(&out.stderr).into_owned()
}

/// The centre x of default-layer button `id` of the preset.
fn x_of(id: &str) -> u16 {
    let layout = presets::default_layout();
    let placed =
        place(layout.layer(Chord::Default).unwrap(), BarSize::MACBOOK_PRO, &SpacingRules::STANDARD).unwrap();
    let rect = placed.iter().find(|p| p.id.as_str() == id).unwrap().rect;
    rect.x + rect.width / 2
}

fn tap(id: &str) -> String {
    format!("down {}\nup\n", x_of(id))
}

fn frame_files(dir: &Path) -> Vec<String> {
    let mut names: Vec<String> = std::fs::read_dir(dir)
        .unwrap()
        .map(|e| e.unwrap().file_name().into_string().unwrap())
        .filter(|n| n.starts_with("frame-") && Path::new(n).extension().is_some_and(|e| e == "png"))
        .collect();
    names.sort();
    names
}

/// (time is zero, type, code, value) of each 24-byte `x86_64` `input_event` record.
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

fn pressed_and_released(code: u16) -> Vec<(bool, u16, u16, i32)> {
    vec![(true, EV_KEY, code, 1), (true, EV_SYN, 0, 0), (true, EV_KEY, code, 0), (true, EV_SYN, 0, 0)]
}

const THREE_FRAMES: [&str; 3] = ["frame-0001.png", "frame-0002.png", "frame-0003.png"];

#[test]
fn a_tap_on_f5_writes_f5_pressed_and_released_to_the_emit_file() {
    let (sim, keys) = (fresh_dir("f5-sim"), fresh_dir("f5-keys").join("keys.bin"));
    let out = run(&["--sim", sim.to_str().unwrap(), "--emit", keys.to_str().unwrap()], &tap("f5"));
    assert!(out.status.success(), "exit {:?}, stderr: {}", out.status, stderr(&out));
    assert_eq!(decode(&std::fs::read(&keys).unwrap()), pressed_and_released(KEY_F5));
    assert_eq!(frame_files(&sim), THREE_FRAMES, "start, press, release as before");
}

#[test]
fn two_taps_write_their_keys_in_tap_order() {
    let (sim, keys) = (fresh_dir("two-sim"), fresh_dir("two-keys").join("keys.bin"));
    let stdin = format!("{}{}", tap("f5"), tap("esc"));
    let out = run(&["--emit", keys.to_str().unwrap(), "--sim", sim.to_str().unwrap()], &stdin);
    assert!(out.status.success(), "exit {:?}, stderr: {}", out.status, stderr(&out));
    let expected = [pressed_and_released(KEY_F5), pressed_and_released(KEY_ESC)].concat();
    assert_eq!(decode(&std::fs::read(&keys).unwrap()), expected);
}

#[test]
fn the_emit_file_starts_empty_and_a_touch_that_is_no_tap_writes_nothing() {
    let (sim, keys) = (fresh_dir("empty-sim"), fresh_dir("empty-keys").join("keys.bin"));
    std::fs::write(&keys, b"left over from an earlier run").unwrap();
    let stdin = format!("down {}\ndown {}\nup\n", x_of("f5"), x_of("f6"));
    let out = run(&["--sim", sim.to_str().unwrap(), "--emit", keys.to_str().unwrap()], &stdin);
    assert!(out.status.success(), "exit {:?}, stderr: {}", out.status, stderr(&out));
    assert_eq!(std::fs::read(&keys).unwrap(), Vec::<u8>::new());
}

#[test]
fn an_emit_file_that_cannot_be_made_is_logged_and_the_bar_still_runs() {
    let sim = fresh_dir("missing-sim");
    let missing = fresh_dir("missing-keys").join("missing-dir");
    let keys = missing.join("keys.bin");
    let out = run(&["--emit", keys.to_str().unwrap(), "--sim", sim.to_str().unwrap()], &tap("f5"));
    assert!(out.status.success(), "exit {:?}, stderr: {}", out.status, stderr(&out));
    let err = stderr(&out);
    assert!(err.contains(keys.to_str().unwrap()), "stderr names the path: {err}");
    assert!(err.contains("key actions do nothing"), "stderr says what is lost: {err}");
    assert!(!missing.exists(), "no directory is made for it");
    assert_eq!(frame_files(&sim), THREE_FRAMES, "the bar still presents the tap");
}

#[test]
fn without_emit_a_tapped_key_is_logged_and_the_bar_still_runs() {
    let sim = fresh_dir("no-emit-sim");
    let out = run(&["--sim", sim.to_str().unwrap()], &tap("f5"));
    assert!(out.status.success(), "exit {:?}, stderr: {}", out.status, stderr(&out));
    let err = stderr(&out);
    assert!(
        err.lines().any(|l| l.starts_with("touchbard: ")
            && l.contains("f5")
            && l.to_lowercase().contains("keyboard")),
        "one line names f5 and the missing keyboard: {err}"
    );
    assert_eq!(frame_files(&sim), THREE_FRAMES);
}

#[test]
fn emit_without_a_value_is_a_usage_error_that_names_emit() {
    let sim = fresh_dir("usage-sim");
    let out = run(&["--sim", sim.to_str().unwrap(), "--emit"], "");
    assert_eq!(out.status.code(), Some(2), "stderr: {}", stderr(&out));
    let err = stderr(&out);
    assert!(err.to_lowercase().contains("usage"), "{err}");
    assert!(err.contains("--emit"), "the usage names --emit: {err}");
}

#[test]
fn emit_given_twice_is_a_usage_error() {
    let dir = fresh_dir("twice");
    let (a, b) = (dir.join("a.bin"), dir.join("b.bin"));
    let out = run(&["--emit", a.to_str().unwrap(), "--emit", b.to_str().unwrap()], "");
    assert_eq!(out.status.code(), Some(2), "stderr: {}", stderr(&out));
    assert!(stderr(&out).contains("--emit"), "the usage names --emit: {}", stderr(&out));
    assert!(!a.exists() && !b.exists(), "nothing is written on a usage error");
}
