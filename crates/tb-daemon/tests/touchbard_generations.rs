//! The real `touchbard` keeps a generation before its first write of a profile, in the store of
//! that profile only: `$XDG_STATE_HOME/touchbar-customizer/backups/` when it is absolute, else
//! `$HOME/.local/state/touchbar-customizer/backups/`. The generation is complete and readable by
//! a new process (this test), survives a restart untouched, and two daemons starting at once on
//! one profile keep exactly one. An existing hand-written profile is drawn and left as it was,
//! with nothing kept (reads keep nothing). Every run has its own temp `HOME` and
//! `XDG_STATE_HOME` under cargo's temp dir (or `XDG_STATE_HOME` removed), never the real ones.
//!
//! The store is read straight from disk, in the layout ADR-0014 fixes: one directory per
//! generation holding `manifest.toml` (`kind = "absent" | "file"`, `dir-existed`, `mode`, ...)
//! and, for a kept file, `config.toml`.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::fs::{self, File};
use std::io::Write;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::thread;
use std::time::{Duration, Instant};

use tb_config::default_path;
use tb_core::config::{Config, to_toml};

const WAIT: Duration = Duration::from_secs(60);
const STORE: &str = "touchbar-customizer/backups";

/// How the run sets `XDG_STATE_HOME`.
#[derive(Clone, Copy)]
enum Xdg<'a> {
    At(&'a Path),
    Unset,
    Relative(&'a str),
}

struct Profile {
    dir: PathBuf,
    home: PathBuf,
    state: PathBuf,
}

impl Profile {
    fn new(test: &str) -> Self {
        let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join("touchbard_generations").join(test);
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

    fn xdg_store(&self) -> PathBuf {
        self.state.join(STORE)
    }

    fn home_store(&self) -> PathBuf {
        self.home.join(".local/state").join(STORE)
    }

    /// Starts `touchbard --sim` on this profile; one tap on stdin, then stdin closes and the
    /// daemon ends.
    fn start(&self, xdg: Xdg<'_>, tag: &str) -> Child {
        let sim = self.dir.join(format!("sim-{tag}"));
        let mut command = Command::new(env!("CARGO_BIN_EXE_touchbard"));
        command
            .arg("--sim")
            .arg(&sim)
            .arg("--emit")
            .arg(self.dir.join(format!("keys-{tag}.bin")))
            .env("HOME", &self.home)
            .current_dir(&self.dir)
            .stdin(Stdio::piped())
            .stdout(Stdio::null())
            .stderr(File::create(self.dir.join(format!("stderr-{tag}"))).unwrap());
        match xdg {
            Xdg::At(path) => command.env("XDG_STATE_HOME", path),
            Xdg::Unset => command.env_remove("XDG_STATE_HOME"),
            Xdg::Relative(rel) => command.env("XDG_STATE_HOME", rel),
        };
        let mut child = command.spawn().unwrap();
        child.stdin.take().unwrap().write_all(b"down 100\nup\n").unwrap();
        child
    }

    fn run(&self, xdg: Xdg<'_>, tag: &str) {
        finish(self.start(xdg, tag), &self.dir.join(format!("stderr-{tag}")));
    }
}

fn finish(mut child: Child, stderr: &Path) {
    let deadline = Instant::now() + WAIT;
    while child.try_wait().unwrap().is_none() {
        if Instant::now() > deadline {
            let _ = child.kill();
            panic!("touchbard did not end; stderr: {}", fs::read_to_string(stderr).unwrap_or_default());
        }
        thread::sleep(Duration::from_millis(20));
    }
}

fn mode(path: &Path) -> u32 {
    fs::symlink_metadata(path).unwrap().permissions().mode() & 0o7777
}

/// The generation directories in `store` (names not starting with a dot), sorted.
fn generations(store: &Path) -> Vec<PathBuf> {
    let Ok(entries) = fs::read_dir(store) else { return Vec::new() };
    let mut dirs: Vec<PathBuf> = entries
        .map(|e| e.unwrap().path())
        .filter(|p| !p.file_name().unwrap().to_string_lossy().starts_with('.'))
        .collect();
    dirs.sort();
    dirs
}

/// The value of `key` in a generation's `manifest.toml`, quotes removed.
fn manifest_value(generation: &Path, key: &str) -> Option<String> {
    let text = fs::read_to_string(generation.join("manifest.toml")).ok()?;
    text.lines().find_map(|line| {
        let (k, v) = line.split_once('=')?;
        (k.trim().trim_matches('"') == key).then(|| v.trim().trim_matches(['"', '\'']).to_owned())
    })
}

/// Names inside one directory, sorted.
fn names_in(dir: &Path) -> Vec<String> {
    let mut names: Vec<String> =
        fs::read_dir(dir).unwrap().map(|e| e.unwrap().file_name().to_string_lossy().into_owned()).collect();
    names.sort();
    names
}

/// Bytes, mode and names of everything under `root`, for "left as it was" checks.
fn tree(root: &Path) -> Vec<(PathBuf, u32, Option<Vec<u8>>)> {
    let mut out = Vec::new();
    let mut stack = vec![root.to_path_buf()];
    while let Some(path) = stack.pop() {
        let Ok(meta) = fs::symlink_metadata(&path) else { continue };
        let bytes = meta.is_file().then(|| fs::read(&path).unwrap());
        if meta.is_dir() {
            stack.extend(fs::read_dir(&path).unwrap().map(|e| e.unwrap().path()));
        }
        out.push((path, meta.permissions().mode() & 0o7777, bytes));
    }
    out.sort();
    out
}

/// The one generation in `store`, checked to be a complete, private record of "no file here".
fn the_absent_generation(store: &Path, dir_existed: bool) -> PathBuf {
    let gens = generations(store);
    assert_eq!(gens.len(), 1, "one generation in {}: {gens:?}", store.display());
    let generation = gens[0].clone();
    assert_eq!(mode(store), 0o700, "the store is private");
    assert_eq!(mode(&generation), 0o700, "the generation is private");
    assert_eq!(names_in(&generation), vec!["manifest.toml"], "an absence keeps no data");
    assert_eq!(mode(&generation.join("manifest.toml")), 0o600);
    assert_eq!(manifest_value(&generation, "kind").as_deref(), Some("absent"));
    assert_eq!(
        manifest_value(&generation, "dir-existed").as_deref(),
        Some(if dir_existed { "true" } else { "false" })
    );
    generation
}

#[test]
fn the_first_run_keeps_an_absent_generation_under_an_absolute_xdg_state_home() {
    let p = Profile::new("first_run_xdg");

    p.run(Xdg::At(&p.state), "first");

    assert!(p.config().is_file(), "the daemon wrote the default config");
    assert_eq!(mode(&p.config()), 0o600, "a new config is private");
    the_absent_generation(&p.xdg_store(), false);
    assert!(!p.home_store().exists(), "nothing kept under HOME when XDG_STATE_HOME is set");
}

#[test]
fn the_first_run_into_an_existing_empty_config_directory_records_that_it_existed() {
    let p = Profile::new("first_run_dir_existed");
    fs::create_dir_all(p.config().parent().unwrap()).unwrap();

    p.run(Xdg::At(&p.state), "first");

    the_absent_generation(&p.xdg_store(), true);
}

#[test]
fn without_an_absolute_xdg_state_home_the_store_is_under_home() {
    let unset = Profile::new("xdg_unset");
    unset.run(Xdg::Unset, "first");
    the_absent_generation(&unset.home_store(), false);
    assert_eq!(names_in(&unset.state), Vec::<String>::new());

    let relative = Profile::new("xdg_relative");
    relative.run(Xdg::Relative("relative-state"), "first");
    the_absent_generation(&relative.home_store(), false);
    assert!(!relative.dir.join("relative-state").exists(), "a relative XDG_STATE_HOME is ignored");
}

#[test]
fn the_first_generation_survives_a_restart_untouched() {
    let p = Profile::new("restart");
    p.run(Xdg::At(&p.state), "first");
    let generation = the_absent_generation(&p.xdg_store(), false);
    let before = tree(&p.xdg_store());

    p.run(Xdg::At(&p.state), "second");

    assert_eq!(generations(&p.xdg_store()), vec![generation]);
    assert_eq!(tree(&p.xdg_store()), before, "a restart that only reads keeps nothing new");
}

#[test]
fn two_daemons_starting_at_once_on_one_profile_keep_exactly_one_generation() {
    let p = Profile::new("simultaneous");

    let a = p.start(Xdg::At(&p.state), "a");
    let b = p.start(Xdg::At(&p.state), "b");
    finish(a, &p.dir.join("stderr-a"));
    finish(b, &p.dir.join("stderr-b"));

    assert!(p.config().is_file());
    the_absent_generation(&p.xdg_store(), false);
}

#[test]
fn an_existing_hand_written_profile_is_drawn_and_left_as_it_was() {
    let p = Profile::new("existing_profile");
    let mine = format!("# my bar\n\n{}\n# keep\n", to_toml(&Config::default()).unwrap());
    fs::create_dir_all(p.config().parent().unwrap()).unwrap();
    fs::write(p.config(), &mine).unwrap();
    fs::set_permissions(p.config(), fs::Permissions::from_mode(0o640)).unwrap();

    p.run(Xdg::At(&p.state), "first");

    assert_eq!(fs::read_to_string(p.config()).unwrap(), mine);
    assert_eq!(mode(&p.config()), 0o640);
    assert_eq!(generations(&p.xdg_store()), Vec::<PathBuf>::new(), "reading keeps nothing");
    let frames = fs::read_dir(p.dir.join("sim-first")).map_or(0, Iterator::count);
    let stderr = fs::read_to_string(p.dir.join("stderr-first")).unwrap_or_default();
    assert!(frames > 0, "the profile is drawn; stderr: {stderr}");
}
