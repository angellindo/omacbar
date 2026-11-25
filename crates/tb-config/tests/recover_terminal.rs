//! Terminal recovery: the real `touchbar-recover` binary (no Rune, no GPU, no display) lists the
//! generations of the profile's config, verifies one, and restores a chosen one byte for byte
//! (comments included) with its permission bits, through the same compare-and-swap as every
//! other write. A restore first keeps a `recover` generation of what it replaces; it restores into
//! a missing file and a missing directory; going back to "no file" needs `--remove-file`; a file
//! changed while the restore waits is a conflict (exit 3) that writes nothing and keeps both
//! versions. Every run is a new process, so what one run keeps another one restores.
//!
//! Each test owns `<CARGO_TARGET_TMPDIR>/recover_terminal/<test>/` with `HOME=<dir>/home` and
//! `XDG_STATE_HOME=<dir>/state`: the store is never inside the profile tree a restore replaces.
//! Exit codes: 0 done, 1 refused or failed, 2 usage, 3 conflict.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod support {
    pub mod generations;
    pub mod recovery;
}

use std::fs::{self, File};
use std::process::Stdio;
use std::thread;
use std::time::{Duration, Instant};

use support::generations::*;
use support::recovery::*;
use tb_config::{ConfigFile, GenerationKind, Health, Loaded};

const SUITE: &str = "recover_terminal";
const WAIT: Duration = Duration::from_secs(60);

fn profile(test: &str) -> Profile {
    Profile::new(SUITE, test)
}

#[test]
fn list_names_the_store_and_every_generation_of_this_config_with_its_health() {
    let p = profile("list");
    let h = three_versions(&p, 0o640);
    let (a, b) = (generation_holding(&p, &h.a), generation_holding(&p, &h.b));

    let run = recover(&p, &["list"]);
    run.exits(0);
    assert!(run.stdout.contains(&p.store.display().to_string()), "list names the store; {run:?}");
    for g in [&a, &b] {
        let line = run.line_of(&g.id.to_string());
        let said: Vec<&str> = words(line).collect();
        for word in ["save", "file", "verified"] {
            assert!(said.contains(&word), "the line of generation {} says {word:?}: {line:?}", g.id);
        }
    }

    let run = recover(&p, &["verify", &a.id.to_string()]);
    run.exits(0);
    assert!(run.stdout.contains(&a.id.to_string()), "verify names the generation; {run:?}");
}

#[test]
fn list_and_verify_work_without_a_config_a_store_or_a_display_and_create_nothing() {
    let p = profile("nothing-yet");
    let before = snapshot(&p.dir);
    let run = recover(&p, &["list"]);
    run.exits(0);
    assert!(
        run.stdout.contains(&p.store.display().to_string()),
        "list names where the store would be; {run:?}"
    );
    assert_eq!(snapshot(&p.dir), before, "listing created nothing (no store, no config directory)");

    let run = recover(&p, &["verify", "1"]);
    run.exits(1);
    assert!(run.stderr.contains('1'), "the refusal names the generation; {run:?}");
    assert_eq!(snapshot(&p.dir), before, "verifying created nothing");
}

#[test]
fn the_store_follows_home_when_xdg_state_home_is_unset_or_relative() {
    let p = profile("home-store");
    let home_store = p.dir.join("home/.local/state/touchbar-customizer/backups");
    // Seed a generation in the HOME store through the library, as a binary without XDG would.
    let config = p.config.clone();
    seed(&config, &hand_written(&relabelled("first")), 0o600);
    let mut file = ConfigFile::protected(&config, tb_config::Backups::at(&home_store));
    assert_eq!(file.load(&theme()).unwrap(), Loaded::Read);
    file.save(relabelled("second"), &theme()).unwrap();
    let id = tb_config::Backups::at(&home_store).list(&config).unwrap()[0].id.to_string();

    for xdg in [None, Some("relative/state"), Some("")] {
        let run = recover_with_xdg(&p, xdg, &["list"]);
        run.exits(0);
        assert!(
            run.stdout.contains(&home_store.display().to_string()),
            "XDG_STATE_HOME={xdg:?}: the store is under HOME; {run:?}"
        );
        run.line_of(&id);
    }
    let run = recover_with_xdg(&p, None, &["restore", &id]);
    run.exits(0);
    assert_eq!(fs::read(&config).unwrap(), hand_written(&relabelled("first")));
    assert!(!p.dir.join("relative").exists(), "a relative XDG_STATE_HOME is ignored, never created");
}

