//! Terminal recovery refuses what it cannot restore exactly and safely, and a restore that cannot
//! finish leaves the config file as it was: ids that are not a generation number of the store
//! (paths, signs, leading zeros, unknown numbers), a generation directory linked in from outside
//! the store, a generation of another config, edited data, a garbled or unknown-format manifest,
//! data swapped for a symlink, and a symlinked config path are refused before anything is written,
//! with a diagnostic on stderr; damaged generations stay in the store. A store or config directory
//! that cannot be written, or no room for the restored file (EFBIG), abort the restore with the
//! original intact, and a retry once the cause is gone succeeds.
//!
//! Each test owns `<CARGO_TARGET_TMPDIR>/recover_terminal_refusals/<test>/`, `HOME=<dir>/home`,
//! `XDG_STATE_HOME=<dir>/state`; the real `touchbar-recover` binary runs with no display.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod support {
    pub mod generations;
    pub mod recovery;
}

use std::fs;
use std::os::unix::fs::symlink;
use std::path::Path;

use support::generations::*;
use support::recovery::*;
use tb_config::{ConfigFile, Health};

const SUITE: &str = "recover_terminal_refusals";

fn profile(test: &str) -> Profile {
    Profile::new(SUITE, test)
}

/// Copies one generation directory (manifest and data) to `to`.
fn copy_generation(from: &Path, to: &Path) {
    fs::create_dir_all(to).unwrap();
    chmod(to, 0o700);
    for entry in fs::read_dir(from).unwrap().flatten() {
        fs::copy(entry.path(), to.join(entry.file_name())).unwrap();
    }
}

/// Replaces `field = <old>` with `field = <new>` in a generation's manifest (asserting it changed).
fn rewrite_manifest(store: &Path, id: &str, field: &str, new: &str) {
    let path = store.join(id).join("manifest.toml");
    let text = fs::read_to_string(&path).unwrap();
    let edited: String = text
        .lines()
        .map(|l| {
            if l.split('=').next().unwrap().trim() == field {
                format!("{field} = {new}")
            } else {
                l.to_owned()
            }
        })
        .collect::<Vec<_>>()
        .join("\n")
        + "\n";
    assert_ne!(edited, text, "the manifest has a `{field}` field to rewrite");
    fs::write(path, edited).unwrap();
}

/// Restore and verify of `id` are both refused, naming `id`, and change nothing anywhere.
fn refuses_untouched(p: &Profile, id: &str) {
    let before = (state(p), snapshot(&p.dir.join("state")));
    let run = recover(p, &["verify", id]);
    run.refused();
    assert!(run.stderr.contains(id), "verify names generation {id}; {run:?}");
    let run = recover(p, &["restore", id]);
    run.refused();
    assert!(run.stderr.contains(id), "restore names generation {id}; {run:?}");
    assert_eq!((state(p), snapshot(&p.dir.join("state"))), before, "refusing {id} changed nothing");
}

#[test]
fn ids_that_are_not_a_generation_number_are_refused_and_touch_nothing() {
    let p = profile("ids");
    let h = three_versions(&p, 0o640);
    let a = generation_holding(&p, &h.a).id.to_string();
    // A copy of a good generation just outside the store, reachable only by a path.
    copy_generation(&p.store.join(&a), &p.dir.join("state/touchbar-customizer").join(&a));
    let before = (state(&p), snapshot(&p.dir.join("state")));
    let traversal = format!("../{a}");
    let padded = format!("0{a}");
    let signed = format!("+{a}");
    let absolute = p.store.join(&a).display().to_string();
    let spaced = format!(" {a}");
    for id in ["abc", "0", &padded, &signed, "-1", &traversal, &absolute, &spaced, "1.0", ""] {
        let run = recover(&p, &["restore", id]);
        run.refused();
        assert_eq!((state(&p), snapshot(&p.dir.join("state"))), before, "restore {id:?} changed nothing");
        let run = recover(&p, &["verify", id]);
        run.refused();
    }
    assert_eq!(fs::read(&p.config).unwrap(), h.c);
}

