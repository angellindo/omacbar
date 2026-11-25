//! Functional: the `touchbard` binary refuses an explicit device path that is not a `/dev/dri/cardN`
//! or `/dev/input/eventN` node with exit 1 (not the usage error 2), before anything is shown and
//! without touching the given file, and its simulator keeps ending on SIGHUP, SIGINT and SIGTERM by
//! their default action. Each run gets an empty HOME, a temp working directory and a temp `--sim`
//! dir under cargo's temp dir; signals go only to the child this file started. No run selects
//! `auto` or a `/dev` path, so no device, `/sys` entry or input device of the host is opened. Needs
//! a Vulkan adapter (lavapipe is fine), like the other `touchbard_*` tests.
//!
//! The binary path is read with `option_env!`, so this file compiles without the binary target.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::io::Read;
use std::os::unix::process::ExitStatusExt;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Output, Stdio};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::mpsc;
use std::thread;
use std::time::Duration;

const WAIT: Duration = Duration::from_secs(60);
const SIGHUP: i32 = 1;
const SIGINT: i32 = 2;
const SIGTERM: i32 = 15;

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
        .join("touchbard_hardware")
        .join(format!("{name}-{}-{n}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

/// `touchbard args` in `cwd` with an empty HOME and no stdin, killed (it is ours) after `WAIT`.
fn run_in(cwd: &Path, args: &[&str]) -> Output {
    let child = touchbard()
        .args(args)
        .current_dir(cwd)
        .env("HOME", fresh_dir("home"))
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    let pid = child.id();
    let (tx, done) = mpsc::channel();
    thread::spawn(move || {
        let _ = tx.send(child.wait_with_output());
    });
    let Ok(output) = done.recv_timeout(WAIT) else {
        let _ = Command::new("kill").args(["-s", "KILL", &pid.to_string()]).status();
        panic!("touchbard {args:?} did not end within {WAIT:?}");
    };
    output.unwrap()
}

#[test]
fn an_explicit_device_path_outside_dev_is_refused_with_exit_one_and_left_unchanged() {
    let dir = fresh_dir("not-a-node");
    let card = dir.join("card0");
    let event = dir.join("event0");
    for file in [&card, &event] {
        std::fs::write(file, b"an ordinary file").unwrap();
    }
    let sim = dir.join("sim");
    let (card, event, sim) = (card.to_str().unwrap(), event.to_str().unwrap(), sim.to_str().unwrap());

    let usage = run_in(&dir, &["--drm", card, "--sim", sim]);
    assert_eq!(usage.status.code(), Some(2), "--drm with --sim is a usage error");

    for (label, args, path) in [
        ("display", vec!["--drm", card], card),
        ("display and digitizer", vec!["--drm", card, "--touch", event], card),
        ("digitizer on the simulator", vec!["--touch", event, "--sim", sim], event),
    ] {
        let out = run_in(&dir, &args);
        let stderr = String::from_utf8_lossy(&out.stderr);
        assert_eq!(out.status.code(), Some(1), "{label}: a refusal, not a usage error: {stderr}");
        assert!(stderr.contains(path), "{label}: names the path: {stderr}");
        assert!(!Path::new(sim).join("frame-0001.png").exists(), "{label}: nothing shown");
        assert!(!dir.join("out").exists(), "{label}: no default simulator dir");
    }
    for file in [card, event] {
        assert_eq!(std::fs::read(file).unwrap(), b"an ordinary file", "{file} is unchanged");
    }
}

/// Waits until `child` has shown its first frame in `sim`: its stderr stays open while it runs, so
/// the wait polls the frame file only against a deadline.
fn wait_for_first_frame(child: &mut Child, sim: &Path, name: &str) {
    let start = std::time::Instant::now();
    while !sim.join("frame-0001.png").is_file() {
        if child.try_wait().unwrap().is_some() || start.elapsed() > WAIT {
            let _ = child.kill();
            let _ = child.wait();
            panic!("SIG{name}: the simulator never showed its first frame");
        }
        thread::sleep(Duration::from_millis(10));
    }
}

#[test]
fn the_simulator_still_ends_on_sighup_sigint_and_sigterm_by_their_default_action() {
    for (name, signal) in [("HUP", SIGHUP), ("INT", SIGINT), ("TERM", SIGTERM)] {
        let dir = fresh_dir("signal-sim");
        let sim = dir.join("sim");
        let mut child = touchbard()
            .args(["--sim", sim.to_str().unwrap()])
            .current_dir(&dir)
            .env("HOME", fresh_dir("home"))
            .stdin(Stdio::piped())
            .stdout(Stdio::null())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap();
        wait_for_first_frame(&mut child, &sim, name);
        let sent = Command::new("sh")
            .args(["-c", "kill -s \"$1\" \"$2\"", "sh", name, &child.id().to_string()])
            .status()
            .unwrap();
        assert!(sent.success(), "SIG{name} sent");
        // Its stderr closes when it exits; reading it to the end waits for that without polling.
        let mut stderr = child.stderr.take().unwrap();
        let (tx, ended) = mpsc::channel();
        thread::spawn(move || {
            let mut text = String::new();
            let _ = stderr.read_to_string(&mut text);
            let _ = tx.send(text);
        });
        if ended.recv_timeout(WAIT).is_err() {
            let _ = child.kill();
            let _ = child.wait();
            panic!("SIG{name}: the simulator kept running");
        }
        let status = child.wait().unwrap();
        assert_eq!(status.signal(), Some(signal), "SIG{name} still ends the simulator unhandled: {status:?}");
    }
}
