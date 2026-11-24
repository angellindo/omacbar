//! Every write of a protected config file first keeps a verified generation of what it replaces
//! (ADR-0014): the absence of a file before the first write, or the exact bytes and permission
//! bits of the file before a save, a resolved save or an acknowledged overwrite. Generations are
//! private (store 0700, generation 0700, files 0600), outlive the `ConfigFile` that wrote them,
//! are listed newest first per config, are never deleted, and an unchanged save reuses the newest
//! verified one. Reads and refused saves keep nothing. The store is found under
//! `$XDG_STATE_HOME` when it is absolute, else under `$HOME/.local/state`. Creating it raises no
//! theme switch. No child process is started in this file (the theme watch case must see none).
//!
//! API pinned here: `Backups::{at, locate, list, verify}`, `Generation { id, reason, kind, mode,
//! len, health, .. }` (`Debug`, `Clone`), `GenerationId: Display` (its store directory name),
//! `Generation: PartialEq`, `Reason::{FirstWrite, Save, Overwrite}` (`Copy`, `Eq`, `Debug`), `GenerationKind::{File, Absent { dir_existed }}`,
//! `Health::{Verified, Damaged(_)}`, `ConfigFile::{protected, last_generation}`.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
// `last_generation` may return the id by value or by reference; the closure reads both.
#![allow(clippy::redundant_closure_for_method_calls)]

mod support {
    pub mod generations;
}

use std::collections::BTreeSet;
use std::ffi::OsStr;
use std::fs;
use std::path::Path;
use std::sync::mpsc::{RecvTimeoutError, channel};
use std::thread;
use std::time::Duration;

use support::generations::*;
use tb_config::{ActiveTheme, Backups, ConfigFile, ConfigFileError, Loaded, Reason};
use tb_core::config::{Config, ThemeSource, to_toml};

const SUITE: &str = "config_generations";
const QUIET: Duration = Duration::from_millis(700);

fn protected(profile: &Profile) -> ConfigFile {
    ConfigFile::protected(&profile.config, profile.backups())
}

fn loaded(profile: &Profile) -> ConfigFile {
    let mut file = protected(profile);
    assert_eq!(file.load(&theme()).unwrap(), Loaded::Read, "{} is a valid config", profile.config.display());
    file
}

/// A config validation refuses: two buttons share an id.
fn duplicate_ids() -> Config {
    let mut bad = Config::default();
    let layer = bad.layout.layer_mut(tb_core::Chord::Default).unwrap();
    let first = layer.groups[1].buttons[0].id.clone();
    layer.groups[1].buttons[1].id = first;
    bad
}

#[test]
fn the_first_write_into_a_missing_directory_keeps_an_absent_generation_before_the_default() {
    let p = Profile::new(SUITE, "first_write_no_dir");

    let mut file = protected(&p);
    assert_eq!(file.load(&theme()).unwrap(), Loaded::WroteDefault);

    assert_eq!(fs::read(&p.config).unwrap(), to_toml(&Config::default_for(&theme())).unwrap().into_bytes());
    let gens = p.generations();
    assert_eq!(gens.len(), 1, "one generation for the first write: {gens:?}");
    assert_eq!(gens[0].reason, Reason::FirstWrite);
    assert!(!absent_dir_existed(&gens[0]), "the config directory did not exist before: {gens:?}");
    assert!(is_verified(&gens[0]), "{gens:?}");
    assert_eq!(
        names_in(&generation_dir(&p.store, &gens[0])),
        vec!["manifest.toml"],
        "an absence keeps no data"
    );
    assert_eq!(file.last_generation().map(|id| id.to_string()), Some(gens[0].id.to_string()));
}