#[test]
fn an_unknown_generation_is_refused_naming_it() {
    let p = profile("unknown");
    three_versions(&p, 0o640);
    refuses_untouched(&p, "999");
}

#[test]
fn a_generation_directory_linked_from_outside_the_store_is_refused() {
    let p = profile("linked-dir");
    let h = three_versions(&p, 0o640);
    let a = generation_holding(&p, &h.a).id.to_string();
    let outside = p.dir.join("elsewhere/77");
    copy_generation(&p.store.join(&a), &outside);
    rewrite_manifest(&p.dir.join("elsewhere"), "77", "id", "77");
    symlink(&outside, p.store.join("77")).unwrap();
    refuses_untouched(&p, "77");
    assert_eq!(fs::read(&p.config).unwrap(), h.c);
}

#[test]
fn a_generation_of_another_config_is_refused() {
    let p = profile("foreign");
    let h = three_versions(&p, 0o640);
    let other = p.other_config("other");
    seed(&other, &hand_written(&relabelled("other")), 0o600);
    let mut file = ConfigFile::protected(&other, p.backups());
    file.load(&theme()).unwrap();
    file.save(relabelled("other-saved"), &theme()).unwrap();
    let foreign = p.generations_of(&other)[0].id.to_string();
    refuses_untouched(&p, &foreign);
    assert_eq!(fs::read(&p.config).unwrap(), h.c);
    let run = recover(&p, &["restore", &foreign]);
    assert!(
        run.stderr.contains(&other.display().to_string())
            || run.stderr.contains(&p.config.display().to_string()),
        "the refusal says which config the generation belongs to; {run:?}"
    );
}

#[test]
fn edited_generation_data_is_reported_damaged_and_never_restored() {
    let p = profile("edited");
    let h = three_versions(&p, 0o640);
    let a = generation_holding(&p, &h.a).id.to_string();
    let data = p.store.join(&a).join("config.toml");
    let mut edited = h.a.clone();
    edited[2] ^= 0x20; // same length, one byte changed
    fs::write(&data, &edited).unwrap();

    let line = recover(&p, &["list"]).exits(0).line_of(&a).to_owned();
    assert!(words(&line).any(|w| w == "damaged"), "list marks generation {a} damaged: {line:?}");
    refuses_untouched(&p, &a);
    assert_eq!(fs::read(&data).unwrap(), edited, "the damaged generation is kept as found");
}

#[test]
fn a_garbled_or_unknown_format_manifest_is_refused_and_kept() {
    let p = profile("manifest");
    let h = three_versions(&p, 0o640);
    let (a, b) = (generation_holding(&p, &h.a).id.to_string(), generation_holding(&p, &h.b).id.to_string());
    fs::write(p.store.join(&a).join("manifest.toml"), b"format = 1\nid = [garbled\n").unwrap();
    rewrite_manifest(&p.store, &b, "format", "99");
    refuses_untouched(&p, &a);
    refuses_untouched(&p, &b);
    assert!(p.store.join(&a).join("config.toml").is_file(), "the generation is kept, not removed");
    assert_eq!(fs::read(p.store.join(&b).join("config.toml")).unwrap(), h.b);
}

#[test]
fn generation_data_swapped_for_a_symlink_is_refused() {
    let p = profile("data-link");
    let h = three_versions(&p, 0o640);
    let a = generation_holding(&p, &h.a).id.to_string();
    let twin = p.dir.join("twin.toml");
    fs::write(&twin, &h.a).unwrap();
    let data = p.store.join(&a).join("config.toml");
    fs::remove_file(&data).unwrap();
    symlink(&twin, &data).unwrap();
    refuses_untouched(&p, &a);
    assert!(fs::symlink_metadata(&data).unwrap().file_type().is_symlink());
}

