//! Generations survive damage, interruptions and concurrent writers without losing a valid one:
//! a damaged generation (edited data, garbled manifest, data swapped for a symlink) is reported,
//! never verified, never reused and never deleted, and the next save keeps a new verified copy;
//! the states a crash leaves (a half-built temp generation, a complete generation whose save never
//! renamed, a stale config temp) are ignored and kept while later saves succeed; simultaneous
//! writers through one store get unique ids and a listing never shows a half-written generation;
//! two editors saving one version keep exactly one generation of it.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod support {
    pub mod generations;
}

use std::collections::BTreeSet;
use std::fs;
use std::os::unix::fs::symlink;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Barrier};
use std::thread;

use support::generations::*;
use tb_config::{Backups, ConfigFile, ConfigFileError, Loaded};
use tb_core::config::to_toml;

const SUITE: &str = "config_generation_faults";

fn loaded(profile: &Profile) -> ConfigFile {
    let mut file = ConfigFile::protected(&profile.config, profile.backups());
    assert_eq!(file.load(&theme()).unwrap(), Loaded::Read);
    file
}

/// A profile whose config was `mine` (hand-written) and was saved once, so the store holds one
/// verified generation of `mine`. Returns the profile and `mine`.
fn saved_once(test: &str) -> (Profile, Vec<u8>) {
    let p = Profile::new(SUITE, test);
    let mine = hand_written(&relabelled("Mine"));
    p.seed(&mine, 0o600);
    loaded(&p).save(relabelled("One"), &theme()).unwrap();
    assert_eq!(p.generations().len(), 1);
    (p, mine)
}

/// The config goes back to `mine` (as an editor would leave it) and a fresh editor saves over it.
fn save_over_mine_again(p: &Profile, mine: &[u8]) {
    external_replace(&p.config, mine);
    loaded(p).save(relabelled("Two"), &theme()).unwrap();
}

fn assert_kept_verified(p: &Profile, bytes: &[u8], damaged: &str) {
    let gens = p.generations();
    assert!(
        gens.iter()
            .any(|g| g.id.to_string() != damaged && is_verified(g) && kept_bytes(&p.store, g) == bytes),
        "a new verified generation keeps the replaced bytes: {gens:?}"
    );
    assert!(p.store.join(damaged).is_dir(), "the damaged generation {damaged} is kept, not deleted");
    for g in gens.iter().filter(|g| g.id.to_string() == damaged) {
        assert!(!is_verified(g), "{g:?}");
        assert!(Backups::at(&p.store).verify(&g.id).is_err(), "{damaged} must not verify");
    }
}

#[test]
fn edited_generation_data_is_reported_damaged_and_never_reused() {
    let (p, mine) = saved_once("edited_data");
    let first = p.generations()[0].clone();
    fs::write(generation_dir(&p.store, &first).join("config.toml"), b"# tampered\n").unwrap();

    let listed = p.generations();
    assert!(!is_verified(&listed[0]), "{listed:?}");
    assert!(Backups::at(&p.store).verify(&first.id).is_err());
    save_over_mine_again(&p, &mine);

    assert_kept_verified(&p, &mine, &first.id.to_string());
}

#[test]
fn a_garbled_manifest_is_never_verified_and_never_reused() {
    let (p, mine) = saved_once("garbled_manifest");
    let first = p.generations()[0].clone();
    fs::write(generation_dir(&p.store, &first).join("manifest.toml"), b"format = [ garbled\n").unwrap();

    assert!(Backups::at(&p.store).verify(&first.id).is_err());
    save_over_mine_again(&p, &mine);

    assert_kept_verified(&p, &mine, &first.id.to_string());
}

#[test]
fn generation_data_swapped_for_a_symlink_is_damaged_even_with_the_same_bytes() {
    let (p, mine) = saved_once("data_symlink");
    let first = p.generations()[0].clone();
    let data = generation_dir(&p.store, &first).join("config.toml");
    let elsewhere = p.dir.join("same-bytes.toml");
    fs::write(&elsewhere, &mine).unwrap();
    fs::remove_file(&data).unwrap();
    symlink(&elsewhere, &data).unwrap();

    let listed = p.generations();
    assert!(!is_verified(&listed[0]), "{listed:?}");
    assert!(Backups::at(&p.store).verify(&first.id).is_err());
    save_over_mine_again(&p, &mine);

    assert_kept_verified(&p, &mine, &first.id.to_string());
}

#[test]
fn a_half_built_temp_generation_is_ignored_and_kept_while_saves_go_on() {
    let p = Profile::new(SUITE, "temp_generation");
    let mine = hand_written(&relabelled("Mine"));
    p.seed(&mine, 0o600);
    let mut file = loaded(&p);
    file.save(relabelled("One"), &theme()).unwrap();
    let leftover = p.store.join(".tmp-999999-0");
    fs::create_dir_all(&leftover).unwrap();
    fs::write(leftover.join("config.toml"), b"# half writ").unwrap();
    let leftover_before = snapshot(&leftover);

    assert_eq!(p.generations().len(), 1, "a temp generation is not listed");
    file.save(relabelled("Two"), &theme()).unwrap();

    let gens = p.generations();
    assert_eq!(gens.len(), 2, "{gens:?}");
    assert!(gens.iter().all(is_verified));
    assert_eq!(kept_bytes(&p.store, &gens[1]), mine);
    assert_eq!(snapshot(&leftover), leftover_before, "the leftover is kept for the person to inspect");
}

