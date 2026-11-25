//! Late conflicts without a product hook: a program (the real `touchbar-recover`, or this test
//! binary running one helper with the real library or configurator) runs under
//! `syscall_pause.py`, which holds it at one step of its protected write while the test changes
//! the config:
//!
//! - `Step::Stage`: it is creating the temp generation directory in the store, so its final check
//!   before publishing the generation is still to come (a conflict found there keeps nothing);
//! - `Step::Publish`: the generation is renamed into the store under its id and the store is being
//!   synced, so the config has not been replaced yet (a conflict found now is after the keep).
//!
//! Only writes after `<signals>/armed` exists count; a helper arms right before the action under
//! test, the parent arms before starting a binary whose only write is that action.
#![allow(dead_code)]

use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

/// How long a held run may take in all (one CPU, nice 19, debug build).
const LIMIT: Duration = Duration::from_secs(600);

/// Where the supervisor is, from either crate's tests.
pub fn supervisor() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../tb-config/tests/support/syscall_pause.py")
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Step {
    Stage,
    Publish,
}

impl Step {
    fn word(self) -> &'static str {
        match self {
            Self::Stage => "stage",
            Self::Publish => "publish",
        }
    }
}

/// Where the program was held: the step and, after a publication, the id of the generation it
/// published (the directory name in the store).
#[derive(Debug, Clone)]
pub struct Paused {
    pub step: String,
    pub generation: Option<String>,
    pub syscall: String,
}

/// How a held run ended.
#[derive(Debug)]
pub struct Held {
    pub code: Option<i32>,
    pub stdout: String,
    pub stderr: String,
    pub paused: Option<Paused>,
    pub log: String,
}

impl Held {
    /// The run was held at `step` (after a publication: with a generation id); returns it.
    pub fn held_at(&self, step: Step) -> &Paused {
        let paused = self.paused.as_ref().unwrap_or_else(|| {
            panic!("the program never reached the `{}` step of a protected write; {self:#?}", step.word())
        });
        assert_eq!(paused.step, step.word(), "{self:#?}");
        if step == Step::Publish {
            assert!(paused.generation.is_some(), "{self:#?}");
        }
        paused
    }

    /// The id of the generation published before the conflict.
    pub fn published(&self) -> String {
        self.held_at(Step::Publish).generation.clone().unwrap_or_default()
    }
}

/// Empties `signals` (created if missing).
pub fn signals_at(dir: &Path) -> PathBuf {
    let _ = fs::remove_dir_all(dir);
    fs::create_dir_all(dir).unwrap();
    dir.to_path_buf()
}

/// From now on, the supervisor holds the first matching write.
pub fn arm(signals: &Path) {
    fs::write(signals.join("armed"), b"armed\n").unwrap();
}

/// What the supervisor wrote when it held the program, if it did (read by a helper after its
/// action returned).
pub fn paused_in(signals: &Path) -> Option<Paused> {
    let text = fs::read_to_string(signals.join("paused")).ok()?;
    let value: serde_json::Value = serde_json::from_str(&text).unwrap();
    let field = |k: &str| value.get(k).and_then(serde_json::Value::as_str).map(str::to_owned);
    Some(Paused {
        step: field("step").unwrap_or_default(),
        generation: field("generation"),
        syscall: field("syscall").unwrap_or_default(),
    })
}

/// Runs `cmd` (program, arguments and environment as configured on it) under the supervisor,
/// held at `step` of its first armed protected write into `store`; `at_pause` runs while it is
/// held, then the program goes on. Its stdout and stderr are kept in `signals`.
pub fn run_held(
    cmd: &Command,
    signals: &Path,
    store: &Path,
    step: Step,
    at_pause: impl FnOnce(&Paused),
) -> Held {
    let mut held = Command::new("python3");
    held.arg("-I")
        .arg(supervisor())
        .arg(signals)
        .arg(store)
        .arg(step.word())
        .arg("--")
        .arg(cmd.get_program())
        .args(cmd.get_args());
    for (key, value) in cmd.get_envs() {
        match value {
            Some(value) => held.env(key, value),
            None => held.env_remove(key),
        };
    }
    let (out, err) = (signals.join("stdout"), signals.join("stderr"));
    held.stdin(Stdio::null()).stdout(fs::File::create(&out).unwrap()).stderr(fs::File::create(&err).unwrap());
    let mut child = held.spawn().unwrap();
    let mut at_pause = Some(at_pause);
    let deadline = Instant::now() + LIMIT;
    let status = loop {
        if let Some(status) = child.try_wait().unwrap() {
            break status;
        }
        if at_pause.is_some()
            && let Some(paused) = paused_in(signals)
        {
            (at_pause.take().unwrap())(&paused);
            fs::write(signals.join("resume"), b"resume\n").unwrap();
        }
        if Instant::now() > deadline {
            let _ = child.kill();
            panic!("a held run took longer than {LIMIT:?}; log:\n{}", log(signals));
        }
        std::thread::sleep(Duration::from_millis(10));
    };
    Held {
        code: status.code(),
        stdout: fs::read_to_string(out).unwrap_or_default(),
        stderr: fs::read_to_string(err).unwrap_or_default(),
        paused: paused_in(signals),
        log: log(signals),
    }
}

fn log(signals: &Path) -> String {
    fs::read_to_string(signals.join("supervisor.log")).unwrap_or_default()
}

/// This test binary, running only the ignored `helper` with `vars` set (libtest, one thread).
pub fn helper_command(helper: &str, vars: &[(&str, &Path)]) -> Command {
    let mut cmd = Command::new(std::env::current_exe().unwrap());
    cmd.args([helper, "--exact", "--ignored", "--nocapture", "--test-threads=1"]);
    for (key, value) in vars {
        cmd.env(key, value);
    }
    cmd
}

/// The helper's run passed: exit 0 and libtest says one test passed.
pub fn helper_passed(held: &Held) {
    assert_eq!(held.code, Some(0), "the helper failed; {held:#?}");
    assert!(held.stdout.contains("1 passed"), "the helper did not run; {held:#?}");
}

/// An editor outside the library replacing `path` atomically with `bytes` (temp + rename).
pub fn replace_by_hand(path: &Path, bytes: &[u8]) {
    let staged = path.with_file_name(".config.toml.by-hand");
    fs::write(&staged, bytes).unwrap();
    fs::rename(staged, path).unwrap();
}

/// Replaces `path` atomically with a symlink to `target` (which holds `bytes`).
pub fn replace_with_link(path: &Path, target: &Path, bytes: &[u8]) {
    fs::write(target, bytes).unwrap();
    let staged = path.with_file_name(".config.toml.link");
    std::os::unix::fs::symlink(target, &staged).unwrap();
    fs::rename(staged, path).unwrap();
}

/// Names in the store that are temp generation directories (`.tmp-*`).
pub fn temps_in(store: &Path) -> Vec<String> {
    let Ok(entries) = fs::read_dir(store) else { return Vec::new() };
    entries
        .flatten()
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .filter(|n| n.starts_with(".tmp-"))
        .collect()
}
