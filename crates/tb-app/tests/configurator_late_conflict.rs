//! The configurator when the last check of a protected write finds a conflict (ADR-0015): Save,
//! the conflict's Overwrite, a factory reset's Save and a restore from the recovery list.
//!
//! - Before the generation is published (`Step::Stage`): not saved, "changed on disk", no
//!   generation kept or claimed; the draft and both conflict resolutions stay.
//! - After it was published (`Step::Publish`), the file changed: not saved / not restored, the
//!   status names the retained generation id, the other editor's file stays and the retained
//!   generation is listed (`recover-<id>`), verified, holding what the write would have replaced.
//!   A Save or Overwrite keeps the draft and offers `conflict-reload` / `conflict-overwrite`; a
//!   factory reset also keeps `reset-cancel`, which brings back the draft from before the reset and
//!   writes nothing; a restore shows the newer file. Overwrite (or tapping the restore again) then
//!   succeeds and keeps the other editor's file.
//! - After it was published, the config became a symlink: not saved / not restored, the status
//!   names the retained id and the link, the draft stays, the link and its target stay.
//!
//! The real configurator runs headless (`rune::Runtime`, as startup opens it with
//! `State::open_protected`) in a child (this test binary, one ignored helper) held by
//! `syscall_pause.py` at that step while the parent changes the config; no product hook. Each case
//! has its own temp profile and store under cargo's temp dir.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod support {
    pub mod configurator;
    #[path = "../../../tb-config/tests/support/late_conflict.rs"]
    pub mod late_conflict;
    pub mod ui;
}

use std::fs;
use std::os::unix::fs::{MetadataExt, PermissionsExt};
use std::path::{Path, PathBuf};

use rune::Runtime;
use support::configurator::{assert_clean, edit, fixture, fresh_dir, operate, runtime, shown, theme, words};
use support::late_conflict::{
    Held, Paused, Step, arm, helper_command, helper_passed, paused_in, replace_by_hand, replace_with_link,
    run_held, signals_at, temps_in,
};
use tb_app::State;
use tb_config::{Backups, ConfigFile, Generation, Health, Loaded, Reason};
use tb_core::Chord;
use tb_core::config::{Config, to_toml};

const SUITE: &str = "configurator_late_conflict";
const HELPER: &str = "late_conflict_configurator_helper";
const CASE: &str = "TB_LATE_CONFLICT_CASE";
const DIR: &str = "TB_LATE_CONFLICT_DIR";

struct Profile {
    dir: PathBuf,
    config: PathBuf,
    store: PathBuf,
}

impl Profile {
    fn at(dir: &Path) -> Self {
        Self {
            dir: dir.to_path_buf(),
            config: dir.join("home/.config/touchbar-customizer/config.toml"),
            store: dir.join("state/touchbar-customizer/backups"),
        }
    }

    /// A fresh profile whose config is `bytes`, written by hand with mode 0640.
    fn seeded(test: &str, bytes: &[u8]) -> Self {
        let profile = Self::at(&fresh_dir(SUITE, test));
        by_hand(&profile.config, bytes);
        profile
    }

    fn backups(&self) -> Backups {
        Backups::at(&self.store)
    }

    fn signals(&self) -> PathBuf {
        self.dir.join("signals")
    }

    fn generations(&self) -> Vec<Generation> {
        if !self.store.exists() {
            return Vec::new();
        }
        self.backups().list(&self.config).unwrap()
    }

    fn generation(&self, id: &str) -> Generation {
        self.generations()
            .into_iter()
            .find(|g| g.id.to_string() == id)
            .unwrap_or_else(|| panic!("generation {id} is not listed; {:?}", self.generations()))
    }

    fn kept(&self, id: &str) -> Vec<u8> {
        fs::read(self.store.join(id).join("config.toml")).unwrap()
    }

    /// The retained generation: listed, verified, of `reason`, holding exactly `bytes`.
    fn retained(&self, id: &str, reason: Reason, bytes: &[u8]) {
        let g = self.generation(id);
        assert_eq!(g.health, Health::Verified, "{g:?}");
        assert_eq!(g.reason, reason, "{g:?}");
        assert_eq!(self.kept(id), bytes, "generation {id} holds what the write would have replaced");
        self.backups().verify(&g.id).unwrap();
    }

