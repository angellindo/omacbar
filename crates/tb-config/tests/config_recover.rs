//! Restoring a generation through the library is a protected compare-and-swap against the version
//! the config file last saw (TB-43 semantics): `look` records the bytes on disk as seen without
//! parsing them, `recover` verifies the chosen generation, keeps a `recover` generation of what it
//! replaces, and writes the kept bytes with the kept permission bits. A file changed after the
//! look is `Changed`, nothing is written and both versions stay; an editor that saw a file which
//! was removed meanwhile gets `Changed` (absent) and nothing is created.
//!
//! API pinned here: `ConfigFile::look(&mut self) -> Result<_, ConfigFileError>`,
//! `ConfigFile::recover(&mut self, &GenerationId) -> Result<_, ConfigFileError>`,
//! `Reason::Recover`, and `last_generation()` naming the generation the restore kept.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
// `last_generation` may return the id by value or by reference; the closure reads both.
#![allow(clippy::redundant_closure_for_method_calls)]

mod support {
    pub mod generations;
}

use std::fs;

use support::generations::*;
use tb_config::{ConfigFile, ConfigFileError, GenerationKind, Health, Loaded, Reason};

const SUITE: &str = "config_recover";

/// A seeded hand-written 0640 file `a`, then saves of `b` and `c` through one protected file.
fn history(p: &Profile) -> (Vec<u8>, Vec<u8>) {
    let a = hand_written(&relabelled("first"));
    p.seed(&a, 0o640);
    let mut file = ConfigFile::protected(&p.config, p.backups());
    assert_eq!(file.load(&theme()).unwrap(), Loaded::Read);
    file.save(relabelled("second"), &theme()).unwrap();
    file.save(relabelled("third"), &theme()).unwrap();
    (a, fs::read(&p.config).unwrap())
}

fn holding(p: &Profile, bytes: &[u8]) -> tb_config::Generation {
    p.generations()
        .into_iter()
        .find(|g| matches!(g.kind, GenerationKind::File) && kept_bytes(&p.store, g) == bytes)
        .expect("a generation holds these bytes")
}

#[test]
fn recover_writes_the_chosen_generation_and_keeps_what_it_replaces() {
    let p = Profile::new(SUITE, "writes");
    let (a, c) = history(&p);
    let chosen = holding(&p, &a);

    let mut file = ConfigFile::protected(&p.config, p.backups());
    file.look().unwrap();
    file.recover(&chosen.id).unwrap();

    assert_eq!(fs::read(&p.config).unwrap(), a, "exact bytes, comments included");
    assert_eq!(mode(&p.config), 0o640);
    let newest = p.generations()[0].clone();
    assert_eq!(newest.reason, Reason::Recover);
    assert_eq!(newest.kind, GenerationKind::File);
    assert_eq!(newest.health, Health::Verified);
    assert_eq!(newest.mode, 0o640);
    assert_eq!(kept_bytes(&p.store, &newest), c, "the replaced file is kept exactly");
    let last = file.last_generation().map(|id| id.to_string());
    assert_eq!(last, Some(newest.id.to_string()), "the file names the generation it kept");
    assert_eq!(p.backups().verify(&chosen.id).unwrap().health, Health::Verified, "the chosen one stays");
}

#[test]
fn a_change_after_look_is_refused_and_both_versions_stay() {
    let p = Profile::new(SUITE, "changed");
    let (a, _) = history(&p);
    let chosen = holding(&p, &a);
    let before: Vec<_> = p.generations();

    let mut file = ConfigFile::protected(&p.config, p.backups());
    file.look().unwrap();
    let theirs = hand_written(&relabelled("theirs"));
    external_replace(&p.config, &theirs);
    let err = file.recover(&chosen.id).unwrap_err();

    assert!(matches!(&err, ConfigFileError::Changed(now) if !now.is_absent()), "{err:?}");
    assert_eq!(fs::read(&p.config).unwrap(), theirs, "the newer file stays");
    assert_eq!(p.generations(), before, "no generation is added for a write that did not happen");
    assert_eq!(kept_bytes(&p.store, &chosen), a, "the chosen generation is untouched");
}

#[test]
fn an_editor_that_saw_a_file_now_removed_gets_changed_and_nothing_is_created() {
    let p = Profile::new(SUITE, "removed");
    let (a, _) = history(&p);
    let chosen = holding(&p, &a);
    let before: Vec<_> = p.generations();

    let mut file = ConfigFile::protected(&p.config, p.backups());
    assert_eq!(file.load(&theme()).unwrap(), Loaded::Read);
    fs::remove_dir_all(p.config.parent().unwrap()).unwrap();
    let err = file.recover(&chosen.id).unwrap_err();

    assert!(matches!(&err, ConfigFileError::Changed(now) if now.is_absent()), "{err:?}");
    assert!(!p.config.parent().unwrap().exists(), "a stale restore does not even create the directory");
    assert_eq!(p.generations(), before);
}
