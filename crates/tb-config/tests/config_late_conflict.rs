//! A conflict that the last check of a protected write finds (ADR-0015, "never deleted"):
//!
//! - found before the generation is published (`Step::Stage`): `Changed`, the store keeps
//!   nothing (no generation, no temp), the file another editor left stays as it is;
//! - found after the generation was published (`Step::Publish`): `ChangedAfterKeep` (the file
//!   changed) or `RefusedAfterKeep` (the path became unsafe), naming the retained id. The file
//!   another editor left is not replaced, the retained generation stays verified, holds exactly
//!   what the write would have replaced and is listed; the attempt is never a success (the editor's
//!   config in memory is not the draft) and a retry from the file now on disk succeeds.
//!
//! Each case runs the real protected library in a child (this test binary, one ignored helper)
//! held by `support/syscall_pause.py` at that step while the parent changes the config; no product
//! hook. Profiles live under cargo's temp dir with their own store. The variant is read from the
//! error's `Debug` text, so this suite compiles against any library with these write methods.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod support {
    pub mod generations;
    pub mod late_conflict;
    pub mod recovery;
}

use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};

use support::generations::{
    Entry, Profile, entry, hand_written, is_verified, kept_bytes, mode, relabelled, theme,
};
use support::late_conflict::{
    Held, Paused, Step, arm, helper_command, helper_passed, paused_in, replace_by_hand, replace_with_link,
    run_held, signals_at, temps_in,
};
use support::recovery::three_versions;
use tb_config::{ConfigFile, ConfigFileError, Generation, GenerationId, Loaded, Reason};
use tb_core::config::Config;

const SUITE: &str = "config_late_conflict";
const HELPER: &str = "late_conflict_library_helper";
const CASE: &str = "TB_LATE_CONFLICT_CASE";
const DIR: &str = "TB_LATE_CONFLICT_DIR";

/// The profile of `case` without wiping it (the helper's view of the parent's profile).
fn profile_at(dir: &Path) -> Profile {
    Profile {
        dir: dir.to_path_buf(),
        config: dir.join("home/.config/touchbar-customizer/config.toml"),
        store: dir.join("state/touchbar-customizer/backups"),
    }
}

fn signals(profile: &Profile) -> PathBuf {
    profile.dir.join("signals")
}

/// Runs the helper for `case` held at `step`; `at_pause` is the other editor's change.
fn held(profile: &Profile, case: &str, step: Step, at_pause: impl FnOnce(&Paused)) -> Held {
    let signals = signals_at(&signals(profile));
    let mut cmd = helper_command(HELPER, &[(DIR, &profile.dir)]);
    cmd.env(CASE, case);
    let held = run_held(&cmd, &signals, &profile.store, step, at_pause);
    helper_passed(&held);
    held
}

fn external() -> Vec<u8> {
    hand_written(&relabelled("another editor"))
}

fn generation(profile: &Profile, id: &str) -> Generation {
    profile
        .generations()
        .into_iter()
        .find(|g| g.id.to_string() == id)
        .unwrap_or_else(|| panic!("generation {id} is not listed; {:?}", profile.generations()))
}

/// The retained generation: listed, verified, of `reason`, holding exactly `bytes`.
fn retained(profile: &Profile, id: &str, reason: Reason, bytes: &[u8]) {
    let g = generation(profile, id);
    assert!(is_verified(&g), "{g:?}");
    assert_eq!(g.reason, reason, "{g:?}");
    assert_eq!(kept_bytes(&profile.store, &g), bytes, "generation {id} holds what the write would replace");
    profile.backups().verify(&g.id).unwrap();
}

/// A seeded hand-written file (0640) and the draft a save would write.
fn seeded(test: &str) -> (Profile, Vec<u8>) {
    let profile = Profile::new(SUITE, test);
    let seed = hand_written(&relabelled("mine"));
    profile.seed(&seed, 0o640);
    (profile, seed)
}

fn draft() -> Config {
    relabelled("draft")
}

#[test]
fn a_change_before_the_save_publishes_its_generation_keeps_nothing() {
    let (profile, seed) = seeded("stage_save");
    let config = profile.config.clone();
    let run = held(&profile, "save", Step::Stage, |_| {
        fs::set_permissions(&config, fs::Permissions::from_mode(0o600)).unwrap();
    });
    run.held_at(Step::Stage);
    assert_eq!(fs::read(&profile.config).unwrap(), seed);
    assert_eq!(mode(&profile.config), 0o600, "the other editor's mode stays");
    assert_eq!(profile.generations(), [], "a conflict before publication keeps no generation");
    assert_eq!(temps_in(&profile.store), Vec::<String>::new(), "nor a temp generation");
}

