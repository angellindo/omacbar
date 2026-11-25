//! `touchbar-recover restore` when the last check of its protected write finds a conflict
//! (ADR-0015): the real binary, held by `support/syscall_pause.py` at one step while the test
//! changes the config like another editor; no product hook.
//!
//! - Before the `recover` generation is published (`Step::Stage`): exit 3 (conflict), nothing kept
//!   or written, no generation id claimed; running the same command again restores.
//! - After it was published (`Step::Publish`), the file changed: exit 3, stderr names the config,
//!   the retained generation id and the exact command to run again (with `--remove-file` when
//!   that is what was asked); stdout claims no restore. The other editor's file stays, the
//!   retained generation is verified and holds what the restore would have replaced; the printed
//!   command then restores and keeps the other editor's file.
//! - After it was published, the config became a symlink: exit 1 (refused), stderr names the
//!   retained id and the link; the link and its target stay.
//!
//! Profiles: `HOME=<dir>/home`, `XDG_STATE_HOME=<dir>/state`, no display, no GPU (slice 2 helpers).
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod support {
    pub mod generations;
    pub mod late_conflict;
    pub mod recovery;
}

use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::process::{Command, Stdio};

use support::generations::{
    Entry, Profile, entry, hand_written, is_verified, kept_bytes, mode, relabelled, theme,
};
use support::late_conflict::{
    Held, Step, arm, replace_by_hand, replace_with_link, run_held, signals_at, temps_in,
};
use support::recovery::{generation_holding, recover, three_versions};
use tb_config::{ConfigFile, Generation, Loaded, Reason};

const SUITE: &str = "recover_terminal_late_conflict";

fn external() -> Vec<u8> {
    hand_written(&relabelled("another editor"))
}

/// `touchbar-recover <args>` in `profile`, held at `step` of the restore's protected write while
/// `change` runs.
fn held(profile: &Profile, args: &[&str], step: Step, change: impl FnOnce()) -> Held {
    let signals = signals_at(&profile.dir.join("signals"));
    arm(&signals);
    let mut cmd = Command::new(support::recovery::recover_bin());
    cmd.args(args)
        .env("HOME", profile.dir.join("home"))
        .env("XDG_STATE_HOME", profile.dir.join("state"))
        .env_remove("DISPLAY")
        .env_remove("WAYLAND_DISPLAY")
        .env("VK_ICD_FILENAMES", profile.dir.join("no-gpu.json"))
        .stdin(Stdio::null());
    run_held(&cmd, &signals, &profile.store, step, |_| change())
}

fn generation(profile: &Profile, id: &str) -> Generation {
    profile
        .generations()
        .into_iter()
        .find(|g| g.id.to_string() == id)
        .unwrap_or_else(|| panic!("generation {id} is not listed; {:?}", profile.generations()))
}

/// stderr names the retained generation `id` and says nothing replaced the config.
fn names_retained(run: &Held, id: &str) {
    let stderr = run.stderr.to_lowercase();
    assert!(
        stderr.contains(&format!("generation {id} was retained")),
        "stderr names the retained id; {run:#?}"
    );
    assert!(!run.stdout.to_lowercase().contains("restored"), "no restore is claimed; {run:#?}");
}

fn retained(profile: &Profile, id: &str, bytes: &[u8]) {
    let g = generation(profile, id);
    assert!(is_verified(&g), "{g:?}");
    assert_eq!(g.reason, Reason::Recover, "{g:?}");
    assert_eq!(kept_bytes(&profile.store, &g), bytes, "it holds what the restore would have replaced");
    recover(profile, &["verify", id]).exits(0);
}

#[test]
fn a_change_before_the_restore_publishes_its_generation_is_a_conflict_that_keeps_nothing() {
    let profile = Profile::new(SUITE, "stage_restore");
    let history = three_versions(&profile, 0o640);
    let target = generation_holding(&profile, &history.a).id.to_string();
    let before = profile.generations();
    let config = profile.config.clone();
    let run = held(&profile, &["restore", &target], Step::Stage, || {
        fs::set_permissions(&config, fs::Permissions::from_mode(0o600)).unwrap();
    });
    run.held_at(Step::Stage);
    assert_eq!(run.code, Some(3), "a conflict exits 3; {run:#?}");
    assert!(!run.stderr.contains("retained"), "nothing was retained; {run:#?}");
    assert_eq!(fs::read(&profile.config).unwrap(), history.c);
    assert_eq!(mode(&profile.config), 0o600, "the other editor's change stays");
    assert_eq!(profile.generations(), before, "no generation is kept");
    assert_eq!(temps_in(&profile.store), Vec::<String>::new());

    recover(&profile, &["restore", &target]).exits(0);
    assert_eq!(fs::read(&profile.config).unwrap(), history.a, "the same command again restores");
}