#[test]
fn restore_brings_back_the_chosen_generation_byte_for_byte_with_its_mode() {
    let p = profile("exact");
    let h = three_versions(&p, 0o640);
    let a = generation_holding(&p, &h.a);
    assert!(h.a.starts_with(b"# my touch bar"), "the chosen version carries hand-written comments");

    let run = recover(&p, &["restore", &a.id.to_string()]);
    run.exits(0);
    assert_eq!(fs::read(&p.config).unwrap(), h.a, "the older generation, not the newest, comes back exactly");
    assert_eq!(mode(&p.config), 0o640, "with the permission bits it had");
    assert!(fs::symlink_metadata(&p.config).unwrap().file_type().is_file());
    let chosen = p.backups().verify(&a.id).unwrap();
    assert_eq!(chosen.health, Health::Verified, "the restored generation stays in the store");
}

#[test]
fn restore_keeps_a_generation_of_the_file_it_replaces() {
    let p = profile("keeps-current");
    let h = three_versions(&p, 0o640);
    let a = generation_holding(&p, &h.a);
    let before: Vec<_> = p.generations().into_iter().map(|g| g.id).collect();

    recover(&p, &["restore", &a.id.to_string()]).exits(0);
    let id = only_recover_generation(&p.store);
    assert_eq!(kept(&p.store, &id), h.c, "the replaced file is kept exactly");
    assert_eq!(manifest_mode(&p.store, &id), 0o640, "with its permission bits");
    assert_eq!(manifest(&p.store, &id)["kind"], "file");
    let listed = p.generations();
    assert_eq!(listed[0].id.to_string(), id, "it is the newest generation of this config");
    assert_eq!(listed[0].health, Health::Verified);
    for old in before {
        assert!(listed.iter().any(|g| g.id == old), "generation {old} is still there");
    }
}

#[test]
fn restore_works_over_an_invalid_config_and_keeps_its_exact_bytes() {
    let p = profile("over-invalid");
    let h = three_versions(&p, 0o600);
    let a = generation_holding(&p, &h.a);
    let broken = b"# half edited\n[layout\nnot = [valid TOML\n".to_vec();
    external_replace(&p.config, &broken);

    recover(&p, &["restore", &a.id.to_string()]).exits(0);
    assert_eq!(fs::read(&p.config).unwrap(), h.a);
    let id = only_recover_generation(&p.store);
    assert_eq!(kept(&p.store, &id), broken, "the invalid file it replaced is kept as it was");
}

#[test]
fn restore_into_a_missing_file_creates_it_and_records_the_absence() {
    let p = profile("missing-file");
    let h = three_versions(&p, 0o640);
    let a = generation_holding(&p, &h.a);
    fs::remove_file(&p.config).unwrap();

    recover(&p, &["restore", &a.id.to_string()]).exits(0);
    assert_eq!(fs::read(&p.config).unwrap(), h.a);
    assert_eq!(mode(&p.config), 0o640);
    let id = only_recover_generation(&p.store);
    let recorded = p.backups().verify(&p.generations()[0].id).unwrap();
    assert_eq!(recorded.id.to_string(), id);
    assert_eq!(recorded.kind, GenerationKind::Absent { dir_existed: true }, "no file, directory present");
}

#[test]
fn restore_into_a_missing_directory_creates_it_and_records_that_it_was_missing() {
    let p = profile("missing-dir");
    let h = three_versions(&p, 0o640);
    let a = generation_holding(&p, &h.a);
    let dir = p.config.parent().unwrap().to_path_buf();
    fs::remove_dir_all(&dir).unwrap();

    recover(&p, &["restore", &a.id.to_string()]).exits(0);
    assert!(dir.is_dir(), "the config directory is created again");
    assert_eq!(fs::read(&p.config).unwrap(), h.a);
    assert_eq!(mode(&p.config), 0o640);
    let id = only_recover_generation(&p.store);
    assert_eq!(p.generations()[0].id.to_string(), id);
    assert_eq!(p.generations()[0].kind, GenerationKind::Absent { dir_existed: false });
}

#[test]
fn returning_to_no_file_needs_remove_file_and_keeps_the_removed_file() {
    let p = profile("remove-file");
    // The first write of a missing file keeps an absence, then writes the default.
    let mut file = ConfigFile::protected(&p.config, p.backups());
    assert_eq!(file.load(&theme()).unwrap(), Loaded::WroteDefault);
    let absent = p.generations()[0].clone();
    assert!(matches!(absent.kind, GenerationKind::Absent { .. }));
    let default = fs::read(&p.config).unwrap();
    let file_mode = mode(&p.config);
    let before = state(&p);

    let run = recover(&p, &["restore", &absent.id.to_string()]);
    run.refused();
    assert!(run.stderr.contains("--remove-file"), "the refusal names the flag that allows it; {run:?}");
    assert_eq!(state(&p), before, "without the flag nothing is written or kept");

    recover(&p, &["restore", &absent.id.to_string(), "--remove-file"]).exits(0);
    assert!(fs::symlink_metadata(&p.config).is_err(), "the config file is gone");
    assert!(p.config.parent().unwrap().is_dir(), "its directory is never removed");
    let id = only_recover_generation(&p.store);
    assert_eq!(kept(&p.store, &id), default, "the removed file is kept exactly");
    assert_eq!(manifest_mode(&p.store, &id), file_mode & 0o777);
}