    /// Runs the helper for `case`, held at `step`; `at_pause` is the other editor's change.
    fn held(&self, case: &str, step: Step, at_pause: impl FnOnce(&Paused)) -> Held {
        let signals = signals_at(&self.signals());
        let mut cmd = helper_command(HELPER, &[(DIR, &self.dir)]);
        cmd.env(CASE, case);
        let held = run_held(&cmd, &signals, &self.store, step, at_pause);
        helper_passed(&held);
        held
    }
}

fn by_hand(path: &Path, bytes: &[u8]) {
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, bytes).unwrap();
    fs::set_permissions(path, fs::Permissions::from_mode(0o640)).unwrap();
}

/// The fixture with button `a` of the default layer labelled `label`, as written by hand.
fn labelled(label: &str) -> (Config, Vec<u8>) {
    let mut config = fixture();
    label.clone_into(&mut config.layout.layer_mut(Chord::Default).unwrap().groups[0].buttons[0].label);
    let bytes = format!("# {label}, written by hand\n\n{}\n# keep this note\n", to_toml(&config).unwrap());
    (config, bytes.into_bytes())
}

fn mine() -> Vec<u8> {
    labelled("mine").1
}

fn external() -> Vec<u8> {
    labelled("another editor").1
}

const DRAFT: &str = "Draft label";

fn drafted() -> Vec<u8> {
    let mut config = labelled("mine").0;
    DRAFT.clone_into(&mut config.layout.layer_mut(Chord::Default).unwrap().groups[0].buttons[0].label);
    to_toml(&config).unwrap().into_bytes()
}

// ----- Save and Overwrite (slice 1) -----

#[test]
fn a_change_before_a_save_publishes_its_generation_keeps_nothing_and_keeps_the_draft() {
    let profile = Profile::seeded("stage_save", &mine());
    let config = profile.config.clone();
    let run = profile.held("save-stage", Step::Stage, |_| {
        fs::set_permissions(&config, fs::Permissions::from_mode(0o600)).unwrap();
    });
    run.held_at(Step::Stage);
    // The helper then overwrote: one generation, of the file with the other editor's mode.
    let generations = profile.generations();
    assert_eq!(generations.len(), 1, "only the overwrite kept a generation; {generations:?}");
    assert_eq!(generations[0].reason, Reason::Overwrite);
    assert_eq!(generations[0].mode, 0o600);
    assert_eq!(temps_in(&profile.store), Vec::<String>::new());
    assert_eq!(fs::read(&profile.config).unwrap(), drafted());
}

#[test]
fn a_change_after_a_save_published_its_generation_names_it_and_keeps_the_draft() {
    let profile = Profile::seeded("publish_save", &mine());
    let config = profile.config.clone();
    let run = profile.held("save", Step::Publish, |_| replace_by_hand(&config, &external()));
    let id = run.published();
    profile.retained(&id, Reason::Save, &mine());
    // The helper then overwrote the other editor's file, keeping it.
    assert_eq!(fs::read(&profile.config).unwrap(), drafted());
    let newest = &profile.generations()[0];
    assert_eq!((newest.reason, profile.kept(&newest.id.to_string())), (Reason::Overwrite, external()));
}

#[test]
fn a_change_after_an_overwrite_published_its_generation_names_it_and_keeps_the_draft() {
    let profile = Profile::seeded("publish_overwrite", &mine());
    let config = profile.config.clone();
    let later = labelled("a later editor").1;
    let kept_later = later.clone();
    let run = profile.held("overwrite", Step::Publish, move |_| replace_by_hand(&config, &later));
    let id = run.published();
    profile.retained(&id, Reason::Overwrite, &external());
    assert_eq!(fs::read(&profile.config).unwrap(), drafted(), "the next overwrite saved the draft");
    assert_eq!(profile.kept(&profile.generations()[0].id.to_string()), kept_later);
}