#[test]
fn the_first_write_into_an_existing_empty_directory_records_that_the_directory_existed() {
    let p = Profile::new(SUITE, "first_write_dir_existed");
    fs::create_dir_all(p.config.parent().unwrap()).unwrap();

    let mut file = protected(&p);
    let theme = ActiveTheme::under_home(&p.dir.join("home")).load().theme;
    file.load_resolved(&theme).unwrap();

    let gens = p.generations();
    assert_eq!(gens.len(), 1, "{gens:?}");
    assert_eq!(gens[0].reason, Reason::FirstWrite);
    assert!(absent_dir_existed(&gens[0]), "the config directory existed, empty: {gens:?}");
    assert!(p.config.is_file(), "the default was written after the generation");
}

#[test]
fn a_save_keeps_the_exact_hand_written_bytes_and_mode_it_replaces_and_keeps_the_file_mode() {
    let p = Profile::new(SUITE, "save_keeps_bytes");
    let mine = hand_written(&relabelled("Mine"));
    p.seed(&mine, 0o640);
    let mut file = loaded(&p);

    file.save(relabelled("Next"), &theme()).unwrap();

    assert_eq!(fs::read(&p.config).unwrap(), to_toml(&relabelled("Next")).unwrap().into_bytes());
    assert_eq!(mode(&p.config), 0o640, "a save keeps the permission bits of the file it replaces");
    let gens = p.generations();
    assert_eq!(gens.len(), 1, "{gens:?}");
    let kept = &gens[0];
    assert_eq!(kept.reason, Reason::Save);
    assert!(is_file_kind(kept), "{kept:?}");
    assert!(is_verified(kept), "{kept:?}");
    assert_eq!(kept.mode, 0o640, "the generation records the replaced file's permission bits");
    assert_eq!(kept.len, mine.len() as u64);
    assert_eq!(kept_bytes(&p.store, kept), mine, "comments and blank lines kept byte for byte");
    assert_eq!(file.last_generation().map(|id| id.to_string()), Some(kept.id.to_string()));
}

#[test]
fn the_store_and_every_generation_are_private_to_the_user() {
    let p = Profile::new(SUITE, "private_modes");
    p.seed(&hand_written(&relabelled("Mine")), 0o644);
    let mut file = loaded(&p);

    file.save(relabelled("Next"), &theme()).unwrap();

    let gens = p.generations();
    let dir = generation_dir(&p.store, &gens[0]);
    assert_eq!(mode(&p.store), 0o700, "the backups directory");
    assert_eq!(mode(&dir), 0o700, "the generation directory");
    assert_eq!(mode(&dir.join("manifest.toml")), 0o600, "the manifest");
    assert_eq!(mode(&dir.join("config.toml")), 0o600, "the kept bytes, even of a 0644 file");
}

#[test]
fn a_new_config_file_is_created_private() {
    let p = Profile::new(SUITE, "new_file_private");

    protected(&p).load(&theme()).unwrap();

    assert_eq!(mode(&p.config), 0o600);
}

#[test]
fn a_resolved_save_and_an_acknowledged_overwrite_keep_generations_and_the_overwrite_backup() {
    let p = Profile::new(SUITE, "resolved_and_overwrite");
    let mine = hand_written(&relabelled("Mine"));
    p.seed(&mine, 0o600);
    let mut file = loaded(&p);

    file.save_resolved(relabelled("Resolved"), &theme()).unwrap();
    let resolved = to_toml(&relabelled("Resolved")).unwrap().into_bytes();
    let theirs = hand_written(&relabelled("Theirs"));
    external_replace(&p.config, &theirs);
    let Err(ConfigFileError::Changed(seen)) = file.save(relabelled("Again"), &theme()) else {
        panic!("a save over an external change is refused")
    };
    file.save_over(relabelled("Again"), &theme(), &seen).unwrap();

    let gens = p.generations();
    assert_eq!(gens.len(), 2, "one per write, none for the refused save: {gens:?}");
    assert_eq!((gens[0].reason, gens[1].reason), (Reason::Overwrite, Reason::Save), "newest first");
    assert_eq!(kept_bytes(&p.store, &gens[0]), theirs, "the overwrite kept the external bytes");
    assert_eq!(kept_bytes(&p.store, &gens[1]), mine, "the resolved save kept the hand-written bytes");
    assert_eq!(fs::read(file.backup().unwrap()).unwrap(), theirs, "TB-43's overwrite backup is still kept");
    assert_ne!(resolved, theirs);
}

