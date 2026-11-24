//! Shared helpers for the generation contract tests: a temp profile per test under cargo's temp
//! dir (never `$HOME`, never the real state dir), a private generation store inside it, configs a
//! person would write by hand, and snapshots of files and directories that compare type, mode and
//! bytes without following symlinks.
//!
//! The store layout these tests read is the one ADR-0014 fixes: `<store>/<id>/manifest.toml` and,
//! for a kept file, `<store>/<id>/config.toml` holding the exact bytes it replaced.
#![allow(dead_code)]

use std::collections::BTreeMap;
use std::fs::{self, Permissions};
use std::os::unix::fs::{MetadataExt, PermissionsExt};
use std::path::{Path, PathBuf};

use tb_config::{Backups, Generation, GenerationKind, Health};
use tb_core::config::{Config, to_toml};
use tb_core::{BarTheme, Chord};

/// One test's throwaway profile: `<dir>/home/.config/touchbar-customizer/config.toml` and the
/// store `<dir>/state/touchbar-customizer/backups` (what `XDG_STATE_HOME=<dir>/state` names).
pub struct Profile {
    pub dir: PathBuf,
    pub config: PathBuf,
    pub store: PathBuf,
}

impl Profile {
    pub fn new(suite: &str, test: &str) -> Self {
        let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join(suite).join(test);
        if dir.exists() {
            make_writable(&dir);
            fs::remove_dir_all(&dir).unwrap();
        }
        fs::create_dir_all(&dir).unwrap();
        let config = dir.join("home/.config/touchbar-customizer/config.toml");
        let store = dir.join("state/touchbar-customizer/backups");
        Self { dir, config, store }
    }

    /// A second config path in the same profile (another config sharing the store).
    pub fn other_config(&self, name: &str) -> PathBuf {
        self.dir.join(name).join(".config/touchbar-customizer/config.toml")
    }

    pub fn backups(&self) -> Backups {
        Backups::at(&self.store)
    }

    /// Every generation of `config` in the store, newest first, read by a fresh `Backups` (as a
    /// new process would).
    pub fn generations_of(&self, config: &Path) -> Vec<Generation> {
        Backups::at(&self.store).list(config).unwrap()
    }

    pub fn generations(&self) -> Vec<Generation> {
        self.generations_of(&self.config)
    }

    /// Writes the config file as a person would (directory created), with `mode`.
    pub fn seed(&self, bytes: &[u8], mode: u32) {
        seed(&self.config, bytes, mode);
    }
}

pub fn seed(path: &Path, bytes: &[u8], mode: u32) {
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, bytes).unwrap();
    fs::set_permissions(path, Permissions::from_mode(mode)).unwrap();
}

/// Restores owner rwx on every directory under `dir` so a test that made one read-only can be
/// cleaned up on the next run.
pub fn make_writable(dir: &Path) {
    let Ok(meta) = fs::symlink_metadata(dir) else { return };
    if meta.is_dir() {
        let _ = fs::set_permissions(dir, Permissions::from_mode(0o700));
        if let Ok(entries) = fs::read_dir(dir) {
            for entry in entries.flatten() {
                make_writable(&entry.path());
            }
        }
    }
}

pub fn theme() -> BarTheme {
    BarTheme::omarchy_fallback()
}

/// The default config with the first F-key button of the default layer given another label.
pub fn relabelled(label: &str) -> Config {
    let mut config = Config::default();
    let layer = config.layout.layer_mut(Chord::Default).unwrap();
    label.clone_into(&mut layer.groups[1].buttons[0].label);
    config
}

/// `config` as a person leaves it after editing by hand: comments and blank lines a
/// re-serialisation would drop, so byte equality proves the exact file was kept.
pub fn hand_written(config: &Config) -> Vec<u8> {
    format!("# my touch bar, edited by hand\n\n{}\n# keep this note\n", to_toml(config).unwrap()).into_bytes()
}

/// Permission bits of `path` itself (never a symlink's target).
pub fn mode(path: &Path) -> u32 {
    fs::symlink_metadata(path).unwrap().permissions().mode() & 0o7777
}

/// The directory of one generation in the store.
pub fn generation_dir(store: &Path, generation: &Generation) -> PathBuf {
    store.join(generation.id.to_string())
}

/// The kept bytes of a `file` generation, read straight from the store.
pub fn kept_bytes(store: &Path, generation: &Generation) -> Vec<u8> {
    let data = generation_dir(store, generation).join("config.toml");
    let meta = fs::symlink_metadata(&data).unwrap();
    assert!(meta.file_type().is_file(), "{} is a regular file", data.display());
    fs::read(data).unwrap()
}

pub fn is_verified(generation: &Generation) -> bool {
    matches!(generation.health, Health::Verified)
}

pub fn is_file_kind(generation: &Generation) -> bool {
    matches!(generation.kind, GenerationKind::File)
}

/// `dir_existed` of an `absent` generation; panics on a `file` one.
pub fn absent_dir_existed(generation: &Generation) -> bool {
    match generation.kind {
        GenerationKind::Absent { dir_existed } => dir_existed,
        GenerationKind::File => panic!("{generation:?} is a kept file, not an absence"),
    }
}

/// Type, permission bits, bytes (files), link text (symlinks) and inode of one path, never
/// following a final symlink.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Entry {
    File { mode: u32, bytes: Vec<u8>, ino: u64 },
    Dir { mode: u32 },
    Link { to: PathBuf },
    Other { mode: u32 },
}

pub fn entry(path: &Path) -> Option<Entry> {
    let meta = fs::symlink_metadata(path).ok()?;
    let ft = meta.file_type();
    let mode = meta.permissions().mode() & 0o7777;
    Some(if ft.is_symlink() {
        Entry::Link { to: fs::read_link(path).unwrap() }
    } else if ft.is_dir() {
        Entry::Dir { mode }
    } else if ft.is_file() {
        Entry::File { mode, bytes: fs::read(path).unwrap(), ino: meta.ino() }
    } else {
        Entry::Other { mode }
    })
}

/// `entry` of `root` and of every path under it (symlinks are not followed).
pub fn snapshot(root: &Path) -> BTreeMap<PathBuf, Entry> {
    let mut out = BTreeMap::new();
    let mut stack = vec![root.to_path_buf()];
    while let Some(path) = stack.pop() {
        let Some(e) = entry(&path) else { continue };
        if matches!(e, Entry::Dir { .. })
            && let Ok(entries) = fs::read_dir(&path)
        {
            stack.extend(entries.flatten().map(|d| d.path()));
        }
        out.insert(path, e);
    }
    out
}

/// Names in `dir` (empty when it does not exist), sorted.
pub fn names_in(dir: &Path) -> Vec<String> {
    let Ok(entries) = fs::read_dir(dir) else { return Vec::new() };
    let mut names: Vec<String> =
        entries.map(|e| e.unwrap().file_name().to_string_lossy().into_owned()).collect();
    names.sort();
    names
}

/// An editor outside the library replacing the file atomically (temp + rename, no lock).
pub fn external_replace(path: &Path, bytes: &[u8]) {
    let staged = path.with_file_name(".config.toml.editor-tmp");
    fs::write(&staged, bytes).unwrap();
    fs::rename(staged, path).unwrap();
}

/// True when this process runs as root (permission refusals cannot be injected then).
pub fn running_as_root(probe_dir: &Path) -> bool {
    fs::metadata(probe_dir).unwrap().uid() == 0
}
