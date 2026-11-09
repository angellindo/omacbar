//! What a tap does: press a button's keys on the virtual keyboard or start its command. Failures
//! are reported, never returned, so the daemon goes on (ADR-0011, ADR-0012).

use std::io;
use std::os::unix::process::CommandExt;
use std::process::{Child, Command, ExitStatus, Stdio};
use std::sync::{Arc, mpsc};
use std::thread;

use tb_core::{Action, Button, ButtonId, Chord, Key};
use thiserror::Error;

use crate::uinput::EmitError;

/// Presses keys on a virtual keyboard: the device, a record file, or a fake in tests.
pub trait KeyEmitter {
    /// Presses `keys` in order, then releases them in reverse.
    fn emit(&mut self, keys: &[Key]) -> Result<(), EmitError>;
}

/// Starts command lines.
pub trait Spawner {
    /// Starts `run` and returns without waiting for it to end.
    fn spawn(&mut self, run: &str) -> io::Result<()>;
}

/// Where action failures go: the binary logs them. Called from the daemon loop and from the
/// threads that wait for commands.
pub type Report = Arc<dyn Fn(ActionError) + Send + Sync>;

/// Why a tapped action did not do what it says. Each message is one line.
#[derive(Debug, Error)]
pub enum ActionError {
    #[error(
        "layer `{chord:?}`: button `{id}` was not pressed: keys on a modifier layer would combine with the held modifier; make it a command"
    )]
    ModifierLayer { chord: Chord, id: ButtonId },
    #[error("layer `{chord:?}`: button `{id}` was not pressed: {keys:?} are not all key names")]
    BadKeys { chord: Chord, id: ButtonId, keys: Vec<String> },
    #[error("layer `{chord:?}`: button `{id}` was not pressed: there is no virtual keyboard")]
    NoKeyboard { chord: Chord, id: ButtonId },
    #[error("layer `{chord:?}`: button `{id}` was not pressed: {source}")]
    Emit { chord: Chord, id: ButtonId, source: EmitError },
    #[error("cannot run `{}`: {source}", one_line(run))]
    Spawn { run: String, source: io::Error },
    #[error("`{}` ended with {status}", one_line(run))]
    Exited { run: String, status: ExitStatus },
}

/// `run` with every control character escaped (`\n`, `\r`, `\t`, `\u{..}`), so a command of
/// several lines is still one log line and stays recognisable.
fn one_line(run: &str) -> String {
    run.chars().flat_map(|c| if c.is_control() { c.escape_default().collect() } else { vec![c] }).collect()
}

/// Runs the actions of tapped buttons.
pub struct Actions {
    /// `None` for [`Actions::none`].
    runner: Option<Runner>,
}

struct Runner {
    keys: Option<Box<dyn KeyEmitter + Send>>,
    spawner: Box<dyn Spawner + Send>,
    report: Report,
}

impl Actions {
    /// Presses keys on `keys` (or reports `NoKeyboard` without one), starts commands with
    /// `spawner`, and sends every failure to `report`.
    pub fn new(
        keys: Option<Box<dyn KeyEmitter + Send>>,
        spawner: Box<dyn Spawner + Send>,
        report: Report,
    ) -> Self {
        Self { runner: Some(Runner { keys, spawner, report }) }
    }

    /// Runs nothing and reports nothing.
    pub fn none() -> Self {
        Self { runner: None }
    }

    /// Runs `button`'s action, tapped on the `chord` layer. Keys are not pressed on a modifier
    /// layer (ADR-0011); commands run on every layer. Failures go to the report.
    pub fn run(&mut self, button: &Button, chord: Chord) {
        let Some(runner) = &mut self.runner else { return };
        let failed = match &button.action {
            Action::None => None,
            Action::Key { keys } => runner.press(button, chord, keys),
            Action::Command { run } => {
                runner.spawner.spawn(run).err().map(|source| ActionError::Spawn { run: run.clone(), source })
            }
        };
        if let Some(error) = failed {
            (runner.report)(error);
        }
    }
}

impl Runner {
    fn press(&mut self, button: &Button, chord: Chord, names: &[String]) -> Option<ActionError> {
        let id = || button.id.clone();
        if chord.holds_modifier() {
            return Some(ActionError::ModifierLayer { chord, id: id() });
        }
        let keys: Option<Vec<Key>> = names.iter().map(|n| Key::named(n)).collect();
        let Some(keys) = keys.filter(|k| !k.is_empty()) else {
            return Some(ActionError::BadKeys { chord, id: id(), keys: names.to_vec() });
        };
        let Some(keyboard) = &mut self.keys else {
            return Some(ActionError::NoKeyboard { chord, id: id() });
        };
        keyboard.emit(&keys).err().map(|source| ActionError::Emit { chord, id: id(), source })
    }
}

/// Runs command lines through `sh -c`, detached: no stdin, stdout discarded, stderr shared with
/// the daemon's log, and its own process group, so terminal signals to the daemon miss it. A
/// thread per command waits for it, so no zombie is left, and reports a failed exit. The thread
/// starts before the command: when it cannot start, the command is not started either.
pub struct ShSpawner {
    report: Report,
}

impl ShSpawner {
    pub fn new(report: Report) -> Self {
        Self { report }
    }
}

impl Spawner for ShSpawner {
    fn spawn(&mut self, run: &str) -> io::Result<()> {
        let (handover, started) = mpsc::channel::<Child>();
        let (report, line) = (Arc::clone(&self.report), run.to_owned());
        // A sender dropped without a child (the shell did not start) ends the thread.
        thread::Builder::new().name("command".to_owned()).spawn(move || {
            if let Ok(child) = started.recv() {
                reap(child, line, &report);
            }
        })?;
        let child = Command::new("sh")
            .arg("-c")
            .arg(run)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .process_group(0)
            .spawn()?;
        // The thread waits on the channel until it gets the child, so the send cannot fail; if it
        // ever did, the child comes back and is reaped here rather than left a zombie.
        if let Err(mpsc::SendError(child)) = handover.send(child) {
            reap(child, run.to_owned(), &self.report);
        }
        Ok(())
    }
}

/// Waits for `child` to end and reports a failed exit.
fn reap(mut child: Child, run: String, report: &Report) {
    match child.wait() {
        Ok(status) if !status.success() => report(ActionError::Exited { run, status }),
        Ok(_) => {}
        Err(source) => report(ActionError::Spawn { run, source }),
    }
}