#[test]
fn a_change_after_the_restore_published_its_generation_exits_3_with_the_id_and_the_retry() {
    let profile = Profile::new(SUITE, "publish_restore");
    let history = three_versions(&profile, 0o640);
    let target = generation_holding(&profile, &history.a).id.to_string();
    let config = profile.config.clone();
    let run = held(&profile, &["restore", &target], Step::Publish, || replace_by_hand(&config, &external()));
    let id = run.published();
    assert_eq!(run.code, Some(3), "a late conflict exits 3; {run:#?}");
    names_retained(&run, &id);
    assert!(run.stderr.contains(&profile.config.display().to_string()), "stderr names the config; {run:#?}");
    let retry = format!("touchbar-recover restore {target}");
    assert!(run.stderr.contains(&format!("`{retry}`")), "stderr gives the exact retry `{retry}`; {run:#?}");
    assert!(!run.stderr.contains("--remove-file"), "{run:#?}");
    assert_eq!(fs::read(&profile.config).unwrap(), external(), "the other editor's file stays");
    retained(&profile, &id, &history.c);

    recover(&profile, &["restore", &target]).exits(0);
    assert_eq!(fs::read(&profile.config).unwrap(), history.a, "the printed command restores");
    assert_eq!(mode(&profile.config), 0o640);
    let newest = &profile.generations()[0];
    assert_eq!(kept_bytes(&profile.store, newest), external(), "and keeps the other editor's file");
    retained(&profile, &id, &history.c);
}

#[test]
fn a_change_after_a_file_removal_published_its_generation_exits_3_with_the_id_and_the_retry() {
    let profile = Profile::new(SUITE, "publish_remove");
    let mut first = ConfigFile::protected(&profile.config, profile.backups());
    assert_eq!(first.load(&theme()).unwrap(), Loaded::WroteDefault);
    let written = fs::read(&profile.config).unwrap();
    let absence = profile.generations().last().unwrap().id.to_string();
    let config = profile.config.clone();
    let run = held(&profile, &["restore", &absence, "--remove-file"], Step::Publish, || {
        replace_by_hand(&config, &external());
    });
    let id = run.published();
    assert_eq!(run.code, Some(3), "a late conflict exits 3; {run:#?}");
    names_retained(&run, &id);
    let retry = format!("touchbar-recover restore {absence} --remove-file");
    assert!(run.stderr.contains(&format!("`{retry}`")), "stderr gives the exact retry `{retry}`; {run:#?}");
    assert!(run.stderr.contains("remove"), "it says the retry removes the newer file; {run:#?}");
    assert_eq!(fs::read(&profile.config).unwrap(), external(), "the file is not removed");
    retained(&profile, &id, &written);

    recover(&profile, &["restore", &absence, "--remove-file"]).exits(0);
    assert!(!profile.config.exists(), "the printed command removes the file");
    assert!(profile.config.parent().unwrap().is_dir(), "never its directory");
    assert_eq!(kept_bytes(&profile.store, &profile.generations()[0]), external(), "after keeping it");
}

#[test]
fn a_config_made_a_symlink_after_the_restore_published_its_generation_is_refused_with_the_id() {
    let profile = Profile::new(SUITE, "publish_symlink");
    let history = three_versions(&profile, 0o640);
    let target = generation_holding(&profile, &history.a).id.to_string();
    let elsewhere = profile.dir.join("elsewhere.toml");
    let (config, link_to) = (profile.config.clone(), elsewhere.clone());
    let run = held(&profile, &["restore", &target], Step::Publish, || {
        replace_with_link(&config, &link_to, &external());
    });
    let id = run.published();
    assert_eq!(run.code, Some(1), "an unsafe path is refused (exit 1); {run:#?}");
    names_retained(&run, &id);
    assert!(run.stderr.contains("symbolic link"), "stderr says why; {run:#?}");
    assert_eq!(entry(&profile.config), Some(Entry::Link { to: elsewhere.clone() }), "the link stays");
    assert_eq!(fs::read(&elsewhere).unwrap(), external(), "nothing is written through it");
    retained(&profile, &id, &history.c);
}