#[test]
fn a_config_made_a_symlink_after_a_save_published_its_generation_is_refused_with_the_id() {
    let profile = Profile::seeded("publish_symlink_save", &mine());
    let elsewhere = profile.dir.join("elsewhere.toml");
    let (config, to) = (profile.config.clone(), elsewhere.clone());
    let run = profile.held("save-unsafe", Step::Publish, |_| replace_with_link(&config, &to, &external()));
    let id = run.published();
    assert!(fs::symlink_metadata(&profile.config).unwrap().file_type().is_symlink(), "the link stays");
    assert_eq!(fs::read_link(&profile.config).unwrap(), elsewhere);
    assert_eq!(fs::read(&elsewhere).unwrap(), external(), "nothing is written through it");
    profile.retained(&id, Reason::Save, &mine());
}

// ----- A factory reset's Save (slice 3) -----

#[test]
fn a_change_after_a_factory_reset_save_published_its_generation_keeps_the_reset_cancellable() {
    let profile = Profile::seeded("publish_factory_reset", &mine());
    let config = profile.config.clone();
    let inode = fs::metadata(&profile.config).unwrap().ino();
    let run = profile.held("factory-reset", Step::Publish, |_| replace_by_hand(&config, &external()));
    let id = run.published();
    assert_eq!(profile.generations().len(), 1, "only the retained generation; {:?}", profile.generations());
    profile.retained(&id, Reason::FactoryReset, &mine());
    assert_eq!(fs::read(&profile.config).unwrap(), external(), "cancel wrote nothing");
    assert_ne!(fs::metadata(&profile.config).unwrap().ino(), inode);
}

// ----- A restore from the recovery list (slice 4) -----

/// v1 written by hand, then replaced by a protected save of v2: generation 1 holds v1 exactly.
fn with_history(test: &str) -> (Profile, Vec<u8>, Vec<u8>) {
    let (_, v1) = labelled("v1");
    let profile = Profile::seeded(test, &v1);
    let mut file = ConfigFile::protected(&profile.config, profile.backups());
    assert_eq!(file.load(&theme()).unwrap(), Loaded::Read);
    file.save(labelled("v2").0, &theme()).unwrap();
    let v2 = fs::read(&profile.config).unwrap();
    assert_eq!(profile.kept("1"), v1);
    (profile, v1, v2)
}

#[test]
fn a_change_after_a_restore_published_its_generation_names_it_and_shows_the_newer_file() {
    let (profile, v1, v2) = with_history("publish_restore");
    let config = profile.config.clone();
    let run = profile.held("restore", Step::Publish, |_| replace_by_hand(&config, &external()));
    let id = run.published();
    profile.retained(&id, Reason::Recover, &v2);
    assert_eq!(fs::read(&profile.config).unwrap(), v1, "tapping the restore again restored");
    assert_eq!(profile.kept(&profile.generations()[0].id.to_string()), external());
}

#[test]
fn a_config_made_a_symlink_after_a_restore_published_its_generation_is_refused_with_the_id() {
    let (profile, _, v2) = with_history("publish_symlink_restore");
    let elsewhere = profile.dir.join("elsewhere.toml");
    let (config, to) = (profile.config.clone(), elsewhere.clone());
    let run = profile.held("restore-unsafe", Step::Publish, |_| replace_with_link(&config, &to, &external()));
    let id = run.published();
    assert_eq!(fs::read_link(&profile.config).unwrap(), elsewhere, "the link stays");
    assert_eq!(fs::read(&elsewhere).unwrap(), external(), "nothing is written through it");
    profile.retained(&id, Reason::Recover, &v2);
}

// ----- the helper: the configurator, held by the supervisor -----

fn status(rt: &Runtime<State>) -> String {
    words(rt, "status").to_lowercase()
}

fn listed(rt: &Runtime<State>, key: &str) -> bool {
    rt.elements().iter().any(|e| e.key.as_deref() == Some(key) && !e.exiting)
}

/// Not saved, the other editor's change named, the retained id named, no success claimed.
fn refused_naming(rt: &Runtime<State>, id: &str, why: &str, not: &str) {
    let status = status(rt);
    assert!(status.starts_with(not), "{status}");
    assert!(status.contains(why), "{status}");
    assert!(
        status.contains(&format!("generation {id} was retained")),
        "the status names the retained id: {status}"
    );
    assert!(listed(rt, &format!("recover-{id}")), "the retained generation is listed");
}

fn draft_kept(rt: &Runtime<State>, draft: &Config) {
    assert_eq!(&rt.state.config, draft, "the draft is kept");
    assert!(words(rt, "dirty-summary").to_lowercase().contains("pending"), "and shown as pending");
}