#[test]
fn an_unchanged_save_reuses_the_newest_verified_generation() {
    let p = Profile::new(SUITE, "dedupe");
    let mut file = protected(&p);
    file.load(&theme()).unwrap();
    let default = Config::default_for(&theme());

    file.save(default.clone(), &theme()).unwrap();
    let after_first = p.generations();
    file.save(default.clone(), &theme()).unwrap();
    file.save(default, &theme()).unwrap();

    let gens = p.generations();
    assert_eq!(gens.len(), 2, "first write + one kept default; unchanged saves add none: {gens:?}");
    assert_eq!(gens, after_first);
    assert_eq!(file.last_generation().map(|id| id.to_string()), Some(gens[0].id.to_string()));
    assert!(gens.iter().all(is_verified));
}

#[test]
fn reads_keep_no_generation() {
    let p = Profile::new(SUITE, "reads");
    p.seed(&hand_written(&relabelled("Mine")), 0o600);

    let mut file = loaded(&p);
    file.reload_resolved(&theme()).unwrap();
    let invalid = Profile::new(SUITE, "reads_invalid");
    invalid.seed(b"# mine\nversion = [ broken\n", 0o600);
    let mut rejected = ConfigFile::protected(&invalid.config, Backups::at(&p.store));
    assert!(matches!(rejected.load(&theme()).unwrap(), Loaded::Rejected(_)));

    assert_eq!(p.generations(), vec![]);
    assert_eq!(Backups::at(&p.store).list(&invalid.config).unwrap(), vec![]);
}

#[test]
fn refused_saves_keep_no_generation_and_leave_the_file() {
    let p = Profile::new(SUITE, "refused");
    let mine = hand_written(&relabelled("Mine"));
    p.seed(&mine, 0o600);
    let mut file = loaded(&p);

    let invalid_save = file.save(duplicate_ids(), &theme());
    let mut missing_theme = relabelled("Theme");
    missing_theme.theme = ThemeSource::File { path: p.dir.join("no-such-theme.toml").display().to_string() };
    let theme_save = file.save_resolved(missing_theme, &theme());
    let theirs = hand_written(&relabelled("Theirs"));
    external_replace(&p.config, &theirs);
    let stale_save = file.save(relabelled("Stale"), &theme());

    assert!(matches!(invalid_save, Err(ConfigFileError::Invalid(_))), "{invalid_save:?}");
    assert!(matches!(theme_save, Err(ConfigFileError::Theme(_))), "{theme_save:?}");
    assert!(matches!(stale_save, Err(ConfigFileError::Changed(_))), "{stale_save:?}");
    assert_eq!(fs::read(&p.config).unwrap(), theirs);
    assert_eq!(p.generations(), vec![]);
}

#[test]
fn saving_over_an_invalid_personal_file_keeps_its_exact_bytes() {
    let p = Profile::new(SUITE, "invalid_personal");
    let broken =
        b"# my half-finished layout, do not lose\nversion = 1\n[[layer]\nchord = \"default\"\n".to_vec();
    p.seed(&broken, 0o640);
    let mut file = protected(&p);
    assert!(matches!(file.load(&theme()).unwrap(), Loaded::Rejected(_)));

    file.save(Config::default(), &theme()).unwrap();

    let gens = p.generations();
    assert_eq!(gens.len(), 1, "{gens:?}");
    assert_eq!(kept_bytes(&p.store, &gens[0]), broken);
    assert_eq!(gens[0].mode, 0o640);
    assert!(is_verified(&gens[0]));
}

