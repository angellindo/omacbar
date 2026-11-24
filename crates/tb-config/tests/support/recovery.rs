//! Shared helpers for the terminal recovery contract: run the real `touchbar-recover` binary of
//! tb-config in one test's throwaway profile (`HOME=<dir>/home`, `XDG_STATE_HOME=<dir>/state`,
//! no display, no GPU), seed generations through the real protected `ConfigFile`, and read what a
//! generation's manifest records straight from the store (ADR layout: `<store>/<id>/manifest.toml`,
//! `<store>/<id>/config.toml`).
//!
//! The binary is located at run time with `option_env!`: on a tree without the `[[bin]]` every
//! case compiles and then fails on the missing binary, never on a compile error.
#![allow(dead_code)]

use std::collections::BTreeMap;
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};

use tb_config::{ConfigFile, Generation, Loaded};

use super::generations::{Entry, Profile, hand_written, relabelled, snapshot, theme};

/// The `touchbar-recover` binary cargo built for this test target.
pub fn recover_bin() -> &'static str {
    let Some(bin) = option_env!("CARGO_BIN_EXE_touchbar-recover") else {
        panic!(
            "tb-config has no `touchbar-recover` binary target: cargo set no CARGO_BIN_EXE_touchbar-recover"
        )
    };
    bin
}

/// What one run printed and how it ended.
#[derive(Debug)]
pub struct Run {
    pub code: Option<i32>,
    pub stdout: String,
    pub stderr: String,
}

impl Run {
    fn of(output: &Output) -> Self {
        Self {
            code: output.status.code(),
            stdout: String::from_utf8_lossy(&output.stdout).into_owned(),
            stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
        }
    }

    /// Asserts the exit code, showing both streams on a mismatch.
    pub fn exits(&self, code: i32) -> &Self {
        assert_eq!(self.code, Some(code), "expected exit {code}; {self:?}");
        self
    }

    /// Refused or failed (exit 1) or a usage error (exit 2): never success, never a conflict.
    pub fn refused(&self) -> &Self {
        assert!(matches!(self.code, Some(1 | 2)), "expected a refusal (exit 1 or 2); {self:?}");
        assert!(!self.stderr.trim().is_empty(), "a refusal says why on stderr; {self:?}");
        self
    }

    /// The line of `stdout` that lists generation `id` (a line holding `id` as a whole word).
    pub fn line_of(&self, id: &str) -> &str {
        self.stdout
            .lines()
            .find(|line| words(line).any(|w| w == id))
            .unwrap_or_else(|| panic!("no line of `list` names generation {id}; {self:?}"))
    }
}

/// The words of a line: runs of ASCII letters, digits, `-` and `_`.
pub fn words(line: &str) -> impl Iterator<Item = &str> {
    line.split(|c: char| !(c.is_ascii_alphanumeric() || c == '-' || c == '_')).filter(|w| !w.is_empty())
}

/// `program` with the environment a run gets: this profile only, no display, no usable GPU.
fn isolated(program: &str, profile: &Profile) -> Command {
    let mut cmd = Command::new(program);
    cmd.env("HOME", profile.dir.join("home"))
        .env("XDG_STATE_HOME", profile.dir.join("state"))
        .env_remove("DISPLAY")
        .env_remove("WAYLAND_DISPLAY")
        .env("VK_ICD_FILENAMES", profile.dir.join("no-gpu.json"))
        .stdin(Stdio::null());
    cmd
}

fn command(profile: &Profile, args: &[&str]) -> Command {
    let mut cmd = isolated(recover_bin(), profile);
    cmd.args(args);
    cmd
}

/// Runs `touchbar-recover <args>` in `profile` and waits for it.
pub fn recover(profile: &Profile, args: &[&str]) -> Run {
    Run::of(&command(profile, args).output().unwrap())
}

/// Like `recover`, with `XDG_STATE_HOME` set to `xdg` (`None`: removed).
pub fn recover_with_xdg(profile: &Profile, xdg: Option<&str>, args: &[&str]) -> Run {
    let mut cmd = command(profile, args);
    match xdg {
        Some(value) => cmd.env("XDG_STATE_HOME", value),
        None => cmd.env_remove("XDG_STATE_HOME"),
    };
    Run::of(&cmd.output().unwrap())
}

/// Runs `touchbar-recover <args>` with every file it writes capped at `bytes` (soft and hard),
/// `SIGXFSZ` ignored so an oversized write fails with EFBIG instead of killing it.
pub fn recover_size_limited(profile: &Profile, bytes: u64, args: &[&str]) -> Run {
    let script = format!("trap '' XFSZ; exec prlimit --fsize={bytes}:{bytes} -- \"$0\" \"$@\"");
    let mut sh = isolated("sh", profile);
    sh.arg("-c").arg(script).arg(recover_bin()).args(args);
    Run::of(&sh.output().unwrap())
}

