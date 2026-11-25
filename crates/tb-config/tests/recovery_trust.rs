//! Recovery refuses untrusted stores and generations without modifying the profile.
#![allow(clippy::unwrap_used, clippy::panic)]

mod support {
    pub mod generations;
    pub mod recovery;
}

use std::fs::{self, Permissions};
use std::os::unix::fs::{PermissionsExt, symlink};

use support::generations::*;
use support::recovery::recover;
use tb_config::{Backups, ConfigFile, Loaded};

const SUITE: &str = "recovery_trust";

fn saved(test: &str) -> (Profile, ConfigFile) {
    let p = Profile::new(SUITE, test);
    fs::set_permissions(&p.dir, Permissions::from_mode(0o700)).unwrap();
    let mut file = ConfigFile::protected(&p.config, p.backups());
    assert_eq!(file.load(&theme()).unwrap(), Loaded::WroteDefault);
    file.save(relabelled("Revised"), &theme()).unwrap();
    (p, file)
}

#[test]
fn save_receipts_cannot_write_into_a_generation_through_any_path_to_it() {
    let (p, file) = saved("receipt");
    let absent = p.generations().into_iter().find(|g| !is_file_kind(g)).unwrap();
    let generation = generation_dir(&p.store, &absent);
    let alias = p.dir.join("alias");
    symlink(&generation, &alias).unwrap();
    let bytes = fs::read(&p.config).unwrap();

    for target in [generation.join("config.toml"), alias.join("config.toml")] {
        let err = file.write_save_receipt(&target).unwrap_err();
        assert_eq!(err.kind(), std::io::ErrorKind::InvalidInput);
        assert!(err.to_string().contains("generation store"), "{err}");
        assert!(!target.exists());
        assert_eq!(p.backups().verify(&absent.id).unwrap(), absent);
    }
    assert_eq!(fs::read(&p.config).unwrap(), bytes);

    let outside = p.dir.join("receipt.json");
    file.write_save_receipt(&outside).unwrap();
    assert!(outside.is_file());
}

#[test]
fn recovery_reads_reject_a_store_that_is_open_to_others() {
    let (p, file) = saved("public-store");
    let id = file.last_generation().unwrap().to_string();
    fs::set_permissions(&p.store, Permissions::from_mode(0o755)).unwrap();
    let before = snapshot(&p.dir);

    for args in [vec!["list"], vec!["verify", &id], vec!["restore", &id]] {
        let output = recover(&p, &args);
        output.exits(1);
        assert!(output.stderr.contains("chmod 700"), "{output:?}");
    }
    assert_eq!(snapshot(&p.dir), before);
}

#[test]
fn listing_and_verification_refuse_nonprivate_or_hard_linked_generation_members() {
    for (test, entry, expected) in [
        ("dir-mode", "", "other users"),
        ("manifest-mode", "manifest.toml", "other users"),
        ("data-mode", "config.toml", "other users"),
        ("manifest-link", "manifest.toml", "hard links"),
        ("data-link", "config.toml", "hard links"),
    ] {
        let (p, file) = saved(test);
        let id = *file.last_generation().unwrap();
        let name = p.store.join(id.to_string());
        let member = if entry.is_empty() { name } else { name.join(entry) };
        if test.ends_with("-link") {
            fs::hard_link(&member, p.dir.join("other-link")).unwrap();
        } else {
            let mode = if entry.is_empty() { 0o755 } else { 0o644 };
            fs::set_permissions(&member, Permissions::from_mode(mode)).unwrap();
        }
        let before = snapshot(&p.dir);

        let mut unknown = Vec::new();
        let list = p.backups().list_with_unreadable(&p.config, |id, why| unknown.push((id, why))).unwrap();
        assert!(list.iter().any(|g| g.id.to_string() == "1" && g.health == tb_config::Health::Verified));
        if entry == "config.toml" {
            assert_eq!(unknown, []);
            assert!(matches!(&list[0].health, tb_config::Health::Damaged(why) if why.contains(expected)));
        } else {
            assert!(list.iter().all(|g| g.id != id));
            assert!(matches!(unknown.as_slice(), [(found, why)] if *found == id && why.contains(expected)));
            let output = recover(&p, &["list"]);
            output.exits(0);
            assert!(output.stdout.contains(&format!("unreadable generation {id}")));
            assert!(output.stdout.contains("config unknown") && output.stdout.contains("1  first-write"));
        }
        let verify = p.backups().verify(&id).unwrap_err().to_string();
        assert!(verify.contains(expected), "{verify}");
        let restore = recover(&p, &["restore", &id.to_string()]);
        restore.exits(1);
        assert!(restore.stderr.contains(expected), "{restore:?}");
        assert_eq!(snapshot(&p.dir), before);
    }
}