/// Pids `/proc/locks` lists as blocked waiting for a lock.
fn blocked_waiters() -> Vec<u32> {
    fs::read_to_string("/proc/locks")
        .unwrap()
        .lines()
        .filter(|l| l.contains("->"))
        .filter_map(|l| l.split_whitespace().nth(5).and_then(|pid| pid.parse().ok()))
        .collect()
}

#[test]
fn a_change_while_restore_waits_is_a_conflict_that_writes_nothing() {
    let p = profile("conflict");
    let h = three_versions(&p, 0o640);
    let a = generation_holding(&p, &h.a);
    let ids_before: Vec<_> = p.generations().into_iter().map(|g| g.id).collect();

    // Hold the store lock and the config-directory lock, as another writer would, so the restore
    // has looked at the file and then waits; change the file meanwhile, then let it go.
    let store_lock = File::open(&p.store).unwrap();
    store_lock.lock().unwrap();
    let dir_lock = File::open(p.config.parent().unwrap()).unwrap();
    dir_lock.lock().unwrap();
    let mut child = std::process::Command::new(recover_bin())
        .args(["restore", &a.id.to_string()])
        .env("HOME", p.dir.join("home"))
        .env("XDG_STATE_HOME", p.dir.join("state"))
        .env_remove("DISPLAY")
        .env_remove("WAYLAND_DISPLAY")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    let deadline = Instant::now() + WAIT;
    while !blocked_waiters().contains(&child.id()) {
        if let Some(status) = child.try_wait().unwrap() {
            let out = child.wait_with_output().unwrap();
            panic!("restore ended ({status}) without waiting for a writer's lock: {out:?}");
        }
        assert!(Instant::now() < deadline, "restore never waited for the held locks");
        thread::sleep(Duration::from_millis(10));
    }
    let theirs = hand_written(&relabelled("theirs"));
    external_replace(&p.config, &theirs);
    drop(dir_lock);
    drop(store_lock);
    let out = child.wait_with_output().unwrap();

    assert_eq!(out.status.code(), Some(3), "a conflict exits 3: {out:?}");
    assert!(!out.stderr.is_empty(), "it says what happened: {out:?}");
    assert_eq!(fs::read(&p.config).unwrap(), theirs, "the newer file stays");
    assert!(recover_generations(&p.store).is_empty(), "nothing was kept for a write that did not happen");
    let ids_after: Vec<_> = p.generations().into_iter().map(|g| g.id).collect();
    assert_eq!(ids_after, ids_before);
    assert_eq!(kept_bytes(&p.store, &a), h.a, "the chosen generation is untouched");

    // Run again: now it restores, and the newer file is kept.
    recover(&p, &["restore", &a.id.to_string()]).exits(0);
    assert_eq!(fs::read(&p.config).unwrap(), h.a);
    assert_eq!(kept(&p.store, &only_recover_generation(&p.store)), theirs);
}

#[test]
fn a_restored_generation_can_be_restored_again_by_a_later_run() {
    let p = profile("again");
    let h = three_versions(&p, 0o640);
    let (a, b) = (generation_holding(&p, &h.a), generation_holding(&p, &h.b));

    recover(&p, &["restore", &a.id.to_string()]).exits(0);
    recover(&p, &["restore", &b.id.to_string()]).exits(0);
    assert_eq!(fs::read(&p.config).unwrap(), h.b);
    recover(&p, &["restore", &a.id.to_string()]).exits(0);
    assert_eq!(fs::read(&p.config).unwrap(), h.a);
    assert_eq!(mode(&p.config), 0o640);
    for g in [&a, &b] {
        assert_eq!(p.backups().verify(&g.id).unwrap().health, Health::Verified);
    }
}

#[test]
fn usage_errors_exit_2_and_touch_nothing() {
    let p = profile("usage");
    let h = three_versions(&p, 0o640);
    let a = generation_holding(&p, &h.a).id.to_string();
    let before = state(&p);
    let cases: [&[&str]; 8] = [
        &[],
        &["frobnicate"],
        &["restore"],
        &["verify"],
        &["list", "extra"],
        &["restore", &a, "--bogus"],
        &["--remove-file"],
        &["restore", &a, &a],
    ];
    for args in cases {
        let run = recover(&p, args);
        assert_eq!(run.code, Some(2), "{args:?} is a usage error; {run:?}");
        assert!(!run.stderr.trim().is_empty(), "{args:?}: usage is printed on stderr; {run:?}");
        assert_eq!(state(&p), before, "{args:?} changed nothing");
    }
    assert_eq!(fs::read(&p.config).unwrap(), h.c);
}
