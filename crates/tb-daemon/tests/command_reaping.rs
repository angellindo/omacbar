//! Every command the daemon starts is reaped when it ends: no zombie child is left behind, whether
//! it succeeded, failed or was never found. Alone in its file, so no other test of the same binary
//! starts children meanwhile. Children are found by scanning `/proc/<pid>/stat` for this process
//! as parent (field 4); state `Z` is a zombie.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

use tb_daemon::{ActionError, Report, ShSpawner, Spawner};

/// How long short commands may take to end and be reaped (loaded laptop).
const SETTLE: Duration = Duration::from_secs(5);

/// (pid, state) of every child of this process.
fn children() -> Vec<(u32, char)> {
    let me = std::process::id();
    let mut found = Vec::new();
    for entry in std::fs::read_dir("/proc").unwrap() {
        let name = entry.unwrap().file_name();
        let Some(pid) = name.to_str().and_then(|n| n.parse::<u32>().ok()) else { continue };
        // A process may end between the listing and the read.
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

fn zombies() -> Vec<u32> {
    children().into_iter().filter(|(_, state)| *state == 'Z').map(|(pid, _)| pid).collect()
}

#[test]
fn ended_commands_leave_no_zombie_child() {
    let reports = Arc::new(Mutex::new(Vec::new()));
    let sink = Arc::clone(&reports);
    let report: Report = Arc::new(move |e: ActionError| sink.lock().unwrap().push(e));
    let mut spawner = ShSpawner::new(report);

    spawner.spawn("sleep 1").unwrap();
    assert!(!children().is_empty(), "precondition: the running command is seen as a child");

    for run in ["true", "exit 3", "no-such-command-tb", "sleep 0.2"] {
        spawner.spawn(run).unwrap();
    }
    let start = Instant::now();
    while start.elapsed() < SETTLE && (reports.lock().unwrap().len() < 2 || !children().is_empty()) {
        thread::sleep(Duration::from_millis(20));
    }
    let seen = std::mem::take(&mut *reports.lock().unwrap());
    assert_eq!(seen.len(), 2, "exit 3 and 127 reported: {seen:?}");
    assert_eq!(zombies(), Vec::<u32>::new(), "children: {:?}", children());
    assert_eq!(children(), Vec::<(u32, char)>::new(), "every command ended and was reaped");
}