#[test]
fn a_wrong_hash_of_an_absence_never_authorizes_removal() {
    let (p, file) = saved("absence-hash");
    let absent = p.generations().into_iter().find(|g| !is_file_kind(g)).unwrap();
    let manifest = generation_dir(&p.store, &absent).join("manifest.toml");
    let text = fs::read_to_string(&manifest)
        .unwrap()
        .replace("af1349b9f5f9a1a6a0404dea36dcc9499bcb25c9adc112b7cc9a93cae41f3262", &"0".repeat(64));
    assert!(text.contains(&"0".repeat(64)));
    fs::write(manifest, text).unwrap();
    let before = snapshot(&p.dir);

    let err = p.backups().verify(&absent.id).unwrap_err().to_string();
    assert!(err.contains("hash"), "{err}");
    let output = recover(&p, &["restore", &absent.id.to_string(), "--remove-file"]);
    output.exits(1);
    assert!(output.stderr.contains("hash"), "{output:?}");
    assert_eq!(fs::read(&p.config).unwrap(), tb_core::config::to_toml(file.config()).unwrap().as_bytes());
    assert_eq!(snapshot(&p.dir), before);
}

#[test]
fn checking_a_missing_store_never_creates_it() {
    let p = Profile::new(SUITE, "missing");
    let backup = Backups::at(&p.store);
    assert_eq!(backup.list(&p.config).unwrap(), []);
    assert_eq!(backup.incomplete().unwrap(), Vec::<String>::new());
    assert!(backup.verify(&tb_config::GenerationId::parse("1").unwrap()).is_err());
    assert!(!p.dir.join("state").exists());
}

#[test]
fn already_absent_recovery_refuses_a_dangling_config_link_without_touching_it() {
    let (p, _) = saved("dangling-absence");
    let absent = p.generations().into_iter().find(|g| !is_file_kind(g)).unwrap();
    fs::remove_file(&p.config).unwrap();
    symlink("missing.toml", &p.config).unwrap();
    let before = snapshot(&p.dir);

    let output = recover(&p, &["restore", &absent.id.to_string(), "--remove-file"]);
    output.exits(1);
    assert!(output.stderr.contains("symbolic link"), "{output:?}");
    assert_eq!(snapshot(&p.dir), before);
}

#[test]
fn already_absent_recovery_refuses_a_store_under_the_config_directory() {
    let (p, _) = saved("nested-absence");
    let absent = p.generations().into_iter().find(|g| !is_file_kind(g)).unwrap();
    fs::remove_file(&p.config).unwrap();
    fs::rename(&p.store, p.config.parent().unwrap().join("backups")).unwrap();
    let before = snapshot(&p.dir);
    let xdg = p.dir.join("home/.config");

    let output = support::recovery::recover_with_xdg(
        &p,
        xdg.to_str(),
        &["restore", &absent.id.to_string(), "--remove-file"],
    );
    output.exits(1);
    assert!(output.stderr.contains("inside the config directory"), "{output:?}");
    assert_eq!(snapshot(&p.dir), before);
}

#[test]
fn already_absent_recovery_refuses_a_public_store_without_creating_config_parents() {
    let (p, _) = saved("public-absence");
    let absent = p.generations().into_iter().find(|g| !is_file_kind(g)).unwrap();
    fs::remove_dir_all(p.dir.join("home")).unwrap();
    fs::set_permissions(&p.store, Permissions::from_mode(0o755)).unwrap();
    let before = snapshot(&p.dir);

    let output = recover(&p, &["restore", &absent.id.to_string(), "--remove-file"]);
    output.exits(1);
    assert!(output.stderr.contains("chmod 700"), "{output:?}");
    assert_eq!(snapshot(&p.dir), before);
}

