//! Every action failure is one log line, even for a command line that spans several lines: the
//! message keeps the command recognisable (in any escaped single-line form) but holds no line
//! break (`\n` or `\r`). `Exited` comes from the real `ShSpawner` running a harmless script;
//! `Spawn` comes from `Actions` with a spawner that cannot start anything.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::io;
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

use tb_core::{Action, Button, Chord};
use tb_daemon::{ActionError, Actions, Report, ShSpawner, Spawner};

/// Two lines with a carriage return inside a shell comment; the script exits 3.
const SCRIPT: &str = "true # first line\r\nexit 3";
/// How long the script may take to end and be reported (loaded laptop).
const SETTLE: Duration = Duration::from_secs(5);

fn collecting() -> (Report, Arc<Mutex<Vec<ActionError>>>) {
    let reports = Arc::new(Mutex::new(Vec::new()));
    let sink = Arc::clone(&reports);
    let report: Report = Arc::new(move |e: ActionError| sink.lock().unwrap().push(e));
    (report, reports)
}

/// The message is one line and still shows both lines of the script.
fn assert_one_line_naming_the_script(message: &str) {
    assert!(!message.contains('\n') && !message.contains('\r'), "one line: {message:?}");
    assert!(message.contains("true # first line"), "the first line is recognisable: {message:?}");
    assert!(message.contains("exit 3"), "the second line is recognisable: {message:?}");
}

/// A spawner that can never start a command.
struct FailingSpawner;

impl Spawner for FailingSpawner {
    fn spawn(&mut self, _run: &str) -> io::Result<()> {
        Err(io::Error::other("no shell in this test"))
    }
}

#[test]
fn a_script_of_several_lines_that_fails_is_reported_on_one_line() {
    let (report, reports) = collecting();
    let mut spawner = ShSpawner::new(report);
    spawner.spawn(SCRIPT).unwrap();
    let start = Instant::now();
    while start.elapsed() < SETTLE && reports.lock().unwrap().is_empty() {
        thread::sleep(Duration::from_millis(10));
    }
    let seen = std::mem::take(&mut *reports.lock().unwrap());
    assert_eq!(seen.len(), 1, "{seen:?}");
    assert!(matches!(&seen[0], ActionError::Exited { status, .. } if status.code() == Some(3)), "{seen:?}");
    let message = seen[0].to_string();
    assert_one_line_naming_the_script(&message);
    assert!(message.contains("exit status: 3"), "{message:?}");
}

#[test]
fn a_script_of_several_lines_that_cannot_start_is_reported_on_one_line() {
    let (report, reports) = collecting();
    let mut actions = Actions::new(None, Box::new(FailingSpawner), report);
    let button = Button::new("script", "script", Action::Command { run: SCRIPT.to_owned() });
    actions.run(&button, Chord::FnCtrl);
    let seen = std::mem::take(&mut *reports.lock().unwrap());
    assert_eq!(seen.len(), 1, "{seen:?}");
    assert!(matches!(&seen[0], ActionError::Spawn { .. }), "{seen:?}");
    let message = seen[0].to_string();
    assert_one_line_naming_the_script(&message);
    assert!(message.contains("no shell in this test"), "the cause is kept: {message:?}");
}