fn resolutions_offered(rt: &Runtime<State>) {
    assert!(shown(rt, "conflict-reload"), "reload is offered");
    assert!(shown(rt, "conflict-overwrite"), "overwrite is offered");
}

#[test]
#[ignore = "run held by the late-conflict cases, in their child"]
fn late_conflict_configurator_helper() {
    let (Some(case), Some(dir)) = (std::env::var_os(CASE), std::env::var_os(DIR)) else { return };
    let profile = Profile::at(Path::new(&dir));
    let signals = profile.signals();
    let state = State::open_protected(Some(profile.config.clone()), Some(profile.backups()), theme());
    let mut rt = runtime(state, tb_app::WINDOW);
    let id = || paused_in(&signals).and_then(|p| p.generation).unwrap();
    match case.to_str().unwrap() {
        "save" | "save-stage" | "save-unsafe" => {
            operate(&mut rt, "row-a");
            edit(&mut rt, "field-label", DRAFT);
            let draft = rt.state.config.clone();
            arm(&signals);
            operate(&mut rt, "save");
            draft_kept(&rt, &draft);
            match case.to_str().unwrap() {
                "save-stage" => {
                    let status = status(&rt);
                    assert!(
                        status.starts_with("not saved") && status.contains("changed on disk"),
                        "{status}"
                    );
                    assert!(!status.contains("retained"), "nothing was retained: {status}");
                    resolutions_offered(&rt);
                }
                "save" => {
                    refused_naming(&rt, &id(), "changed on disk", "not saved");
                    resolutions_offered(&rt);
                }
                _ => {
                    refused_naming(&rt, &id(), "symbolic link", "not saved");
                    return;
                }
            }
            operate(&mut rt, "conflict-overwrite");
            assert!(status(&rt).starts_with("saved config"), "{}", status(&rt));
            assert_clean(&rt);
        }
        "overwrite" => {
            operate(&mut rt, "row-a");
            edit(&mut rt, "field-label", DRAFT);
            let draft = rt.state.config.clone();
            replace_by_hand(&profile.config, &external());
            operate(&mut rt, "save");
            resolutions_offered(&rt);
            arm(&signals);
            operate(&mut rt, "conflict-overwrite");
            refused_naming(&rt, &id(), "changed on disk", "not saved");
            draft_kept(&rt, &draft);
            resolutions_offered(&rt);
            operate(&mut rt, "conflict-overwrite");
            assert!(status(&rt).starts_with("saved config"), "{}", status(&rt));
            assert_clean(&rt);
        }
        "factory-reset" => {
            operate(&mut rt, "row-a");
            edit(&mut rt, "field-label", "Before the reset");
            let before = rt.state.config.clone();
            operate(&mut rt, "reset-factory");
            let factory = rt.state.config.clone();
            assert_eq!(factory.layout, Config::default_for(&rt.state.theme).layout);
            arm(&signals);
            operate(&mut rt, "save");
            refused_naming(&rt, &id(), "changed on disk", "not saved");
            assert!(
                !status(&rt).contains("before the factory reset is kept"),
                "no reset was saved: {}",
                status(&rt)
            );
            draft_kept(&rt, &factory);
            resolutions_offered(&rt);
            assert!(shown(&rt, "reset-cancel"), "the reset stays cancellable");
            operate(&mut rt, "reset-cancel");
            assert_eq!(rt.state.config, before, "cancel brings back the draft from before the reset");
            assert!(!shown(&rt, "reset-cancel"));
        }
        "restore" | "restore-unsafe" => {
            assert!(listed(&rt, "recover-1"));
            arm(&signals);
            operate(&mut rt, "recover-1");
            if case == "restore-unsafe" {
                refused_naming(&rt, &id(), "symbolic link", "not restored");
                return;
            }
            refused_naming(&rt, &id(), "changed on disk", "not restored");
            assert_eq!(rt.state.config, labelled("another editor").0, "the newer file is shown");
            assert_clean(&rt);
            operate(&mut rt, "recover-1");
            assert!(status(&rt).starts_with("restored"), "{}", status(&rt));
        }
        other => panic!("unknown case {other}"),
    }
}