#[test]
fn generations_outlive_their_config_file_and_are_never_deleted() {
    let p = Profile::new(SUITE, "outlive");
    let mut versions = vec![hand_written(&relabelled("v0"))];
    p.seed(&versions[0], 0o600);
    {
        let mut file = loaded(&p);
        for n in 1..=5 {
            let next = relabelled(&format!("v{n}"));
            file.save(next.clone(), &theme()).unwrap();
            versions.push(to_toml(&next).unwrap().into_bytes());
        }
    }

    let gens = p.generations();
    assert_eq!(gens.len(), 5, "one per replaced version: {gens:?}");
    let ids: BTreeSet<String> = gens.iter().map(|g| g.id.to_string()).collect();
    assert_eq!(ids.len(), 5, "ids are unique");
    let kept: Vec<Vec<u8>> = gens.iter().rev().map(|g| kept_bytes(&p.store, g)).collect();
    assert_eq!(kept, versions[..5].to_vec(), "oldest to newest, the versions each save replaced");
    let backups = Backups::at(&p.store);
    for g in &gens {
        assert!(is_verified(g), "{g:?}");
        backups.verify(&g.id).unwrap_or_else(|e| panic!("{} verifies: {e}", g.id));
    }
}

#[test]
fn a_listing_names_only_the_generations_of_that_config() {
    let p = Profile::new(SUITE, "per_config");
    let other = p.other_config("other");
    let mut mine = protected(&p);
    mine.load(&theme()).unwrap();
    let mut theirs = ConfigFile::protected(&other, p.backups());
    theirs.load(&theme()).unwrap();
    theirs.save(relabelled("Theirs"), &theme()).unwrap();

    assert_eq!(p.generations().len(), 1);
    assert_eq!(p.generations_of(&other).len(), 2);
    let all: BTreeSet<String> =
        p.generations().iter().chain(p.generations_of(&other).iter()).map(|g| g.id.to_string()).collect();
    assert_eq!(all.len(), 3, "ids are unique across configs sharing the store");
}

#[test]
fn creating_the_store_beside_the_theme_hook_raises_no_theme_switch() {
    let p = Profile::new(SUITE, "no_theme_switch");
    let home = p.dir.join("home");
    fs::create_dir_all(home.join(".local/state/omarchy/current/theme")).unwrap();
    let mut watch = ActiveTheme::under_home(&home).watch().unwrap();
    let (tx, rx) = channel();
    thread::spawn(move || {
        let _ = tx.send(watch.next_change());
    });
    let store = home.join(".local/state/touchbar-customizer/backups");

    let mut file = ConfigFile::protected(&p.config, Backups::at(&store));
    file.load(&theme()).unwrap();
    file.save(relabelled("Next"), &theme()).unwrap();

    assert_eq!(Backups::at(&store).list(&p.config).unwrap().len(), 2, "both writes kept a generation");
    match rx.recv_timeout(QUIET) {
        Err(RecvTimeoutError::Timeout) => {}
        other => panic!("creating backups/ beside theme-set is not a theme switch: {other:?}"),
    }
}

#[test]
fn the_store_is_under_an_absolute_xdg_state_home_else_under_home() {
    let home = Path::new("/home/someone");
    let backups = |base: &str| Path::new(base).join("touchbar-customizer/backups");

    assert_eq!(Backups::locate(Some(home), Some(OsStr::new("/run/state"))), Some(backups("/run/state")));
    assert_eq!(Backups::locate(Some(home), None), Some(backups("/home/someone/.local/state")));
    assert_eq!(
        Backups::locate(Some(home), Some(OsStr::new("relative/state"))),
        Some(backups("/home/someone/.local/state")),
        "a relative XDG_STATE_HOME is ignored"
    );
    assert_eq!(
        Backups::locate(Some(home), Some(OsStr::new(""))),
        Some(backups("/home/someone/.local/state")),
        "an empty XDG_STATE_HOME is unset"
    );
    assert_eq!(Backups::locate(None, Some(OsStr::new("/run/state"))), Some(backups("/run/state")));
    assert_eq!(Backups::locate(None, None), None);
    assert_eq!(Backups::locate(None, Some(OsStr::new("relative"))), None);
}