#[test]
fn a_change_after_the_save_published_its_generation_is_refused_and_the_generation_retained() {
    let (profile, seed) = seeded("publish_save");
    let config = profile.config.clone();
    let run = held(&profile, "save", Step::Publish, |_| replace_by_hand(&config, &external()));
    let id = run.published();
    retained(&profile, &id, Reason::Save, &seed);
    assert_eq!(fs::read(&profile.config).unwrap(), to_bytes(&draft()), "the retry from disk saved the draft");
    assert!(
        is_verified(&generation(&profile, &id)),
        "the retained generation stays verified after the retry"
    );
}

#[test]
fn a_change_after_an_overwrite_published_its_generation_is_refused_and_the_generation_retained() {
    let (profile, _) = seeded("publish_overwrite");
    let config = profile.config.clone();
    let later = hand_written(&relabelled("a later editor"));
    let run = held(&profile, "overwrite", Step::Publish, |_| replace_by_hand(&config, &later));
    let id = run.published();
    retained(&profile, &id, Reason::Overwrite, &external());
    assert_eq!(fs::read(&profile.config).unwrap(), to_bytes(&draft()), "the retry from disk saved the draft");
}

#[test]
fn a_change_after_a_factory_reset_save_published_its_generation_is_refused_and_the_generation_retained() {
    let (profile, seed) = seeded("publish_factory_reset");
    let config = profile.config.clone();
    let run = held(&profile, "factory-reset", Step::Publish, |_| replace_by_hand(&config, &external()));
    let id = run.published();
    retained(&profile, &id, Reason::FactoryReset, &seed);
}

#[test]
fn a_change_after_a_restore_published_its_generation_is_refused_and_the_generation_retained() {
    let profile = Profile::new(SUITE, "publish_recover");
    let history = three_versions(&profile, 0o640);
    let config = profile.config.clone();
    let run = held(&profile, "recover", Step::Publish, |_| replace_by_hand(&config, &external()));
    let id = run.published();
    retained(&profile, &id, Reason::Recover, &history.c);
}

#[test]
fn a_change_after_a_file_removal_published_its_generation_is_refused_and_the_generation_retained() {
    let profile = Profile::new(SUITE, "publish_remove");
    let mut first = ConfigFile::protected(&profile.config, profile.backups());
    assert_eq!(first.load(&theme()).unwrap(), Loaded::WroteDefault);
    let written = fs::read(&profile.config).unwrap();
    let config = profile.config.clone();
    let run = held(&profile, "remove", Step::Publish, |_| replace_by_hand(&config, &external()));
    let id = run.published();
    retained(&profile, &id, Reason::Recover, &written);
}

#[test]
fn a_config_made_a_symlink_after_the_save_published_its_generation_is_refused_with_the_generation() {
    let (profile, seed) = seeded("publish_symlink");
    let config = profile.config.clone();
    let elsewhere = profile.dir.join("elsewhere.toml");
    let target = elsewhere.clone();
    let run =
        held(&profile, "save-unsafe", Step::Publish, |_| replace_with_link(&config, &target, &external()));
    let id = run.published();
    assert_eq!(
        entry(&profile.config),
        Some(Entry::Link { to: elsewhere.clone() }),
        "the link is not replaced"
    );
    assert_eq!(fs::read(&elsewhere).unwrap(), external(), "nothing is written through it");
    retained(&profile, &id, Reason::Save, &seed);
}

fn to_bytes(config: &Config) -> Vec<u8> {
    tb_core::config::to_toml(config).unwrap().into_bytes()
}

fn generation_bytes(profile: &Profile, id: &str) -> Vec<u8> {
    kept_bytes(&profile.store, &generation(profile, id))
}

// ----- the helper: the protected library, held by the supervisor -----

/// `Debug` of the error says which refusal it is; `Display` names the retained id.
fn late(err: &ConfigFileError, variant: &str, id: &str) {
    let debug = format!("{err:?}");
    assert!(debug.starts_with(variant), "expected {variant}, got {debug}");
    let shown = err.to_string();
    assert!(shown.contains(&format!("generation {id}")), "the refusal names generation {id}: {shown}");
}

