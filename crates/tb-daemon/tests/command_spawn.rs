//! A command action runs its command line through `sh -c`, detached: `spawn` returns as soon as
//! the command has started, the command gets its own process group and no stdin, and a command
//! that exits with a failure is reported with its command line and exit status, never fatal.
//! Real `sh` with harmless commands that write only under cargo's temp dir.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

use tb_daemon::{ActionError, Report, ShSpawner, Spawner};

/// How long a short command may take to finish and be reported (loaded laptop).
const SETTLE: Duration = Duration::from_secs(5);

fn collecting() -> (Report, Arc<Mutex<Vec<ActionError>>>) {
    let reports = Arc::new(Mutex::new(Vec::new()));
    let sink = Arc::clone(&reports);
    let report: Report = Arc::new(move |e: ActionError| sink.lock().unwrap().push(e));
    (report, reports)
}

fn fresh_dir(test: &str) -> PathBuf {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join("command_spawn").join(test);
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

/// Polls `done` every 10 ms until it holds or `SETTLE` passes; whether it held.
fn eventually(mut done: impl FnMut() -> bool) -> bool {
    let start = Instant::now();
    while start.elapsed() < SETTLE {
        if done() {
            return true;
        }
        thread::sleep(Duration::from_millis(10));
    }
    done()
}

fn read(path: &Path) -> Option<String> {
    std::fs::read_to_string(path).ok()
}

#[test]
fn a_command_line_runs_through_sh_with_its_shell_syntax() {
    let dir = fresh_dir("shell_syntax");
    let out = dir.join("out");
    let (report, reports) = collecting();
    let mut spawner = ShSpawner::new(report);
    spawner.spawn(&format!("echo $((1 + 2)) hi > '{}'", out.display())).unwrap();
    assert!(eventually(|| read(&out).as_deref() == Some("3 hi\n")), "out: {:?}", read(&out));
    thread::sleep(Duration::from_millis(200));
    let seen = std::mem::take(&mut *reports.lock().unwrap());
    assert!(seen.is_empty(), "{seen:?}");
}

#[test]
fn spawn_returns_without_waiting_for_the_command_to_end() {
    let (report, _reports) = collecting();
    let mut spawner = ShSpawner::new(report);
    let start = Instant::now();
    spawner.spawn("sleep 3").unwrap();
    let took = start.elapsed();
    assert!(took < Duration::from_secs(1), "spawn blocked for {took:?}");
}

#[test]
fn a_command_that_exits_3_is_reported_with_its_command_line_and_status() {
    let (report, reports) = collecting();
    let mut spawner = ShSpawner::new(report);
    spawner.spawn("exit 3").unwrap();
    assert!(eventually(|| !reports.lock().unwrap().is_empty()), "no report");
    let reports = reports.lock().unwrap();
    assert_eq!(reports.len(), 1, "{reports:?}");
    assert!(
        matches!(&reports[0], ActionError::Exited { run, status } if run == "exit 3" && status.code() == Some(3)),
        "{reports:?}"
    );
    let message = reports[0].to_string();
    assert!(message.contains("exit 3"), "names the command line: {message}");
    assert!(message.contains("exit status: 3"), "names the status: {message}");
    assert!(!message.contains('\n'), "one line: {message}");
}

#[test]
fn a_command_sh_cannot_find_is_reported_with_status_127() {
    let (report, reports) = collecting();
    let mut spawner = ShSpawner::new(report);
    spawner.spawn("no-such-command-tb").unwrap();
    assert!(eventually(|| !reports.lock().unwrap().is_empty()), "no report");
    let reports = reports.lock().unwrap();
    assert_eq!(reports.len(), 1, "{reports:?}");
    assert!(
        matches!(&reports[0], ActionError::Exited { run, status }
            if run == "no-such-command-tb" && status.code() == Some(127)),
        "{reports:?}"
    );
}

#[test]
fn a_command_that_succeeds_reports_nothing() {
    let dir = fresh_dir("succeeds");
    let done = dir.join("done");
    let (report, reports) = collecting();
    let mut spawner = ShSpawner::new(report);
    spawner.spawn(&format!("true && touch '{}'", done.display())).unwrap();
    assert!(eventually(|| done.exists()), "the command ran");
    thread::sleep(Duration::from_millis(300));
    let seen = std::mem::take(&mut *reports.lock().unwrap());
    assert!(seen.is_empty(), "{seen:?}");
}

#[test]
fn a_command_runs_in_its_own_process_group() {
    let dir = fresh_dir("process_group");
    let out = dir.join("out");
    let (report, _reports) = collecting();
    let mut spawner = ShSpawner::new(report);
    let line = format!(
        "read -r pid comm state ppid pgrp rest < /proc/$$/stat; echo \"$$ $pgrp\" > '{}.tmp' && mv '{0}.tmp' '{0}'",
        out.display()
    );
    spawner.spawn(&line).unwrap();
    assert!(eventually(|| out.exists()), "the command ran");
    let text = read(&out).unwrap();
    let ids: Vec<u32> = text.split_whitespace().map(|n| n.parse().unwrap()).collect();
    let (shell, group) = (ids[0], ids[1]);
    assert_eq!(group, shell, "the shell leads its own process group: {text}");
    let own = std::fs::read_to_string("/proc/self/stat").unwrap();
    let own_group: u32 = own[own.rfind(')').unwrap() + 2..].split(' ').nth(2).unwrap().parse().unwrap();
    assert_ne!(group, own_group, "not the daemon's process group");
}

#[test]
fn a_command_reads_nothing_from_the_daemons_stdin() {
    let dir = fresh_dir("stdin");
    let (input, done) = (dir.join("input"), dir.join("done"));
    let (report, _reports) = collecting();
    let mut spawner = ShSpawner::new(report);
    spawner.spawn(&format!("cat > '{}'; touch '{}'", input.display(), done.display())).unwrap();
    assert!(eventually(|| done.exists()), "cat waited on a stdin that never ends");
    assert_eq!(read(&input).as_deref(), Some(""));
}
