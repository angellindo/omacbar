//! The real `touchbar-customizer` keeps a generation before it writes the profile: before the
//! first default (the absence, and whether the directory existed) and before a save from the
//! editor (the exact hand-written bytes and permission bits it replaces, while the saved file
//! keeps those bits). A new process (this test) finds the generation complete; quitting and
//! reopening keeps nothing new. When the copy cannot be written (file size limit, EFBIG) the save
//! is abandoned and the hand-written file stays byte for byte; with room for the copy the same
//! save goes through. Every run has its own temp `HOME` and `XDG_STATE_HOME` under cargo's temp
//! dir, never the real ones. Needs a GPU adapter (lavapipe is fine).
//!
//! The store is read straight from disk, in the layout ADR-0014 fixes: one directory per
//! generation holding `manifest.toml` (`kind`, `reason`, `mode`, ...) and, for a kept file,
//! `config.toml`.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::ffi::OsStr;
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

use tb_config::default_path;
use tb_core::config::{Config, to_toml};

const STORE: &str = "touchbar-customizer/backups";
/// The file size limit of the "no room for the copy" runs, in bytes.
const LIMIT: usize = 64 * 1024;

struct Profile {
    dir: PathBuf,
    home: PathBuf,
    state: PathBuf,
}

impl Profile {
    fn new(test: &str) -> Self {
        let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join("configurator_generations").join(test);
        if dir.exists() {
            fs::remove_dir_all(&dir).unwrap();
        }
        let (home, state) = (dir.join("home"), dir.join("state"));
        fs::create_dir_all(&home).unwrap();
        fs::create_dir_all(&state).unwrap();
        Self { dir, home, state }
    }

    fn config(&self) -> PathBuf {
        default_path(&self.home)
    }

    fn store(&self) -> PathBuf {
        self.state.join(STORE)
    }

    fn seed(&self, bytes: &[u8], mode: u32) {
        fs::create_dir_all(self.config().parent().unwrap()).unwrap();
        fs::write(self.config(), bytes).unwrap();
        fs::set_permissions(self.config(), fs::Permissions::from_mode(mode)).unwrap();
    }

    fn command(&self, program: &OsStr) -> Command {
        let mut command = Command::new(program);
        command.env("HOME", &self.home).env("XDG_STATE_HOME", &self.state).current_dir(&self.dir);
        command
    }

    /// `--snap <dir>/<tag>.png` plus `extra`.
    fn args(&self, tag: &str, extra: &[&OsStr]) -> Vec<std::ffi::OsString> {
        let mut args = vec!["--snap".into(), self.dir.join(format!("{tag}.png")).into_os_string()];
        args.extend(extra.iter().map(|a| a.to_os_string()));
        args
    }

    /// One configurator run; it must exit 0.
    fn run(&self, tag: &str, extra: &[&OsStr]) {
        let out = self
            .command(OsStr::new(env!("CARGO_BIN_EXE_touchbar-customizer")))
            .args(self.args(tag, extra))
            .output()
            .unwrap();
        assert!(
            out.status.success(),
            "exit {:?}, stderr: {}",
            out.status,
            String::from_utf8_lossy(&out.stderr)
        );
    }

    /// One configurator run under a file size limit of `LIMIT` bytes, `SIGXFSZ` ignored so an
    /// oversized write fails with EFBIG instead of killing it. Its exit status is not judged (the
    /// snapshot itself may not fit).
    fn run_limited(&self, tag: &str, extra: &[&OsStr]) -> Output {
        self.command(OsStr::new("sh"))
            .arg("-c")
            .arg(r#"trap "" XFSZ; exec prlimit --fsize="$0" -- "$@""#)
            .arg(LIMIT.to_string())
            .arg(env!("CARGO_BIN_EXE_touchbar-customizer"))
            .args(self.args(tag, extra))
            .output()
            .unwrap()
    }
}

fn save_script() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("scripts/drag-f3-before-f1.json")
}

/// `--script` arguments of the flow that drags F3 before F1 and saves.
fn script_args(script: &Path) -> Vec<&OsStr> {
    vec!["--script".as_ref(), script.as_os_str(), "--at".as_ref(), "mark:end".as_ref()]
}

fn mode(path: &Path) -> u32 {
    fs::symlink_metadata(path).unwrap().permissions().mode() & 0o7777
}

fn generations(store: &Path) -> Vec<PathBuf> {
    let Ok(entries) = fs::read_dir(store) else { return Vec::new() };
    let mut dirs: Vec<PathBuf> = entries
        .map(|e| e.unwrap().path())
        .filter(|p| !p.file_name().unwrap().to_string_lossy().starts_with('.'))
        .collect();
    dirs.sort();
    dirs
}

fn manifest_value(generation: &Path, key: &str) -> Option<String> {
    let text = fs::read_to_string(generation.join("manifest.toml")).ok()?;
    text.lines().find_map(|line| {
        let (k, v) = line.split_once('=')?;
        (k.trim().trim_matches('"') == key).then(|| v.trim().trim_matches(['"', '\'']).to_owned())
    })
}