#[test]
fn a_symlinked_config_is_listed_but_never_restored_over() {
    let p = profile("config-link");
    let h = three_versions(&p, 0o640);
    let a = generation_holding(&p, &h.a);
    let target = p.dir.join("dotfiles/touchbar.toml");
    seed(&target, &h.c, 0o600);
    fs::remove_file(&p.config).unwrap();
    symlink(&target, &p.config).unwrap();

    recover(&p, &["list"]).exits(0).line_of(&a.id.to_string());
    let before = (state(&p), entry(&target));
    let run = recover(&p, &["restore", &a.id.to_string()]);
    run.refused();
    assert!(run.stderr.contains(&p.config.display().to_string()), "the refusal names the link; {run:?}");
    assert_eq!((state(&p), entry(&target)), before, "the link, its target and the store are unchanged");
}

#[test]
fn an_unwritable_store_refuses_the_restore_and_a_retry_succeeds() {
    let p = profile("store-readonly");
    let h = three_versions(&p, 0o640);
    if running_as_root(&p.dir) {
        return;
    }
    let a = generation_holding(&p, &h.a).id.to_string();
    let before = state(&p);
    chmod(&p.store, 0o500);
    let run = recover(&p, &["restore", &a]);
    chmod(&p.store, 0o700);
    run.exits(1);
    assert!(run.stderr.contains(&p.store.display().to_string()), "the diagnostic names the store; {run:?}");
    assert_eq!(state(&p), before, "the file and the store are as they were");

    recover(&p, &["restore", &a]).exits(0);
    assert_eq!(fs::read(&p.config).unwrap(), h.a);
    assert_eq!(kept(&p.store, &only_recover_generation(&p.store)), h.c);
}

#[test]
fn an_unwritable_config_directory_keeps_the_original_and_a_retry_succeeds() {
    let p = profile("dir-readonly");
    let h = three_versions(&p, 0o640);
    if running_as_root(&p.dir) {
        return;
    }
    let a = generation_holding(&p, &h.a).id.to_string();
    let dir = p.config.parent().unwrap().to_path_buf();
    chmod(&dir, 0o500);
    let before = entry(&p.config);
    let run = recover(&p, &["restore", &a]);
    chmod(&dir, 0o700);
    run.exits(1);
    assert_eq!(entry(&p.config), before, "the original file is untouched");
    assert_eq!(names_in(&dir), ["config.toml"], "no temp file is left beside it");
    for g in p.generations() {
        assert_eq!(g.health, Health::Verified, "every generation still verifies");
    }

    recover(&p, &["restore", &a]).exits(0);
    assert_eq!(fs::read(&p.config).unwrap(), h.a);
    assert!(
        recover_generations(&p.store).iter().any(|id| kept(&p.store, id) == h.c),
        "the file the restore replaced is kept"
    );
}

#[test]
fn no_room_for_the_restored_file_keeps_the_original() {
    let p = profile("no-room");
    // A large hand-written original (comments pad it past the cap), then a small saved file.
    let mut big = hand_written(&relabelled("big"));
    while big.len() < 300 * 1024 {
        big.extend_from_slice(b"# a long note the person keeps in their config file\n");
    }
    p.seed(&big, 0o640);
    let mut file = ConfigFile::protected(&p.config, p.backups());
    file.load(&theme()).unwrap();
    file.save(relabelled("small"), &theme()).unwrap();
    let small = fs::read(&p.config).unwrap();
    assert!(small.len() < 16 * 1024);
    let id = generation_holding(&p, &big).id.to_string();
    let before = entry(&p.config);

    let run = recover_size_limited(&p, 64 * 1024, &["restore", &id]);
    run.exits(1);
    assert!(run.stderr.contains("os error 27"), "the diagnostic carries the EFBIG cause; {run:?}");
    assert_eq!(entry(&p.config), before, "the original file is untouched");
    assert_eq!(kept_bytes(&p.store, &generation_holding(&p, &big)), big, "the chosen generation is intact");

    recover(&p, &["restore", &id]).exits(0);
    assert_eq!(fs::read(&p.config).unwrap(), big);
    assert_eq!(mode(&p.config), 0o640);
}
