//! A stale save is refused as `ConfigFileError::Changed`, naming the version now on disk as an
//! opaque `DiskVersion` (absent, or the bytes seen). An acknowledged overwrite (`save_over`) is the
//! same compare-and-swap against that version: it keeps the exact replaced bytes, with their mode,
//! as `config.toml.bak`, never readable more widely than the replaced file; a further change since
//! the acknowledgement is `Changed` again and writes nothing; a backup that was already there (a
//! file or a symlink) is never silently lost or written through, and an overwrite at a symlink either
//! keeps the link and every replaced byte recoverable or is refused without writing. Simultaneous
//! overwrites of one version have one winner, and an overwrite is exactly one change for the
//! daemon's watch. Each test owns one temp directory.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::fs::{self, Permissions};
use std::os::unix::fs::{MetadataExt, PermissionsExt, symlink};
use std::path::{Path, PathBuf};
use std::sync::Barrier;
use std::sync::mpsc::{Receiver, RecvTimeoutError, channel};
use std::thread;
use std::time::Duration;

use tb_config::{ConfigFile, ConfigFileError, ConfigWatch, DiskVersion, Loaded};
use tb_core::config::{Config, parse, to_toml};
use tb_core::{BarTheme, Chord};

const CHANGE: Duration = Duration::from_secs(10);
const QUIET: Duration = Duration::from_millis(500);

