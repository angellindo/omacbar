//! A watch on the config file reports a change when the file is written in place or renamed into
//! place (as editors and `save` do), and nothing for other files in its directory or for a
//! delete. A change followed by a load gives the new config, or keeps the old one if the file is
//! invalid. Each test owns one temp directory; positive waits are long (loaded laptop), negative
//! waits short.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::mpsc::{Receiver, RecvTimeoutError, channel};
use std::thread;
use std::time::Duration;

use tb_config::{ConfigFile, ConfigFileError, ConfigWatch, Loaded};
use tb_core::config::{Config, to_toml};
use tb_core::{BarTheme, Chord};

const CHANGE: Duration = Duration::from_secs(10);
const QUIET: Duration = Duration::from_millis(500);

/// An empty directory for one test, under cargo's temp dir (never `$HOME`).
fn fresh_dir(test: &str) -> PathBuf {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join("config_watch").join(test);
    if dir.exists() {
        fs::remove_dir_all(&dir).unwrap();
    }
    fs::create_dir_all(&dir).unwrap();
    dir
}

fn theme() -> BarTheme {
    BarTheme::omarchy_fallback()
}

/// The default config with the default-layer button `id` given another label.
fn relabelled(id: &str, label: &str) -> Config {
    let mut config = Config::default();
    let layer = config.layout.layer_mut(Chord::Default).unwrap();
    let button =
        layer.groups.iter_mut().flat_map(|g| g.buttons.iter_mut()).find(|b| b.id.as_str() == id).unwrap();
    label.clone_into(&mut button.label);
    config
}

/// A config file whose default was just written by a load, and the receiver of its changes.
fn watched(dir: &Path) -> (ConfigFile, Receiver<Result<(), ConfigFileError>>) {
    let mut file = ConfigFile::new(dir.join("config.toml"));
    assert_eq!(file.load(&theme()).unwrap(), Loaded::WroteDefault);
    let changes = on_a_thread(file.watch().unwrap());
    (file, changes)
}

/// Blocks on `next_change` on its own thread and sends every result.
fn on_a_thread(mut watch: ConfigWatch) -> Receiver<Result<(), ConfigFileError>> {
    let (tx, rx) = channel();
    thread::spawn(move || {
        loop {
            let change = watch.next_change();
            let ended = change.is_err();
            if tx.send(change).is_err() || ended {
                break;
            }
        }
    });
    rx
}

fn one_change(changes: &Receiver<Result<(), ConfigFileError>>) {
    match changes.recv_timeout(CHANGE) {
        Ok(Ok(())) => {}
        other => panic!("expected a change, got {other:?}"),
    }
}

fn no_change(changes: &Receiver<Result<(), ConfigFileError>>) {
    match changes.recv_timeout(QUIET) {
        Err(RecvTimeoutError::Timeout) => {}
        other => panic!("expected no change, got {other:?}"),
    }
}

#[test]
fn writing_the_config_in_place_is_a_change_and_the_next_load_reads_it() {
    let dir = fresh_dir("in_place_write");
    let (mut file, changes) = watched(&dir);
    let mine = relabelled("f5", "bright");

    fs::write(file.path(), to_toml(&mine).unwrap()).unwrap();

    one_change(&changes);
    assert_eq!(file.load(&theme()).unwrap(), Loaded::Read);
    assert_eq!(file.config(), &mine);
}

#[test]
fn renaming_a_file_onto_the_config_is_one_change() {
    let dir = fresh_dir("rename_onto");
    let (mut file, changes) = watched(&dir);
    let mine = relabelled("f5", "bright");
    let edit = dir.join(".edit.tmp");
    fs::write(&edit, to_toml(&mine).unwrap()).unwrap();

    fs::rename(&edit, file.path()).unwrap();

    one_change(&changes);
    no_change(&changes);
    assert_eq!(file.load(&theme()).unwrap(), Loaded::Read);
    assert_eq!(file.config(), &mine);
}

#[test]
fn a_save_through_another_config_file_is_exactly_one_change() {
    let dir = fresh_dir("save_is_one_change");
    let (_file, changes) = watched(&dir);
    let mut other = ConfigFile::new(dir.join("config.toml"));

    other.save(relabelled("f5", "bright"), &theme()).unwrap();

    one_change(&changes);
    no_change(&changes);
}

#[test]
fn writing_another_file_in_the_directory_is_not_a_change() {
    let dir = fresh_dir("sibling_write");
    let (file, changes) = watched(&dir);

    fs::write(dir.join("other.toml"), "x = 1\n").unwrap();
    no_change(&changes);

    fs::write(file.path(), to_toml(&relabelled("f5", "bright")).unwrap()).unwrap();
    one_change(&changes);
}

#[test]
fn deleting_the_config_is_not_a_change() {
    let dir = fresh_dir("delete");
    let (file, changes) = watched(&dir);

    fs::remove_file(file.path()).unwrap();
    no_change(&changes);

    fs::write(file.path(), to_toml(&relabelled("f5", "bright")).unwrap()).unwrap();
    one_change(&changes);
}

#[test]
fn an_invalid_write_is_a_change_and_the_next_load_keeps_the_old_config() {
    let dir = fresh_dir("invalid_write");
    let (mut file, changes) = watched(&dir);

    fs::write(file.path(), "version = ").unwrap();

    one_change(&changes);
    assert!(matches!(file.load(&theme()).unwrap(), Loaded::Rejected(_)));
    assert_eq!(file.config(), &Config::default());
}