/// A TOML integer as written in a manifest (`416` or `0o640`).
fn manifest_int(generation: &Path, key: &str) -> Option<u32> {
    let v = manifest_value(generation, key)?.replace('_', "");
    match v.strip_prefix("0o") {
        Some(octal) => u32::from_str_radix(octal, 8).ok(),
        None => v.parse().ok(),
    }
}

/// The default config as a person leaves it after editing by hand, padded with `padding` bytes of
/// comments: byte equality then proves the exact file was kept.
fn hand_written(padding: usize) -> Vec<u8> {
    let mut text = format!(
        "# my touch bar, edited by hand\n\n{}\n# keep this note\n",
        to_toml(&Config::default()).unwrap()
    );
    while text.len() < padding {
        text.push_str("# a long personal note that only a byte-exact copy keeps .........................\n");
    }
    text.into_bytes()
}

/// The one generation in the store that kept a file, with its exact bytes.
fn the_kept_file(store: &Path) -> (PathBuf, Vec<u8>) {
    let gens = generations(store);
    assert_eq!(gens.len(), 1, "one generation in {}: {gens:?}", store.display());
    let generation = gens[0].clone();
    assert_eq!(manifest_value(&generation, "kind").as_deref(), Some("file"));
    let data = generation.join("config.toml");
    assert!(fs::symlink_metadata(&data).unwrap().is_file());
    assert_eq!(mode(&data), 0o600, "the kept copy is private");
    let bytes = fs::read(&data).unwrap();
    (generation, bytes)
}

#[test]
fn the_first_run_keeps_an_absent_generation_before_writing_the_default() {
    let p = Profile::new("first_run");

    p.run("first", &[]);

    assert!(p.config().is_file());
    assert_eq!(mode(&p.config()), 0o600, "a new config is private");
    let gens = generations(&p.store());
    assert_eq!(gens.len(), 1, "{gens:?}");
    assert_eq!(manifest_value(&gens[0], "kind").as_deref(), Some("absent"));
    assert_eq!(manifest_value(&gens[0], "dir-existed").as_deref(), Some("false"));
    assert_eq!(mode(&p.store()), 0o700);
    assert!(!p.home.join(".local/state").join(STORE).exists(), "the store follows XDG_STATE_HOME");
}

#[test]
fn a_save_from_the_editor_keeps_the_exact_hand_written_file_and_reopening_keeps_nothing_new() {
    let p = Profile::new("save_then_reopen");
    let mine = hand_written(0);
    p.seed(&mine, 0o640);
    let script = save_script();

    p.run("save", &script_args(&script));

    let saved = fs::read(p.config()).unwrap();
    assert_ne!(saved, mine, "the flow saved a new layout");
    assert_eq!(mode(&p.config()), 0o640, "the saved file keeps the hand-written file's bits");
    let (generation, kept) = the_kept_file(&p.store());
    assert_eq!(kept, mine, "the generation holds the hand-written bytes, comments included");
    assert_eq!(manifest_int(&generation, "mode"), Some(0o640));
    assert_eq!(manifest_value(&generation, "reason").as_deref(), Some("save"));

    p.run("reopen", &[]);

    assert_eq!(fs::read(p.config()).unwrap(), saved, "reopening leaves the saved file");
    assert_eq!(generations(&p.store()), vec![generation], "reopening keeps nothing new");
}

#[test]
fn with_room_for_the_copy_a_size_limited_save_goes_through() {
    let p = Profile::new("limit_room");
    let mine = hand_written(0);
    assert!(mine.len() < LIMIT / 4, "the control file fits easily");
    p.seed(&mine, 0o600);
    let script = save_script();

    let out = p.run_limited("save", &script_args(&script));

    let stderr = String::from_utf8_lossy(&out.stderr);
    assert_ne!(fs::read(p.config()).unwrap(), mine, "the save ran under the limit; stderr: {stderr}");
    assert_eq!(the_kept_file(&p.store()).1, mine);
}

#[test]
fn when_the_copy_cannot_be_written_the_save_is_abandoned_and_the_file_stays() {
    let p = Profile::new("limit_no_room");
    let mine = hand_written(4 * LIMIT);
    p.seed(&mine, 0o640);
    let script = save_script();

    let out = p.run_limited("save", &script_args(&script));

    let stderr = String::from_utf8_lossy(&out.stderr);
    assert_eq!(fs::read(p.config()).unwrap(), mine, "the hand-written file is untouched; stderr: {stderr}");
    assert_eq!(mode(&p.config()), 0o640);
    for generation in generations(&p.store()) {
        assert_ne!(
            manifest_value(&generation, "kind").as_deref(),
            Some("file"),
            "no partial copy listed: {generation:?}"
        );
    }
}
