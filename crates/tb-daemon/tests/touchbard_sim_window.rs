//! Functional: `touchbard --sim-window` and the headless runs beside it. Every run removes
//! `WAYLAND_DISPLAY`, `WAYLAND_SOCKET` and `DISPLAY` from the child's environment and points
//! `XDG_RUNTIME_DIR` at an empty directory, so no test ever opens a window: with the flag the
//! binary must fail cleanly (exit 1, `simulator window` on stderr) without waiting on stdin;
//! without it the simulator run works as before, needing no display. Each run gets an empty
//! `HOME` (the bundled Tokyo Night) and is killed if it does not end within `WAIT`.
//! Headless runs need a Vulkan adapter (lavapipe is fine).
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::fs::File;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, ExitStatus, Stdio};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::{Duration, Instant};

use tb_core::{BarSize, BarTheme, Chord, Rect, Rgba, SpacingRules, place, presets};

const WAIT: Duration = Duration::from_secs(20);

struct Run {
    status: ExitStatus,
    stderr: String,
}

/// A directory with nothing in it, unique to one call.
fn scratch(kind: &str) -> PathBuf {
    static RUNS: AtomicUsize = AtomicUsize::new(0);
    let n = RUNS.fetch_add(1, Ordering::Relaxed);
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR"))
        .join("touchbard_sim_window")
        .join(format!("{kind}-{}-{n}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

/// `touchbard` with no display to connect to and an empty home.
fn touchbard(args: &[&str]) -> Command {
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_touchbard"));
    cmd.args(args)
        .env_remove("WAYLAND_DISPLAY")
        .env_remove("WAYLAND_SOCKET")
        .env_remove("DISPLAY")
        .env("XDG_RUNTIME_DIR", scratch("runtime"))
        .env("HOME", scratch("home"))
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::piped());
    cmd
}

/// Waits for `child` up to `WAIT`; kills it and fails the test if it is still running.
fn finish(mut child: Child, what: &str) -> Run {
    let start = Instant::now();
    loop {
        if child.try_wait().unwrap().is_some() {
            break;
        }
        if start.elapsed() > WAIT {
            let _ = child.kill();
            let _ = child.wait();
            panic!("{what}: still running after {WAIT:?} (it must not wait for stdin or a window)");
        }
        std::thread::sleep(Duration::from_millis(50));
    }
    let out = child.wait_with_output().unwrap();
    Run { status: out.status, stderr: String::from_utf8_lossy(&out.stderr).into_owned() }
}

/// Runs `touchbard args` with stdin held open (never written, never closed) until it exits.
fn run_with_stdin_open(args: &[&str]) -> Run {
    let mut child = touchbard(args).spawn().unwrap();
    let stdin = child.stdin.take().unwrap();
    let run = finish(child, &args.join(" "));
    drop(stdin);
    run
}

/// Runs `touchbard args` with `input` on stdin, then stdin closed.
fn run_with_stdin(args: &[&str], input: &str) -> Run {
    let mut child = touchbard(args).spawn().unwrap();
    let mut stdin = child.stdin.take().unwrap();
    stdin.write_all(input.as_bytes()).unwrap();
    drop(stdin);
    finish(child, &args.join(" "))
}

fn assert_no_window_fails_cleanly(run: &Run, what: &str) {
    assert_eq!(run.status.code(), Some(1), "{what}: exit {:?}, stderr: {}", run.status, run.stderr);
    assert!(
        run.stderr.contains("simulator window"),
        "{what}: stderr names the simulator window: {}",
        run.stderr
    );
}

fn assert_usage_names_the_flag(run: &Run, what: &str) {
    assert_eq!(run.status.code(), Some(2), "{what}: exit {:?}, stderr: {}", run.status, run.stderr);
    assert!(run.stderr.to_lowercase().contains("usage"), "{what}: stderr shows usage: {}", run.stderr);
    assert!(run.stderr.contains("--sim-window"), "{what}: usage names --sim-window: {}", run.stderr);
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
fn sim_window_without_a_display_exits_one_naming_the_simulator_window() {
    let run = run_with_stdin_open(&["--sim-window"]);
    assert_no_window_fails_cleanly(&run, "--sim-window");
}

#[test]
fn sim_window_combines_with_a_sim_directory() {
    let dir = scratch("frames");
    let run = run_with_stdin_open(&["--sim", dir.to_str().unwrap(), "--sim-window"]);
    assert_no_window_fails_cleanly(&run, "--sim DIR --sim-window");
}

#[test]
fn sim_window_combines_with_a_sim_directory_and_keys_in_any_order() {
    let dir = scratch("frames");
    let keys = scratch("keys").join("no-key-events");
    std::fs::write(&keys, b"").unwrap();
    let run = run_with_stdin_open(&[
        "--sim-window",
        "--sim",
        dir.to_str().unwrap(),
        "--keys",
        keys.to_str().unwrap(),
    ]);
    assert_no_window_fails_cleanly(&run, "--sim-window --sim DIR --keys PATH");
}

#[test]
fn sim_window_given_twice_is_refused_with_usage() {
    let run = run_with_stdin(&["--sim-window", "--sim-window"], "");
    assert_usage_names_the_flag(&run, "--sim-window --sim-window");
}

#[test]
fn sim_window_takes_no_value() {
    let run = run_with_stdin(&["--sim-window", "extra"], "");
    assert_usage_names_the_flag(&run, "--sim-window extra");
}

#[test]
fn an_unknown_argument_shows_a_usage_that_names_sim_window() {
    let run = run_with_stdin(&["--no-such-flag"], "");
    assert_usage_names_the_flag(&run, "--no-such-flag");
}

#[test]
fn a_headless_run_without_a_display_still_presses_f5_from_stdin() {
    let dir = scratch("headless");
    let f5 = f5();
    let input = format!("down {}\nup\n", f5.x + f5.width / 2);
    let run = run_with_stdin(&["--sim", dir.to_str().unwrap()], &input);

    assert!(run.status.success(), "exit {:?}, stderr: {}", run.status, run.stderr);
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
fn a_headless_run_ends_when_stdin_closes() {
    let dir = scratch("stdin_closed");
    let run = run_with_stdin(&["--sim", dir.to_str().unwrap()], "");
    assert!(run.status.success(), "exit {:?}, stderr: {}", run.status, run.stderr);
    assert_eq!(frame_files(&dir), ["frame-0001.png"], "only the start frame");
}