/// An empty directory for one test, under cargo's temp dir (never `$HOME`).
fn fresh_dir(test: &str) -> PathBuf {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join("config_conflict_overwrite").join(test);
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

fn mode(path: &Path) -> u32 {
    fs::metadata(path).unwrap().permissions().mode() & 0o7777
}

/// A backup is readable by its owner and grants no access the replaced file (`source`) did not.
fn no_broader_than(backup: u32, source: u32) -> bool {
    backup & 0o400 != 0 && (backup | source) & 0o777 == source & 0o777
}

/// What every entry directly inside each of `dirs` holds: a regular file's bytes, or a symlink's
/// target, by entry path.
fn entries(dirs: &[&Path]) -> Vec<(PathBuf, Entry)> {
    let mut found = Vec::new();
    for dir in dirs {
        for entry in fs::read_dir(dir).unwrap() {
            let path = entry.unwrap().path();
            let kind = fs::symlink_metadata(&path).unwrap().file_type();
            if kind.is_symlink() {
                found.push((path.clone(), Entry::Link(fs::read_link(&path).unwrap())));
            } else if kind.is_file() {
                found.push((path.clone(), Entry::Bytes(fs::read(&path).unwrap())));
            }
        }
    }
    found
}

#[derive(Debug, PartialEq, Eq)]
enum Entry {
    Bytes(Vec<u8>),
    Link(PathBuf),
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

/// `config` with comments and blank lines a re-serialisation would drop.
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

/// The version a refused save names; anything else fails the test.
fn changed(result: Result<(), ConfigFileError>) -> DiskVersion {
    match result {
        Err(ConfigFileError::Changed(version)) => version,
        other => panic!("expected a refusal as Changed, got {other:?}"),
    }
}

/// A config file at `<dir>/config.toml` that read the default, then an external editor replaced
/// the file with `newer`. Returns the stale config file and the version its refused save named.
fn stale(dir: &Path, newer: &str) -> (ConfigFile, DiskVersion) {
    let path = dir.join("config.toml");
    fs::write(&path, to_toml(&Config::default()).unwrap()).unwrap();
    let mut file = loaded_from(&path);
    external_rename(&path, newer);
    let seen = changed(file.save(relabelled("f6", "mine"), &theme()));
    (file, seen)
}

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
fn a_stale_save_is_refused_as_changed_naming_the_version_now_on_disk() {
    let dir = fresh_dir("names_the_version");
    let path = dir.join("config.toml");
    fs::write(&path, to_toml(&Config::default()).unwrap()).unwrap();
    let mut a = loaded_from(&path);
    let mut b = loaded_from(&path);
    a.save(relabelled("f5", "from a"), &theme()).unwrap();
    let mine = relabelled("f6", "mine");

    let first = changed(b.save(mine.clone(), &theme()));
    let again = changed(b.save(mine.clone(), &theme()));
    external_rename(&path, &hand_written(&relabelled("f7", "third")));
    let third = changed(b.save(mine, &theme()));

    assert!(!first.is_absent());
    assert_eq!(first, again, "one disk version is named the same each time");
    assert_ne!(third, first, "a later disk version is a different version");
}

#[test]
fn a_removed_file_is_named_as_the_absent_version() {
    let dir = fresh_dir("absent_version");
    let path = dir.join("config.toml");
    fs::write(&path, to_toml(&Config::default()).unwrap()).unwrap();
    let mut b = loaded_from(&path);
    fs::remove_file(&path).unwrap();

    let seen = changed(b.save(relabelled("f6", "mine"), &theme()));

    assert!(seen.is_absent());
    assert_eq!(names_in(&dir), Vec::<String>::new());
}

#[test]
fn an_acknowledged_overwrite_keeps_the_exact_replaced_bytes_and_mode_as_bak() {
    let dir = fresh_dir("overwrite_keeps_bak");
    let path = dir.join("config.toml");
    let bak = dir.join("config.toml.bak");
    let newer = hand_written(&relabelled("f5", "by hand"));
    let (mut b, seen) = stale(&dir, &newer);
    fs::set_permissions(&path, Permissions::from_mode(0o640)).unwrap();
    let mine = relabelled("f6", "mine");

    b.save_over(mine.clone(), &theme(), &seen).unwrap();

    assert_eq!(parse(&fs::read_to_string(&path).unwrap()).unwrap(), mine);
    assert_eq!(b.config(), &mine);
    assert_eq!(fs::read_to_string(&bak).unwrap(), newer, "the replaced version, byte for byte");
    assert!(
        no_broader_than(mode(&bak), 0o640),
        "the backup is no more readable than the replaced file: {:o}",
        mode(&bak)
    );
    assert_eq!(names_in(&dir), vec!["config.toml".to_owned(), "config.toml.bak".to_owned()]);

    let later = relabelled("f7", "later");
    b.save(later.clone(), &theme()).unwrap();
    assert_eq!(parse(&fs::read_to_string(&path).unwrap()).unwrap(), later, "no conflict with itself");
    assert_eq!(fs::read_to_string(&bak).unwrap(), newer, "an ordinary save leaves the backup alone");
    assert_eq!(names_in(&dir), vec!["config.toml".to_owned(), "config.toml.bak".to_owned()]);
}

#[test]
fn an_overwrite_acknowledging_an_older_version_is_refused_again_and_writes_nothing() {
    let dir = fresh_dir("overwrite_retry");
    let path = dir.join("config.toml");
    let bak = dir.join("config.toml.bak");
    let (mut b, acknowledged) = stale(&dir, &hand_written(&relabelled("f5", "second")));
    let third = hand_written(&relabelled("f7", "third"));
    external_rename(&path, &third);
    let inode = fs::metadata(&path).unwrap().ino();
    let mine = relabelled("f6", "mine");

    let now = changed(b.save_over(mine.clone(), &theme(), &acknowledged));

    assert_ne!(now, acknowledged);
    assert_eq!(fs::read_to_string(&path).unwrap(), third, "never a blind force");
    assert_eq!(fs::metadata(&path).unwrap().ino(), inode);
    assert!(!bak.exists(), "a refused overwrite writes no backup");
    assert_eq!(b.config(), &Config::default());
    assert_eq!(names_in(&dir), vec!["config.toml".to_owned()]);

    b.save_over(mine.clone(), &theme(), &now).unwrap();

    assert_eq!(parse(&fs::read_to_string(&path).unwrap()).unwrap(), mine);
    assert_eq!(fs::read_to_string(&bak).unwrap(), third, "the backup is the version acknowledged last");
}

#[test]
fn an_acknowledged_overwrite_of_a_removed_file_creates_it_without_a_backup() {
    let dir = fresh_dir("overwrite_removed");
    let path = dir.join("config.toml");
    fs::write(&path, to_toml(&Config::default()).unwrap()).unwrap();
    let mut b = loaded_from(&path);
    fs::remove_file(&path).unwrap();
    let mine = relabelled("f6", "mine");
    let seen = changed(b.save(mine.clone(), &theme()));

    b.save_over(mine.clone(), &theme(), &seen).unwrap();

    assert_eq!(parse(&fs::read_to_string(&path).unwrap()).unwrap(), mine);
    assert_eq!(names_in(&dir), vec!["config.toml".to_owned()], "nothing was replaced, nothing backed up");
}

#[test]
fn an_overwrite_acknowledging_a_removal_is_refused_when_the_file_came_back() {
    let dir = fresh_dir("overwrite_removed_then_back");
    let path = dir.join("config.toml");
    fs::write(&path, to_toml(&Config::default()).unwrap()).unwrap();
    let mut b = loaded_from(&path);
    fs::remove_file(&path).unwrap();
    let mine = relabelled("f6", "mine");
    let seen = changed(b.save(mine.clone(), &theme()));
    let back = hand_written(&relabelled("f5", "back"));
    external_rename(&path, &back);

    let now = changed(b.save_over(mine, &theme(), &seen));

    assert!(!now.is_absent());
    assert_eq!(fs::read_to_string(&path).unwrap(), back);
    assert_eq!(names_in(&dir), vec!["config.toml".to_owned()]);
}

#[test]
fn an_overwrite_never_silently_discards_a_backup_that_was_already_there() {
    let dir = fresh_dir("overwrite_existing_bak");
    let path = dir.join("config.toml");
    let unrelated = "my own backup, not made by the configurator\n";
    let bak = dir.join("config.toml.bak");
    fs::write(&bak, unrelated).unwrap();
    let newer = hand_written(&relabelled("f5", "by hand"));
    let (mut b, seen) = stale(&dir, &newer);
    fs::set_permissions(&path, Permissions::from_mode(0o600)).unwrap();
    let mine = relabelled("f6", "mine");

    let result = b.save_over(mine.clone(), &theme(), &seen);

    assert_eq!(fs::read_to_string(&bak).unwrap(), unrelated, "the earlier backup is never replaced");
    match result {
        Ok(()) => {
            assert_eq!(parse(&fs::read_to_string(&path).unwrap()).unwrap(), mine);
            let copies: Vec<String> = names_in(&dir)
                .into_iter()
                .filter(|name| name != "config.toml" && name != "config.toml.bak")
                .filter(|name| fs::read(dir.join(name)).ok().as_deref() == Some(newer.as_bytes()))
                .collect();
            assert_eq!(copies.len(), 1, "the replaced version is kept in one new file: {:?}", names_in(&dir));
            assert!(
                copies[0].starts_with("config.toml"),
                "the new backup is named after the config: {copies:?}"
            );
            let kept = mode(&dir.join(&copies[0]));
            assert!(no_broader_than(kept, 0o600), "the new backup stays private: {kept:o}");
        }
        Err(e) => {
            assert!(!matches!(e, ConfigFileError::Invalid(_)), "{e:?}");
            assert_eq!(fs::read_to_string(&path).unwrap(), newer, "a refused overwrite writes nothing");
            assert_eq!(names_in(&dir), vec!["config.toml".to_owned(), "config.toml.bak".to_owned()]);
        }
    }
}

#[test]
fn an_overwrite_never_writes_through_a_backup_symlink() {
    let dir = fresh_dir("overwrite_bak_symlink");
    let path = dir.join("config.toml");
    let elsewhere = dir.join("elsewhere.txt");
    fs::write(&elsewhere, "not a config backup\n").unwrap();
    let bak = dir.join("config.toml.bak");
    symlink(&elsewhere, &bak).unwrap();
    let newer = hand_written(&relabelled("f5", "by hand"));
    let (mut b, seen) = stale(&dir, &newer);
    let mine = relabelled("f6", "mine");

    let result = b.save_over(mine.clone(), &theme(), &seen);

    assert!(fs::symlink_metadata(&bak).unwrap().file_type().is_symlink(), "the backup link stays a link");
    assert_eq!(fs::read_link(&bak).unwrap(), elsewhere);
    assert_eq!(fs::read_to_string(&elsewhere).unwrap(), "not a config backup\n", "its target is untouched");
    match result {
        Ok(()) => {
            assert_eq!(parse(&fs::read_to_string(&path).unwrap()).unwrap(), mine);
            let kept = entries(&[&dir]).into_iter().filter(|(p, _)| *p != path && *p != elsewhere);
            assert!(
                kept.into_iter().any(|(_, e)| e == Entry::Bytes(newer.clone().into_bytes())),
                "the replaced version is kept in a new file: {:?}",
                names_in(&dir)
            );
        }
        Err(e) => {
            assert!(!matches!(e, ConfigFileError::Invalid(_)), "{e:?}");
            assert_eq!(fs::read_to_string(&path).unwrap(), newer, "a refused overwrite writes nothing");
            assert_eq!(
                names_in(&dir),
                vec!["config.toml".to_owned(), "config.toml.bak".to_owned(), "elsewhere.txt".to_owned()]
            );
        }
    }
}

#[test]
fn an_overwrite_at_a_symlink_keeps_the_link_and_the_replaced_bytes_recoverable_or_writes_nothing() {
    let dir = fresh_dir("overwrite_at_symlink");
    let real = dir.join("dotfiles");
    fs::create_dir(&real).unwrap();
    let target = real.join("config.toml");
    fs::write(&target, to_toml(&Config::default()).unwrap()).unwrap();
    let link = dir.join("config.toml");
    symlink(&target, &link).unwrap();
    let mut b = loaded_from(&link);
    let newer = hand_written(&relabelled("f5", "through the link"));
    fs::write(&target, &newer).unwrap();
    let mine = relabelled("f6", "mine");
    let seen = changed(b.save(mine.clone(), &theme()));

    let result = b.save_over(mine.clone(), &theme(), &seen);

    match result {
        Ok(()) => {
            assert_eq!(parse(&fs::read_to_string(&link).unwrap()).unwrap(), mine);
            let kept = entries(&[&dir, &real]);
            assert!(
                kept.iter().any(|(_, e)| *e == Entry::Bytes(newer.clone().into_bytes())),
                "the replaced bytes are recoverable: {kept:?}"
            );
            assert!(
                kept.iter().any(|(_, e)| *e == Entry::Link(target.clone())),
                "the link to the target is recoverable: {kept:?}"
            );
        }
        Err(e) => {
            assert!(!matches!(e, ConfigFileError::Invalid(_)), "{e:?}");
            assert!(fs::symlink_metadata(&link).unwrap().file_type().is_symlink(), "the link stays a link");
            assert_eq!(fs::read_link(&link).unwrap(), target);
            assert_eq!(fs::read_to_string(&target).unwrap(), newer, "a refused overwrite writes nothing");
            assert_eq!(names_in(&dir), vec!["config.toml".to_owned(), "dotfiles".to_owned()]);
            assert_eq!(names_in(&real), vec!["config.toml".to_owned()]);
        }
    }
}

#[test]
fn an_invalid_overwrite_is_refused_before_anything_is_written() {
    let dir = fresh_dir("overwrite_invalid");
    let path = dir.join("config.toml");
    let newer = hand_written(&relabelled("f5", "by hand"));
    let (mut b, seen) = stale(&dir, &newer);
    let mut bad = Config::default();
    let layer = bad.layout.layer_mut(Chord::Default).unwrap();
    let f6 = layer.groups[1].buttons.iter().find(|b| b.id.as_str() == "f6").unwrap().id.clone();
    layer.groups[1].buttons.iter_mut().find(|b| b.id.as_str() == "f5").unwrap().id = f6;

    let result = b.save_over(bad, &theme(), &seen);

    assert!(matches!(result, Err(ConfigFileError::Invalid(_))), "{result:?}");
    assert_eq!(fs::read_to_string(&path).unwrap(), newer);
    assert_eq!(names_in(&dir), vec!["config.toml".to_owned()]);
}

#[test]
fn simultaneous_overwrites_acknowledging_one_version_have_exactly_one_winner() {
    const WRITERS: usize = 4;
    const ROUNDS: usize = 30;
    for round in 0..ROUNDS {
        let dir = fresh_dir(&format!("overwrite_race/{round}"));
        let path = dir.join("config.toml");
        fs::write(&path, to_toml(&Config::default()).unwrap()).unwrap();
        let files: Vec<ConfigFile> = (0..WRITERS).map(|_| loaded_from(&path)).collect();
        let newer = hand_written(&relabelled("f5", &format!("newer {round}")));
        external_rename(&path, &newer);
        let conflicted: Vec<(ConfigFile, DiskVersion, Config)> = files
            .into_iter()
            .enumerate()
            .map(|(writer, mut file)| {
                let mine = relabelled("f6", &format!("r{round}w{writer}"));
                let seen = changed(file.save(mine.clone(), &theme()));
                (file, seen, mine)
            })
            .collect();
        let barrier = Barrier::new(WRITERS);

        let results: Vec<(Config, Result<(), ConfigFileError>)> = thread::scope(|scope| {
            let writers: Vec<_> = conflicted
                .into_iter()
                .map(|(mut file, seen, mine)| {
                    let barrier = &barrier;
                    scope.spawn(move || {
                        barrier.wait();
                        let result = file.save_over(mine.clone(), &theme(), &seen);
                        (mine, result)
                    })
                })
                .collect();
            writers.into_iter().map(|w| w.join().unwrap()).collect()
        });

        let winners: Vec<&Config> = results.iter().filter(|(_, r)| r.is_ok()).map(|(c, _)| c).collect();
        assert_eq!(winners.len(), 1, "round {round}: {results:?}");
        for (_, result) in &results {
            assert!(
                result.is_ok() || matches!(result, Err(ConfigFileError::Changed(_))),
                "round {round}: {result:?}"
            );
        }
        assert_eq!(fs::read_to_string(&path).unwrap(), to_toml(winners[0]).unwrap(), "round {round}");
        assert_eq!(
            fs::read_to_string(dir.join("config.toml.bak")).unwrap(),
            newer,
            "round {round}: the backup is the acknowledged version, never another overwriter's"
        );
    }
}

#[test]
fn an_acknowledged_overwrite_is_exactly_one_change_the_daemon_reads() {
    let dir = fresh_dir("overwrite_watch");
    let path = dir.join("config.toml");
    let newer = hand_written(&relabelled("f5", "by hand"));
    let (mut b, seen) = stale(&dir, &newer);
    let mut daemon = ConfigFile::new(&path);
    daemon.load_resolved(&theme()).unwrap();
    let changes = on_a_thread(daemon.watch().unwrap());
    let mine = relabelled("f6", "mine");

    b.save_over(mine.clone(), &theme(), &seen).unwrap();

    one_change(&changes);
    no_change(&changes);
    daemon.reload_resolved(&theme()).unwrap();
    assert_eq!(daemon.config(), &mine);
}