#[test]
fn recovering_an_absence_that_already_holds_needs_no_new_directory_or_generation() {
    let (p, _) = saved("valid-absence");
    let absent = p.generations().into_iter().find(|g| !is_file_kind(g)).unwrap();
    fs::remove_dir_all(p.dir.join("home")).unwrap();
    let before = snapshot(&p.dir);

    let output = recover(&p, &["restore", &absent.id.to_string(), "--remove-file"]);
    output.exits(0);
    assert!(output.stdout.contains("already absent"), "{output:?}");
    assert_eq!(snapshot(&p.dir), before);
}

#[test]
fn the_cli_lists_an_unreadable_manifest_without_claiming_that_it_belongs_to_this_config() {
    let (p, file) = saved("unreadable");
    let id = file.last_generation().unwrap().to_string();
    fs::write(p.store.join(&id).join("manifest.toml"), b"format = [garbled").unwrap();
    let before = snapshot(&p.dir);

    let listed = recover(&p, &["list"]);
    listed.exits(0);
    assert!(listed.stdout.contains(&format!("unreadable generation {id}")), "{listed:?}");
    assert!(listed.stdout.contains("config unknown"), "{listed:?}");
    assert!(listed.stdout.contains("1  first-write"), "a valid generation is still listed: {listed:?}");
    let verify = recover(&p, &["verify", &id]);
    verify.exits(1);
    assert!(!verify.stdout.contains("restores it"), "{verify:?}");
    let restore = recover(&p, &["restore", &id]);
    restore.exits(1);
    assert_eq!(snapshot(&p.dir), before);
}

#[test]
fn post_restore_output_distinguishes_the_bundled_theme_from_the_active_theme() {
    let (p, file) = saved("fallback");
    let id = file.last_generation().unwrap().to_string();
    let fallback = recover(&p, &["restore", &id]);
    fallback.exits(0);
    assert!(fallback.stdout.contains("bundled Tokyo Night"), "{fallback:?}");
    assert!(fallback.stdout.contains("colors.toml"), "{fallback:?}");

    let colors = p.dir.join("home/.local/state/omarchy/current/theme/colors.toml");
    seed(&colors, include_bytes!("../../../fixtures/omarchy-themes/tokyo-night/colors.toml"), 0o600);
    let active = recover(&p, &["restore", &id]);
    active.exits(0);
    assert!(active.stdout.contains("active theme"), "{active:?}");
    assert!(!active.stdout.contains("bundled Tokyo Night"), "{active:?}");
}

#[test]
fn an_absence_restore_conflict_keeps_remove_file_in_the_retry_command() {
    use std::process::{Command, Stdio};
    use std::thread;
    use std::time::{Duration, Instant};

    let (p, _) = saved("removal-conflict");
    let absent = p.generations().into_iter().find(|g| !is_file_kind(g)).unwrap();
    let id = absent.id.to_string();
    let lock = fs::File::open(&p.store).unwrap();
    lock.lock().unwrap();
    let mut restore = Command::new(support::recovery::recover_bin())
        .args(["restore", &id, "--remove-file"])
        .env("HOME", p.dir.join("home"))
        .env("XDG_STATE_HOME", p.dir.join("state"))
        .env_remove("DISPLAY")
        .env_remove("WAYLAND_DISPLAY")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    let deadline = Instant::now() + Duration::from_secs(8);
    let pid = restore.id().to_string();
    while !fs::read_to_string("/proc/locks").unwrap().lines().any(|line| {
        let fields = line.split_whitespace().collect::<Vec<_>>();
        fields.get(1) == Some(&"->") && fields.get(5) == Some(&pid.as_str())
    }) {
        if restore.try_wait().unwrap().is_some() || Instant::now() >= deadline {
            let _ = restore.kill();
            panic!("the restore did not wait for the store lock: {:?}", restore.wait_with_output());
        }
        thread::sleep(Duration::from_millis(5));
    }
    fs::write(&p.config, b"# concurrent edit\n").unwrap();
    let before = snapshot(&p.dir);
    lock.unlock().unwrap();

    let output = restore.wait_with_output().unwrap();
    assert_eq!(output.status.code(), Some(3), "{output:?}");
    let diagnostic = String::from_utf8(output.stderr).unwrap();
    assert!(diagnostic.contains(&format!("`touchbar-recover restore {id} --remove-file`")), "{diagnostic}");
    assert!(diagnostic.contains("back up and remove the newer file"), "{diagnostic}");
    assert_eq!(snapshot(&p.dir), before);
}
