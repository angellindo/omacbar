//! A save writes only over the exact file version its `ConfigFile` last read or wrote. A stale save
//! (another config file saved, an external editor renamed or rewrote the file, the file or a
//! symlink's target was removed, or the `ConfigFile` never read the path) is refused and leaves the
//! file, its mode, its inode and any symlink alone. Simultaneous saves of one version have exactly
//! one winner. A conflict-free save is still exactly one change for the daemon's watch, and a
//! refused one is none. Only API that exists before compare-and-swap saves is used here; the
//! `Changed` variant itself is pinned in `config_conflict_overwrite.rs`. Each test owns one temp
//! directory; positive waits are long (loaded laptop), negative waits short.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::fs::{self, OpenOptions, Permissions};
use std::io::Write;
use std::os::unix::fs::{MetadataExt, PermissionsExt, symlink};
use std::path::{Path, PathBuf};
use std::sync::Barrier;
use std::sync::mpsc::{Receiver, RecvTimeoutError, channel};
use std::thread;
use std::time::Duration;

use tb_config::{ConfigFile, ConfigFileError, ConfigWatch, Loaded};
use tb_core::config::{Config, parse, to_toml};
use tb_core::{BarTheme, Chord};

const CHANGE: Duration = Duration::from_secs(10);
const QUIET: Duration = Duration::from_millis(500);

/// An empty directory for one test, under cargo's temp dir (never `$HOME`).
fn fresh_dir(test: &str) -> PathBuf {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join("config_stale_save").join(test);
    if dir.exists() {
        fs::remove_dir_all(&dir).unwrap();
    }
    fs::create_dir_all(&dir).unwrap();
    dir
}

fn theme() -> BarTheme {
    BarTheme::omarchy_fallback()
}

fn names_in(dir: &Path) -> Vec<String> {
    let mut names: Vec<String> =
        fs::read_dir(dir).unwrap().map(|e| e.unwrap().file_name().to_string_lossy().into_owned()).collect();
    names.sort();
    names
}

/// The inode of `path` itself (a symlink's own inode, not its target's).
fn ino(path: &Path) -> u64 {
    fs::symlink_metadata(path).unwrap().ino()
}