/// A protected config file of `profile`, opened (read) as an editor would.
pub fn opened(profile: &Profile) -> ConfigFile {
    let mut file = ConfigFile::protected(&profile.config, profile.backups());
    file.load(&theme()).unwrap();
    file
}

/// Three hand-written versions saved in turn: `a` is seeded (mode `mode`, with comments), then
/// replaced by a save of `b`, then by a save of `c`. The store then holds a `file` generation of
/// `a` and one of `b`; the config file holds `c` with mode `mode`. Returns the three byte strings.
pub struct History {
    pub a: Vec<u8>,
    pub b: Vec<u8>,
    pub c: Vec<u8>,
}

pub fn three_versions(profile: &Profile, mode: u32) -> History {
    let a = hand_written(&relabelled("first"));
    profile.seed(&a, mode);
    let mut file = ConfigFile::protected(&profile.config, profile.backups());
    assert_eq!(file.load(&theme()).unwrap(), Loaded::Read);
    file.save(relabelled("second"), &theme()).unwrap();
    let b = fs::read(&profile.config).unwrap();
    file.save(relabelled("third"), &theme()).unwrap();
    let c = fs::read(&profile.config).unwrap();
    assert_ne!(a, b);
    assert_ne!(b, c);
    History { a, b, c }
}

/// The `file` generation of `profile`'s config holding exactly `bytes`.
pub fn generation_holding(profile: &Profile, bytes: &[u8]) -> Generation {
    profile
        .generations()
        .into_iter()
        .find(|g| {
            matches!(g.kind, tb_config::GenerationKind::File)
                && fs::read(profile.store.join(g.id.to_string()).join("config.toml")).ok().as_deref()
                    == Some(bytes)
        })
        .unwrap_or_else(|| panic!("no generation holds the expected {} bytes", bytes.len()))
}

/// `key = value` pairs of one generation's `manifest.toml`, read from disk (values unquoted).
pub fn manifest(store: &Path, id: &str) -> BTreeMap<String, String> {
    let text = fs::read_to_string(store.join(id).join("manifest.toml")).unwrap();
    text.lines()
        .filter_map(|line| line.split_once('='))
        .map(|(k, v)| (k.trim().to_owned(), v.trim().trim_matches('"').to_owned()))
        .collect()
}

/// Ids (directory names) of every generation in the store whose manifest has `reason = "recover"`.
pub fn recover_generations(store: &Path) -> Vec<String> {
    let Ok(entries) = fs::read_dir(store) else { return Vec::new() };
    let mut ids: Vec<String> = entries
        .flatten()
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .filter(|name| name.parse::<u64>().is_ok())
        .filter(|id| {
            fs::read_to_string(store.join(id).join("manifest.toml"))
                .is_ok_and(|t| t.lines().any(|l| l.replace(' ', "") == "reason=\"recover\""))
        })
        .collect();
    ids.sort_by_key(|id| id.parse::<u64>().unwrap_or(0));
    ids
}

/// The one `recover` generation in the store; panics unless there is exactly one.
pub fn only_recover_generation(store: &Path) -> String {
    let ids = recover_generations(store);
    assert_eq!(ids.len(), 1, "exactly one `recover` generation is kept; store {:?}", snapshot(store).keys());
    ids[0].clone()
}

/// Bytes a `file` generation holds, read straight from the store.
pub fn kept(store: &Path, id: &str) -> Vec<u8> {
    let data = store.join(id).join("config.toml");
    assert!(
        fs::symlink_metadata(&data).unwrap().file_type().is_file(),
        "{} is a regular file",
        data.display()
    );
    fs::read(data).unwrap()
}

/// Permission bits a manifest records (decimal, or `0o` octal), as a number.
pub fn manifest_mode(store: &Path, id: &str) -> u32 {
    let raw = manifest(store, id).remove("mode").unwrap();
    raw.strip_prefix("0o").map_or_else(|| raw.parse().unwrap(), |o| u32::from_str_radix(o, 8).unwrap())
}

/// The config file entry (type, mode, bytes) and the whole store, to prove a run changed nothing.
pub fn state(profile: &Profile) -> (Option<Entry>, BTreeMap<PathBuf, Entry>) {
    (super::generations::entry(&profile.config), snapshot(&profile.store))
}

/// Mode bits of a directory, set.
pub fn chmod(path: &Path, mode: u32) {
    fs::set_permissions(path, fs::Permissions::from_mode(mode)).unwrap();
}
