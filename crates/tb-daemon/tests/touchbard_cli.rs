//! Functional: the `touchbard` binary with the simulator backend. Touches come as stdin lines
//! (`down <x>` | `up`); frames land in `--sim <dir>` as `frame-NNNN.png`. Needs a Vulkan adapter
//! (lavapipe is fine). Rects come from `tb_core::place`; pixels compare with ±2 per channel.
//!
//! The binary path is read with `option_env!`, so this file compiles before the package has a
//! `touchbard` target and each case fails at run time until it exists.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::fs::File;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};

use tb_core::{BarSize, BarTheme, Chord, Rect, Rgba, SpacingRules, place, presets};

fn touchbard() -> Command {
    match option_env!("CARGO_BIN_EXE_touchbard") {
        Some(bin) => Command::new(bin),
        None => panic!("the tb-daemon package has no `touchbard` binary target"),
    }
}

fn run(args: &[&str], stdin: &str) -> Output {
    let mut child = touchbard()
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    child.stdin.take().unwrap().write_all(stdin.as_bytes()).unwrap();
    child.wait_with_output().unwrap()
}

fn fresh_dir(name: &str) -> PathBuf {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join("touchbard_cli").join(name);
    let _ = std::fs::remove_dir_all(&dir);
    dir
}

fn f5() -> Rect {
    let layout = presets::default_layout();
    let placed =
        place(layout.layer(Chord::Default).unwrap(), BarSize::MACBOOK_PRO, &SpacingRules::STANDARD).unwrap();
    placed.iter().find(|p| p.id.as_str() == "f5").unwrap().rect
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

/// The RGBA pixel at `(x, y)` of a PNG written by the simulator.
fn png_pixel(path: &Path, x: u32, y: u32) -> [u8; 4] {
    let mut reader = png::Decoder::new(File::open(path).unwrap()).read_info().unwrap();
    let mut buf = vec![0; reader.output_buffer_size()];
    let info = reader.next_frame(&mut buf).unwrap();
    assert_eq!((info.width, info.height), (2170, 60), "{}", path.display());
    assert_eq!(info.color_type, png::ColorType::Rgba, "{}", path.display());
    let i = ((y * info.width + x) * 4) as usize;
    [buf[i], buf[i + 1], buf[i + 2], buf[i + 3]]
}

fn close(a: [u8; 4], b: [u8; 4]) -> bool {
    a.iter().zip(b).all(|(x, y)| x.abs_diff(y) <= 2)
}

fn bytes(c: Rgba) -> [u8; 4] {
    [c.r, c.g, c.b, c.a]
}

#[test]
fn touchbard_presents_start_press_and_release_as_three_frames() {
    let dir = fresh_dir("press_release");
    let f5 = f5();
    let stdin = format!("down {}\nup\n", f5.x + f5.width / 2);
    let out = run(&["--sim", dir.to_str().unwrap()], &stdin);

    assert!(out.status.success(), "exit {:?}, stderr: {}", out.status, String::from_utf8_lossy(&out.stderr));
    assert_eq!(frame_files(&dir), ["frame-0001.png", "frame-0002.png", "frame-0003.png"]);

    let (x, y) = (u32::from(f5.x + 12), u32::from(f5.y + f5.height / 2));
    let pressed = bytes(BarTheme::omarchy_fallback().pressed_fill);
    let start = png_pixel(&dir.join("frame-0001.png"), x, y);
    let press = png_pixel(&dir.join("frame-0002.png"), x, y);
    let release = png_pixel(&dir.join("frame-0003.png"), x, y);
    assert!(close(press, pressed), "frame-0002 F5 = {press:?}, want pressed_fill {pressed:?}");
    assert!(!close(start, pressed), "frame-0001 F5 is idle, got {start:?}");
    assert!(close(release, start), "frame-0003 F5 is idle again: {release:?} vs {start:?}");
}

#[test]
fn touchbard_exits_with_an_error_naming_a_bad_stdin_line() {
    let dir = fresh_dir("bad_line");
    let out = run(&["--sim", dir.to_str().unwrap()], "press 3\n");
    assert!(!out.status.success(), "a bad line must fail, got {:?}", out.status);
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(stderr.contains("press 3"), "stderr names the line: {stderr}");
}

#[test]
fn touchbard_rejects_an_unknown_argument_with_usage_and_exit_two() {
    let out = run(&["--no-such-flag"], "");
    assert_eq!(out.status.code(), Some(2), "stderr: {}", String::from_utf8_lossy(&out.stderr));
    let stderr = String::from_utf8_lossy(&out.stderr).to_lowercase();
    assert!(stderr.contains("usage"), "stderr shows usage: {stderr}");
}
