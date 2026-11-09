//! A command is either never started or always reaped, even when the daemon cannot start one more
//! thread. The case runs this test binary again with `RUST_MIN_STACK` above the user address
//! space (2^48 bytes), so every new thread without its own stack size fails to be created
//! (`EAGAIN`), whatever the overcommit setting, while starting a process still works. libtest then
//! runs the test on its main thread. In that process `ShSpawner` gets a harmless command; after it
//! has had time to end, no child may be left as a zombie or still running.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::path::Path;
use std::process::Command;
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

use tb_daemon::{ActionError, Report, ShSpawner, Spawner};

/// A thread stack larger than the user address space: no thread can be created with it.
const NO_THREADS: &str = "281474976710656";
/// Set only in the process that runs the helper, so the helper never runs on its own.
const IN_HELPER: &str = "TB_COMMAND_REAPING_WITHOUT_THREADS";
const HELPER: &str = "a_command_started_where_no_thread_can_be_created";
/// How long the command may take to end, and its exit to be seen (loaded laptop).
const SETTLE: Duration = Duration::from_secs(3);

/// (pid, state) of every child of this process, from `/proc/<pid>/stat` (field 4 is the parent).
fn children() -> Vec<(u32, char)> {
    let me = std::process::id();
    let mut found = Vec::new();
    for entry in std::fs::read_dir("/proc").unwrap() {
        let name = entry.unwrap().file_name();
        let Some(pid) = name.to_str().and_then(|n| n.parse::<u32>().ok()) else { continue };
        let Ok(stat) = std::fs::read_to_string(format!("/proc/{pid}/stat")) else { continue };
        let Some(close) = stat.rfind(')') else { continue };
        let mut fields = stat[close + 1..].split_whitespace();
        let state = fields.next().and_then(|s| s.chars().next()).unwrap_or('?');
        let parent: Option<u32> = fields.next().and_then(|p| p.parse().ok());
        if parent == Some(me) {
            found.push((pid, state));
        }
    }
    found
}

#[test]
fn a_command_is_never_left_unreaped_when_its_reaper_cannot_start() {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join("command_reaping_without_threads");
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let out = Command::new(std::env::current_exe().unwrap())
        .args(["--exact", HELPER, "--ignored", "--nocapture", "--test-threads=1"])
        .env("RUST_MIN_STACK", NO_THREADS)
        .env(IN_HELPER, &dir)
        .output()
        .unwrap();
    let text = format!("{}{}", String::from_utf8_lossy(&out.stdout), String::from_utf8_lossy(&out.stderr));
    assert!(text.contains("1 passed"), "the helper ran and passed:\n{text}");
    assert!(out.status.success(), "{text}");
}

#[test]
#[ignore = "run by a_command_is_never_left_unreaped_when_its_reaper_cannot_start in a process without threads"]
fn a_command_started_where_no_thread_can_be_created() {
    let dir = std::env::var_os(IN_HELPER).expect("run only by the case above");
    let started = Path::new(&dir).join("started");
    assert!(
        thread::Builder::new().spawn(|| ()).is_err(),
        "precondition: no thread can be created in this process"
    );
    assert!(Command::new("true").status().unwrap().success(), "precondition: a process can start");

    let reports = Arc::new(Mutex::new(Vec::new()));
    let sink = Arc::clone(&reports);
    let report: Report = Arc::new(move |e: ActionError| sink.lock().unwrap().push(e.to_string()));
    let mut spawner = ShSpawner::new(report);
    let outcome = spawner.spawn(&format!("touch '{}'", started.display()));

    let start = Instant::now();
    while start.elapsed() < SETTLE && !children().iter().all(|(_, state)| *state == 'Z') {
        thread::sleep(Duration::from_millis(20));
    }
    let left = children();
    assert_eq!(
        left,
        Vec::<(u32, char)>::new(),
        "spawn returned {outcome:?}, the command {} started; children left (Z = never reaped): {left:?}",
        if started.exists() { "was" } else { "was not" }
    );
}