#[test]
fn a_complete_generation_whose_save_never_renamed_stays_and_the_next_save_succeeds() {
    let (p, mine) = saved_once("crash_after_generation");
    // The crash state: the generation of `mine` is complete, the file still holds `mine`.
    external_replace(&p.config, &mine);
    let first = p.generations()[0].clone();
    let first_dir = snapshot(&generation_dir(&p.store, &first));

    let mut file = loaded(&p);
    file.save(relabelled("Retry"), &theme()).unwrap();

    assert_eq!(fs::read(&p.config).unwrap(), to_toml(&relabelled("Retry")).unwrap().into_bytes());
    assert_eq!(snapshot(&generation_dir(&p.store, &first)), first_dir, "the earlier generation is untouched");
    let gens = p.generations();
    assert!(gens.iter().all(is_verified), "{gens:?}");
    assert!(gens.iter().any(|g| kept_bytes(&p.store, g) == mine));
}

#[test]
fn a_stale_config_temp_from_an_interrupted_save_is_kept_and_skipped() {
    let p = Profile::new(SUITE, "stale_config_temp");
    let mine = hand_written(&relabelled("Mine"));
    p.seed(&mine, 0o600);
    let stale = p.config.with_file_name(format!(".config.toml.{}.0.tmp", std::process::id()));
    fs::write(&stale, b"# interrupted").unwrap();

    loaded(&p).save(relabelled("Next"), &theme()).unwrap();

    assert_eq!(fs::read(&stale).unwrap(), b"# interrupted");
    assert_eq!(kept_bytes(&p.store, &p.generations()[0]), mine);
}

#[test]
fn simultaneous_writers_through_one_store_get_unique_ids_and_listings_never_see_half_a_generation() {
    const WRITERS: usize = 6;
    const SAVES: usize = 3;
    let p = Arc::new(Profile::new(SUITE, "simultaneous"));
    let configs: Vec<_> = (0..WRITERS).map(|n| p.other_config(&format!("writer-{n}"))).collect();
    let start = Arc::new(Barrier::new(WRITERS + 1));
    let done = Arc::new(AtomicBool::new(false));

    let lister = {
        let (p, configs, done, start) = (p.clone(), configs.clone(), done.clone(), start.clone());
        thread::spawn(move || {
            start.wait();
            let mut seen = 0;
            while !done.load(Ordering::SeqCst) {
                for config in &configs {
                    if let Ok(gens) = Backups::at(&p.store).list(config) {
                        assert!(
                            gens.iter().all(is_verified),
                            "a listing showed an unfinished generation: {gens:?}"
                        );
                        seen += gens.len();
                    }
                }
            }
            seen
        })
    };
    let writers: Vec<_> = configs
        .iter()
        .cloned()
        .enumerate()
        .map(|(n, config)| {
            let (store, start) = (p.store.clone(), start.clone());
            thread::spawn(move || {
                let mut file = ConfigFile::protected(&config, Backups::at(&store));
                file.load(&theme()).unwrap();
                start.wait();
                for s in 0..SAVES {
                    file.save(relabelled(&format!("w{n}s{s}")), &theme()).unwrap();
                }
            })
        })
        .collect();
    for w in writers {
        w.join().unwrap();
    }
    done.store(true, Ordering::SeqCst);
    let _ = lister.join().unwrap();

    let mut ids = BTreeSet::new();
    for (n, config) in configs.iter().enumerate() {
        let gens = p.generations_of(config);
        assert_eq!(gens.len(), 1 + SAVES, "writer {n}: absence + one per save: {gens:?}");
        assert!(gens.iter().all(is_verified), "{gens:?}");
        for (s, g) in gens.iter().rev().skip(2).enumerate() {
            assert_eq!(
                kept_bytes(&p.store, g),
                to_toml(&relabelled(&format!("w{n}s{s}"))).unwrap().into_bytes()
            );
        }
        ids.extend(gens.iter().map(|g| g.id.to_string()));
    }
    assert_eq!(ids.len(), WRITERS * (1 + SAVES), "every generation has its own id");
}

#[test]
fn two_editors_saving_one_version_keep_exactly_one_generation_of_it() {
    let p = Arc::new(Profile::new(SUITE, "two_editors"));
    let mine = hand_written(&relabelled("Mine"));
    p.seed(&mine, 0o600);
    let gate = Arc::new(Barrier::new(2));
    let editors: Vec<_> = ["A", "B"]
        .into_iter()
        .map(|label| {
            let (p, gate) = (p.clone(), gate.clone());
            thread::spawn(move || {
                let mut file = loaded(&p);
                gate.wait();
                file.save(relabelled(label), &theme())
            })
        })
        .collect();
    let results: Vec<_> = editors.into_iter().map(|e| e.join().unwrap()).collect();

    assert_eq!(results.iter().filter(|r| r.is_ok()).count(), 1, "{results:?}");
    assert!(results.iter().any(|r| matches!(r, Err(ConfigFileError::Changed(_)))), "{results:?}");
    let gens = p.generations();
    assert_eq!(gens.len(), 1, "{gens:?}");
    assert_eq!(kept_bytes(&p.store, &gens[0]), mine);
}