fn mode(path: &Path) -> u32 {
    fs::metadata(path).unwrap().permissions().mode() & 0o7777
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

/// `config` as a person would leave it after editing by hand: comments and blank lines that a
/// re-serialisation would drop, so byte equality proves nothing rewrote the file.
fn hand_written(config: &Config) -> String {
    format!("# edited by hand\n\n{}\n# keep this note\n", to_toml(config).unwrap())
}

/// A config file that just read the valid file at `path`.
fn loaded_from(path: &Path) -> ConfigFile {
    let mut file = ConfigFile::new(path);
    assert_eq!(file.load(&theme()).unwrap(), Loaded::Read, "{} is a valid config", path.display());
    file
}

/// An editor outside the library replacing the file atomically (plain temp + rename, no lock).
fn external_rename(path: &Path, text: &str) {
    let staged = path.with_file_name(".external-editor.tmp");
    fs::write(&staged, text).unwrap();
    fs::rename(&staged, path).unwrap();
}

/// A refusal because the file is no longer the version this `ConfigFile` read: an error, and none
/// of the failure kinds that existed before compare-and-swap saves (I/O, invalid config, ...).
fn refused_as_changed<T>(result: &Result<T, ConfigFileError>) -> bool {
    matches!(
        result,
        Err(e) if !matches!(
            e,
            ConfigFileError::Io { .. }
                | ConfigFileError::Invalid(_)
                | ConfigFileError::Serialize(_)
                | ConfigFileError::Theme(_)
                | ConfigFileError::WatchEnded { .. }
        )
    )
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
fn a_stale_save_after_another_config_files_save_is_refused_and_keeps_the_newer_bytes() {
    let dir = fresh_dir("stale_after_library_save");
    let path = dir.join("config.toml");
    fs::write(&path, to_toml(&Config::default()).unwrap()).unwrap();
    let mut a = loaded_from(&path);
    let mut b = loaded_from(&path);
    a.save(relabelled("f5", "from a"), &theme()).unwrap();
    let newer = fs::read(&path).unwrap();
    let inode = ino(&path);

    let result = b.save(relabelled("f6", "from b"), &theme());

    assert!(refused_as_changed(&result), "a stale save must be refused as changed, got {result:?}");
    assert_eq!(fs::read(&path).unwrap(), newer, "the newer file is kept byte for byte");
    assert_eq!(ino(&path), inode, "nothing is renamed over the newer file");
    assert_eq!(b.config(), &Config::default(), "a refused save leaves the current config");
    assert_eq!(names_in(&dir), vec!["config.toml".to_owned()], "no temp or backup file is left");
}

#[test]
fn a_refused_stale_save_stays_refused_until_the_file_is_read_again() {
    let dir = fresh_dir("refused_until_read");
    let path = dir.join("config.toml");
    fs::write(&path, to_toml(&Config::default()).unwrap()).unwrap();
    let mut a = loaded_from(&path);
    let mut b = loaded_from(&path);
    let theirs = relabelled("f5", "from a");
    let mine = relabelled("f6", "from b");
    a.save(theirs.clone(), &theme()).unwrap();
    let newer = fs::read(&path).unwrap();

    let first = b.save(mine.clone(), &theme());
    let again = b.save(mine.clone(), &theme());

    assert!(refused_as_changed(&first), "{first:?}");
    assert!(refused_as_changed(&again), "refusing never adopts the newer version silently: {again:?}");
    assert_eq!(fs::read(&path).unwrap(), newer);
    assert_eq!(b.load(&theme()).unwrap(), Loaded::Read);
    assert_eq!(b.config(), &theirs, "reading again shows the newer file");
    b.save(mine.clone(), &theme()).unwrap();
    assert_eq!(parse(&fs::read_to_string(&path).unwrap()).unwrap(), mine);
}

#[test]
fn a_stale_save_that_resolves_the_selected_theme_is_refused_too() {
    let dir = fresh_dir("stale_save_resolved");
    let path = dir.join("config.toml");
    fs::write(&path, to_toml(&Config::default()).unwrap()).unwrap();
    let mut a = loaded_from(&path);
    let mut b = loaded_from(&path);
    a.save(relabelled("f5", "from a"), &theme()).unwrap();
    let newer = fs::read(&path).unwrap();

    let result = b.save_resolved(relabelled("f6", "from b"), &theme());

    assert!(refused_as_changed(&result), "the configurator's save path is compare-and-swap too: {result:?}");
    assert_eq!(fs::read(&path).unwrap(), newer);
    assert_eq!(b.config(), &Config::default());
}

#[test]
fn a_stale_save_over_an_external_rename_keeps_the_hand_written_bytes_mode_and_inode() {
    let dir = fresh_dir("stale_after_external_rename");
    let path = dir.join("config.toml");
    fs::write(&path, to_toml(&Config::default()).unwrap()).unwrap();
    let mut b = loaded_from(&path);
    let newer = hand_written(&relabelled("f5", "by hand"));
    external_rename(&path, &newer);
    fs::set_permissions(&path, Permissions::from_mode(0o600)).unwrap();
    let inode = ino(&path);

    let result = b.save(relabelled("f6", "stale"), &theme());

    assert!(refused_as_changed(&result), "{result:?}");
    assert_eq!(fs::read_to_string(&path).unwrap(), newer, "comments and formatting are kept");
    assert_eq!(mode(&path), 0o600, "the file mode is unchanged");
    assert_eq!(ino(&path), inode, "the file is not replaced");
    assert!(fs::symlink_metadata(&path).unwrap().file_type().is_file());
    assert_eq!(names_in(&dir), vec!["config.toml".to_owned()]);
}

#[test]
fn an_in_place_edit_of_equal_length_and_restored_mtime_is_still_a_newer_version() {
    let dir = fresh_dir("same_size_same_mtime");
    let path = dir.join("config.toml");
    let original = to_toml(&relabelled("f5", "aaaa")).unwrap();
    fs::write(&path, &original).unwrap();
    let mut b = loaded_from(&path);
    let modified = fs::metadata(&path).unwrap().modified().unwrap();
    let edited = to_toml(&relabelled("f5", "bbbb")).unwrap();
    assert_eq!(edited.len(), original.len(), "the edit keeps the size");
    let mut in_place = OpenOptions::new().write(true).open(&path).unwrap();
    in_place.write_all(edited.as_bytes()).unwrap();
    in_place.set_modified(modified).unwrap();
    drop(in_place);

    let result = b.save(relabelled("f6", "stale"), &theme());

    assert!(refused_as_changed(&result), "the version is the content, not size or time: {result:?}");
    assert_eq!(fs::read_to_string(&path).unwrap(), edited);
}

#[test]
fn rewriting_identical_bytes_is_not_a_newer_version() {
    let dir = fresh_dir("identical_rewrite");
    let path = dir.join("config.toml");
    let original = hand_written(&Config::default());
    fs::write(&path, &original).unwrap();
    let mut b = loaded_from(&path);
    external_rename(&path, &original);
    let mine = relabelled("f6", "mine");

    b.save(mine.clone(), &theme()).unwrap();

    assert_eq!(parse(&fs::read_to_string(&path).unwrap()).unwrap(), mine);
    assert_eq!(b.config(), &mine);
}

#[test]
fn a_config_file_that_never_read_the_path_cannot_replace_an_existing_file() {
    let dir = fresh_dir("never_read");
    let path = dir.join("config.toml");
    let existing = hand_written(&relabelled("f5", "theirs"));
    fs::write(&path, &existing).unwrap();
    let mut fresh = ConfigFile::new(&path);

    let result = fresh.save(relabelled("f6", "blind"), &theme());

    assert!(refused_as_changed(&result), "a blind overwrite is refused: {result:?}");
    assert_eq!(fs::read_to_string(&path).unwrap(), existing);
    assert_eq!(fresh.config(), &Config::default());
    assert_eq!(names_in(&dir), vec!["config.toml".to_owned()]);
}

#[test]
fn a_save_after_a_rejected_or_invalid_read_replaces_the_file_that_was_read() {
    let mine = relabelled("f6", "replacement");
    let by_load = fresh_dir("after_rejected_load").join("config.toml");
    fs::write(&by_load, "not valid TOML").unwrap();
    let mut file = ConfigFile::new(&by_load);
    assert!(matches!(file.load(&theme()).unwrap(), Loaded::Rejected(_)));
    file.save(mine.clone(), &theme()).unwrap();
    assert_eq!(parse(&fs::read_to_string(&by_load).unwrap()).unwrap(), mine);

    let by_resolved = fresh_dir("after_invalid_load_resolved").join("config.toml");
    fs::write(&by_resolved, "not valid TOML").unwrap();
    let mut file = ConfigFile::new(&by_resolved);
    assert!(matches!(file.load_resolved(&theme()), Err(ConfigFileError::Invalid(_))));
    file.save(mine.clone(), &theme()).unwrap();
    assert_eq!(parse(&fs::read_to_string(&by_resolved).unwrap()).unwrap(), mine);
}

#[test]
fn saving_again_through_one_config_file_is_never_a_conflict_with_itself() {
    let dir = fresh_dir("own_saves");
    let path = dir.join("config.toml");
    let mut file = ConfigFile::new(&path);
    assert_eq!(file.load(&theme()).unwrap(), Loaded::WroteDefault);

    for label in ["one", "two", "three"] {
        file.save(relabelled("f5", label), &theme()).unwrap();
    }

    assert_eq!(parse(&fs::read_to_string(&path).unwrap()).unwrap(), relabelled("f5", "three"));
    assert_eq!(names_in(&dir), vec!["config.toml".to_owned()], "an ordinary save makes no backup");
}

#[test]
fn a_stale_save_after_the_file_was_removed_is_refused_and_creates_nothing() {
    let dir = fresh_dir("stale_after_removal");
    let path = dir.join("config.toml");
    fs::write(&path, to_toml(&Config::default()).unwrap()).unwrap();
    let mut b = loaded_from(&path);
    fs::remove_file(&path).unwrap();

    let result = b.save(relabelled("f6", "stale"), &theme());

    assert!(refused_as_changed(&result), "{result:?}");
    assert!(fs::symlink_metadata(&path).is_err(), "nothing is written in place of a removed file");
    assert_eq!(names_in(&dir), Vec::<String>::new(), "nothing is created");
}

#[test]
fn a_stale_save_through_a_symlink_leaves_the_link_and_its_target_untouched() {
    let dir = fresh_dir("stale_through_symlink");
    let real = dir.join("dotfiles");
    fs::create_dir(&real).unwrap();
    let target = real.join("config.toml");
    fs::write(&target, to_toml(&Config::default()).unwrap()).unwrap();
    let link = dir.join("config.toml");
    symlink(&target, &link).unwrap();
    let mut b = loaded_from(&link);
    let newer = hand_written(&relabelled("f5", "through the link"));
    fs::write(&target, &newer).unwrap();

    let result = b.save(relabelled("f6", "stale"), &theme());

    assert!(refused_as_changed(&result), "{result:?}");
    assert!(fs::symlink_metadata(&link).unwrap().file_type().is_symlink(), "the link stays a link");
    assert_eq!(fs::read_link(&link).unwrap(), target);
    assert_eq!(fs::read_to_string(&target).unwrap(), newer);
    assert_eq!(names_in(&dir), vec!["config.toml".to_owned(), "dotfiles".to_owned()]);
    assert_eq!(names_in(&real), vec!["config.toml".to_owned()]);
}

#[test]
fn a_stale_save_through_a_symlink_whose_target_was_removed_writes_nothing() {
    let dir = fresh_dir("stale_dangling_symlink");
    let real = dir.join("dotfiles");
    fs::create_dir(&real).unwrap();
    let target = real.join("config.toml");
    fs::write(&target, to_toml(&Config::default()).unwrap()).unwrap();
    let link = dir.join("config.toml");
    symlink(&target, &link).unwrap();
    let mut b = loaded_from(&link);
    fs::remove_file(&target).unwrap();

    let result = b.save(relabelled("f6", "stale"), &theme());

    assert!(refused_as_changed(&result), "{result:?}");
    assert!(fs::symlink_metadata(&link).unwrap().file_type().is_symlink(), "the link is not replaced");
    assert_eq!(fs::read_link(&link).unwrap(), target);
    assert!(!target.exists(), "the missing target is not recreated");
    assert_eq!(names_in(&dir), vec!["config.toml".to_owned(), "dotfiles".to_owned()]);
    assert_eq!(names_in(&real), Vec::<String>::new());
}

#[test]
fn simultaneous_saves_from_editors_that_read_the_same_version_have_exactly_one_winner() {
    const WRITERS: usize = 4;
    const ROUNDS: usize = 100;
    let dir = fresh_dir("simultaneous_saves");
    let path = dir.join("config.toml");
    fs::write(&path, to_toml(&Config::default()).unwrap()).unwrap();

    for round in 0..ROUNDS {
        let barrier = Barrier::new(WRITERS);
        let results: Vec<(Config, Result<(), ConfigFileError>)> = thread::scope(|scope| {
            let writers: Vec<_> = (0..WRITERS)
                .map(|writer| {
                    let (path, barrier) = (&path, &barrier);
                    scope.spawn(move || {
                        let mut file = loaded_from(path);
                        let mine = relabelled("f5", &format!("r{round}w{writer}"));
                        barrier.wait();
                        let result = file.save(mine.clone(), &theme());
                        (mine, result)
                    })
                })
                .collect();
            writers.into_iter().map(|w| w.join().unwrap()).collect()
        });

        let winners: Vec<&Config> = results.iter().filter(|(_, r)| r.is_ok()).map(|(c, _)| c).collect();
        assert_eq!(winners.len(), 1, "round {round}: exactly one save of one version wins: {results:?}");
        for (_, result) in &results {
            assert!(result.is_ok() || refused_as_changed(result), "round {round}: {result:?}");
        }
        let written = fs::read_to_string(&path).unwrap();
        assert_eq!(
            written,
            to_toml(winners[0]).unwrap(),
            "round {round}: the file is the winner's whole config"
        );
        assert_eq!(names_in(&dir), vec!["config.toml".to_owned()], "round {round}: no temp file is left");
    }
}

#[test]
fn a_missing_file_default_never_replaces_a_config_another_writer_created() {
    const LOADERS: usize = 3;
    const ROUNDS: usize = 100;
    let created = relabelled("f5", "created");

    for round in 0..ROUNDS {
        let path = fresh_dir(&format!("default_race/{round}")).join("config.toml");
        let barrier = Barrier::new(LOADERS + 1);
        let (creator, loads) = thread::scope(|scope| {
            let (path, barrier, created) = (&path, &barrier, &created);
            let creator = scope.spawn(move || {
                let mut file = ConfigFile::new(path);
                barrier.wait();
                file.save(created.clone(), &theme())
            });
            let loaders: Vec<_> = (0..LOADERS)
                .map(|_| {
                    scope.spawn(move || {
                        let mut file = ConfigFile::new(path);
                        barrier.wait();
                        file.load(&theme())
                    })
                })
                .collect();
            let loads: Vec<_> = loaders.into_iter().map(|l| l.join().unwrap()).collect();
            (creator.join().unwrap(), loads)
        });

        for load in &loads {
            assert!(load.is_ok(), "round {round}: a load on a missing or new file succeeds: {load:?}");
        }
        let written = fs::read_to_string(&path).unwrap();
        if creator.is_ok() {
            assert_eq!(
                written,
                to_toml(&created).unwrap(),
                "round {round}: a default replaced a saved config"
            );
        } else {
            assert!(refused_as_changed(&creator), "round {round}: {creator:?}");
            assert_eq!(parse(&written).unwrap(), Config::default(), "round {round}");
        }
    }
}

#[test]
fn a_conflict_free_save_is_one_change_the_daemon_reads_and_a_refused_stale_save_is_none() {
    let dir = fresh_dir("daemon_watch");
    let path = dir.join("config.toml");
    fs::write(&path, to_toml(&Config::default()).unwrap()).unwrap();
    let mut daemon = ConfigFile::new(&path);
    daemon.load_resolved(&theme()).unwrap();
    let changes = on_a_thread(daemon.watch().unwrap());
    let mut a = loaded_from(&path);
    let mut b = loaded_from(&path);
    let theirs = relabelled("f5", "from a");

    a.save(theirs.clone(), &theme()).unwrap();

    one_change(&changes);
    no_change(&changes);
    daemon.reload_resolved(&theme()).unwrap();
    assert_eq!(daemon.config(), &theirs);

    let result = b.save(relabelled("f6", "stale"), &theme());

    assert!(refused_as_changed(&result), "{result:?}");
    no_change(&changes);
    daemon.reload_resolved(&theme()).unwrap();
    assert_eq!(daemon.config(), &theirs, "the daemon keeps the newer config");
}