fn changed_before_publication(err: &ConfigFileError) {
    assert!(matches!(err, ConfigFileError::Changed(_)), "expected Changed, got {err:?}");
    assert!(!err.to_string().contains("generation"), "nothing was retained: {err}");
}

#[test]
#[ignore = "run held by the late-conflict cases, in their child"]
fn late_conflict_library_helper() {
    let (Some(case), Some(dir)) = (std::env::var_os(CASE), std::env::var_os(DIR)) else { return };
    let profile = profile_at(Path::new(&dir));
    let signals = signals(&profile);
    let theme = theme();
    let mut file = ConfigFile::protected(&profile.config, profile.backups());
    match case.to_str().unwrap() {
        "save" | "save-unsafe" => {
            assert_eq!(file.load(&theme).unwrap(), Loaded::Read);
            let before = file.config().clone();
            arm(&signals);
            let err = file.save(draft(), &theme).unwrap_err();
            assert_eq!(file.config(), &before, "a refused save leaves the editor's config as it was");
            match paused_in(&signals) {
                Some(p) if p.step == "stage" => changed_before_publication(&err),
                Some(p) => {
                    let id = p.generation.unwrap();
                    let variant = if case == "save" { "ChangedAfterKeep" } else { "RefusedAfterKeep" };
                    late(&err, variant, &id);
                    if case == "save-unsafe" {
                        assert!(err.to_string().contains("symbolic link"), "{err}");
                        return;
                    }
                    // Retry from the file now on disk: it is replaced, its bytes kept.
                    let mut again = ConfigFile::protected(&profile.config, profile.backups());
                    assert_eq!(again.load(&theme).unwrap(), Loaded::Read);
                    again.save(draft(), &theme).unwrap();
                    let newest = again.last_generation().unwrap().to_string();
                    assert_ne!(newest, id);
                    assert_eq!(generation_bytes(&profile, &newest), external());
                }
                None => panic!("the save was never held"),
            }
        }
        "overwrite" => {
            assert_eq!(file.load(&theme).unwrap(), Loaded::Read);
            replace_by_hand(&profile.config, &external());
            let Err(ConfigFileError::Changed(seen)) = file.save(draft(), &theme) else {
                panic!("a save over a newer file is Changed")
            };
            arm(&signals);
            let err = file.save_over(draft(), &theme, &seen).unwrap_err();
            let id = paused_in(&signals).unwrap().generation.unwrap();
            late(&err, "ChangedAfterKeep", &id);
            assert_ne!(fs::read(&profile.config).unwrap(), to_bytes(&draft()), "not replaced");
            file.look().unwrap();
            file.save(draft(), &theme).unwrap();
            assert_eq!(fs::read(&profile.config).unwrap(), to_bytes(&draft()), "a retry from disk saves");
        }
        "factory-reset" => {
            assert_eq!(file.load(&theme).unwrap(), Loaded::Read);
            arm(&signals);
            let err = file
                .save_resolved_for(Config::default_for(&theme), &theme, Reason::FactoryReset)
                .unwrap_err();
            let id = paused_in(&signals).unwrap().generation.unwrap();
            late(&err, "ChangedAfterKeep", &id);
            assert_eq!(fs::read(&profile.config).unwrap(), external(), "the other editor's file stays");
            file.look().unwrap();
            file.save_resolved_for(Config::default_for(&theme), &theme, Reason::FactoryReset).unwrap();
        }
        "recover" | "remove" => {
            let target: GenerationId = profile.generations().last().unwrap().id;
            file.look().unwrap();
            arm(&signals);
            let err = if case == "recover" { file.recover(&target) } else { file.recover_removing(&target) }
                .unwrap_err();
            let id = paused_in(&signals).unwrap().generation.unwrap();
            late(&err, "ChangedAfterKeep", &id);
            assert_eq!(fs::read(&profile.config).unwrap(), external(), "the other editor's file stays");
            file.look().unwrap();
            if case == "recover" {
                file.recover(&target).unwrap();
                assert_eq!(
                    fs::read(&profile.config).unwrap(),
                    generation_bytes(&profile, &target.to_string())
                );
            } else {
                file.recover_removing(&target).unwrap();
                assert!(!profile.config.exists() && profile.config.parent().unwrap().is_dir());
            }
            let newest = file.last_generation().unwrap().to_string();
            assert_eq!(
                generation_bytes(&profile, &newest),
                external(),
                "the retry kept the other editor's file"
            );
            assert!(is_verified(&generation(&profile, &id)), "the retained generation stays verified");
        }
        other => panic!("unknown case {other}"),
    }
}
