//! A protected write refuses, before anything is written, every path it cannot keep and restore
//! exactly: a symlinked config (dangling or not), a FIFO (without blocking on it), a socket, a
//! directory, a hard-linked file; and every store it cannot trust: a `backups` directory readable
//! by others, a symlinked `backups` (even one swapped in after generations exist), a store inside
//! the config directory that a restore would replace, and a store it cannot write. On a refusal
//! the config, its link or target, and every earlier generation stay byte for byte, no generation
//! is added, and the diagnostic names the path. A symlinked parent directory loses nothing.
//! Reads still follow a symlinked profile, so existing profiles stay usable.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod support {
    pub mod generations;
}

use std::fs::{self, OpenOptions, Permissions};
use std::os::unix::fs::{OpenOptionsExt, PermissionsExt, symlink};
use std::os::unix::net::UnixListener;
use std::sync::mpsc::channel;
use std::thread;
use std::time::Duration;

use support::generations::*;
use tb_config::{Backups, ConfigFile, ConfigFileError, Loaded};
use tb_core::config::to_toml;

const SUITE: &str = "config_generation_refusals";
const WAIT: Duration = Duration::from_secs(10);

fn protected(profile: &Profile) -> ConfigFile {
    ConfigFile::protected(&profile.config, profile.backups())
}

fn is_symlink_refusal(result: &Result<impl std::fmt::Debug, ConfigFileError>) -> bool {
    matches!(result, Err(ConfigFileError::Symlink { .. }))
}

fn is_unsupported(result: &Result<impl std::fmt::Debug, ConfigFileError>) -> bool {
    matches!(result, Err(ConfigFileError::Unsupported { .. }))
}

fn is_backup_refusal(result: &Result<impl std::fmt::Debug, ConfigFileError>) -> bool {
    matches!(result, Err(ConfigFileError::Backup(_)))
}

fn message(result: &Result<impl std::fmt::Debug, ConfigFileError>) -> String {
    result.as_ref().err().map(ToString::to_string).unwrap_or_default()
}

#[test]
fn a_symlinked_config_is_shown_but_never_written_through_or_replaced() {
    let p = Profile::new(SUITE, "symlinked_config");
    let target = p.dir.join("dotfiles/touchbar.toml");
    seed(&target, &hand_written(&relabelled("Linked")), 0o600);
    fs::create_dir_all(p.config.parent().unwrap()).unwrap();
    symlink(&target, &p.config).unwrap();
    let before = snapshot(&p.dir);

    let mut file = protected(&p);
    assert_eq!(file.load(&theme()).unwrap(), Loaded::Read, "an existing symlinked profile still loads");
    file.reload_resolved(&theme()).unwrap();
    let saved = file.save(relabelled("Next"), &theme());

    assert!(is_symlink_refusal(&saved), "{saved:?}");
    assert!(message(&saved).contains(&p.config.display().to_string()), "{}", message(&saved));
    assert_eq!(file.config(), &relabelled("Linked"), "the shown config is still the linked one");
    assert_eq!(snapshot(&p.dir), before, "link, target and store untouched");
}

#[test]
fn a_dangling_symlink_is_not_replaced_by_the_first_default() {
    let p = Profile::new(SUITE, "dangling");
    fs::create_dir_all(p.config.parent().unwrap()).unwrap();
    symlink(p.dir.join("gone/config.toml"), &p.config).unwrap();
    let before = snapshot(&p.dir);

    let loaded = protected(&p).load(&theme());

    assert!(is_symlink_refusal(&loaded), "{loaded:?}");
    assert_eq!(snapshot(&p.dir), before);
}

#[test]
fn a_hard_linked_config_is_refused_and_both_names_keep_their_bytes() {
    let p = Profile::new(SUITE, "hard_link");
    let mine = hand_written(&relabelled("Mine"));
    p.seed(&mine, 0o600);
    fs::hard_link(&p.config, p.dir.join("second-name.toml")).unwrap();
    let mut file = protected(&p);
    assert_eq!(file.load(&theme()).unwrap(), Loaded::Read);
    let before = snapshot(&p.dir);

    let saved = file.save(relabelled("Next"), &theme());

    assert!(is_unsupported(&saved), "{saved:?}");
    assert!(message(&saved).contains(&p.config.display().to_string()), "{}", message(&saved));
    assert_eq!(snapshot(&p.dir), before);
}

/// Runs `write` on its own thread; a write that blocks (on a FIFO) fails the test after `WAIT`.
fn within_wait<T: Send + 'static>(write: impl FnOnce() -> T + Send + 'static) -> T {
    let (tx, rx) = channel();
    thread::spawn(move || {
        let _ = tx.send(write());
    });
    rx.recv_timeout(WAIT).unwrap_or_else(|_| panic!("the protected write blocked instead of refusing"))
}

#[test]
fn a_fifo_at_the_config_path_is_refused_without_blocking() {
    let p = Profile::new(SUITE, "fifo");
    fs::create_dir_all(p.config.parent().unwrap()).unwrap();
    rustix::fs::mknodat(rustix::fs::CWD, &p.config, rustix::fs::FileType::Fifo, 0o600.into(), 0).unwrap();
    let before = snapshot(&p.dir);
    let (config, store) = (p.config.clone(), p.store.clone());

    let loaded = within_wait(move || ConfigFile::protected(&config, Backups::at(&store)).load(&theme()));
    // Release a reader that may still be blocked in open (a no-op otherwise).
    let _ = OpenOptions::new().write(true).custom_flags(libc_nonblock()).open(&p.config);

    assert!(is_unsupported(&loaded), "{loaded:?}");
    assert_eq!(snapshot(&p.dir), before);
}

/// `O_NONBLOCK` on Linux.
const fn libc_nonblock() -> i32 {
    0o4000
}

#[test]
fn a_socket_or_a_directory_at_the_config_path_is_refused() {
    let p = Profile::new(SUITE, "socket");
    fs::create_dir_all(p.config.parent().unwrap()).unwrap();
    let _listener = UnixListener::bind(&p.config).unwrap();
    let before = snapshot(&p.dir);
    let (config, store) = (p.config.clone(), p.store.clone());
    let socket = within_wait(move || ConfigFile::protected(&config, Backups::at(&store)).load(&theme()));
    assert!(is_unsupported(&socket), "{socket:?}");
    assert_eq!(snapshot(&p.dir), before);

    let d = Profile::new(SUITE, "directory");
    fs::create_dir_all(&d.config).unwrap();
    fs::write(d.config.join("inside"), b"keep").unwrap();
    let before = snapshot(&d.dir);
    let dir = protected(&d).load(&theme());
    assert!(is_unsupported(&dir), "{dir:?}");
    assert_eq!(snapshot(&d.dir), before);
}

#[test]
fn a_store_readable_by_others_is_refused_before_the_file_changes() {
    let p = Profile::new(SUITE, "store_open");
    let mine = hand_written(&relabelled("Mine"));
    p.seed(&mine, 0o600);
    fs::create_dir_all(&p.store).unwrap();
    fs::set_permissions(&p.store, Permissions::from_mode(0o755)).unwrap();
    let mut file = protected(&p);
    assert_eq!(file.load(&theme()).unwrap(), Loaded::Read);
    let before = snapshot(&p.dir);

    let saved = file.save(relabelled("Next"), &theme());

    assert!(is_backup_refusal(&saved), "{saved:?}");
    assert!(message(&saved).contains(&p.store.display().to_string()), "{}", message(&saved));
    assert_eq!(snapshot(&p.dir), before, "nothing written, the store's mode is not changed for the user");
}

#[test]
fn a_symlinked_store_swapped_in_after_generations_exist_is_refused_and_loses_nothing() {
    let p = Profile::new(SUITE, "store_swapped");
    p.seed(&hand_written(&relabelled("Mine")), 0o600);
    let mut file = protected(&p);
    assert_eq!(file.load(&theme()).unwrap(), Loaded::Read);
    file.save(relabelled("One"), &theme()).unwrap();
    let real = p.store.with_file_name("backups-real");
    fs::rename(&p.store, &real).unwrap();
    let decoy = p.dir.join("decoy");
    fs::create_dir_all(&decoy).unwrap();
    fs::set_permissions(&decoy, Permissions::from_mode(0o700)).unwrap();
    symlink(&decoy, &p.store).unwrap();
    let before = snapshot(&p.dir);

    let saved = file.save(relabelled("Two"), &theme());

    assert!(is_backup_refusal(&saved), "{saved:?}");
    assert_eq!(snapshot(&p.dir), before, "nothing written through the link, the moved generations intact");
    let kept = Backups::at(&real).list(&p.config).unwrap();
    assert_eq!(kept.len(), 1);
    assert_eq!(kept_bytes(&real, &kept[0]), hand_written(&relabelled("Mine")));
}

#[test]
fn a_store_inside_the_config_directory_is_refused() {
    let p = Profile::new(SUITE, "store_in_config_dir");
    let mine = hand_written(&relabelled("Mine"));
    p.seed(&mine, 0o600);
    let dir = p.config.parent().unwrap().to_path_buf();
    for store in [dir.join("touchbar-customizer/backups"), dir.join("backups"), dir.clone()] {
        let mut file = ConfigFile::protected(&p.config, Backups::at(&store));
        assert_eq!(file.load(&theme()).unwrap(), Loaded::Read);
        let before = snapshot(&p.dir);

        let saved = file.save(relabelled("Next"), &theme());

        assert!(is_backup_refusal(&saved), "store {}: {saved:?}", store.display());
        assert_eq!(snapshot(&p.dir), before, "store {}", store.display());
    }
    let missing = Profile::new(SUITE, "store_in_missing_config_dir");
    let inside = missing.config.parent().unwrap().join("backups");
    let first = ConfigFile::protected(&missing.config, Backups::at(&inside)).load(&theme());
    assert!(is_backup_refusal(&first), "{first:?}");
    assert!(!missing.config.exists(), "no default written");
}

#[test]
fn a_store_that_cannot_be_written_aborts_the_save_and_a_retry_succeeds_once_it_can() {
    let p = Profile::new(SUITE, "store_read_only");
    let mine = hand_written(&relabelled("Mine"));
    p.seed(&mine, 0o640);
    fs::create_dir_all(&p.store).unwrap();
    if running_as_root(&p.store) {
        return; // root writes into a 0500 directory: the refusal cannot be injected
    }
    fs::set_permissions(&p.store, Permissions::from_mode(0o500)).unwrap();
    let mut file = protected(&p);
    assert_eq!(file.load(&theme()).unwrap(), Loaded::Read);
    let before = snapshot(&p.dir);

    let saved = file.save(relabelled("Next"), &theme());

    assert!(is_backup_refusal(&saved), "{saved:?}");
    assert_eq!(snapshot(&p.dir), before);
    assert_eq!(file.config(), &relabelled("Mine"), "the current config is unchanged");

    fs::set_permissions(&p.store, Permissions::from_mode(0o700)).unwrap();
    file.save(relabelled("Next"), &theme()).unwrap();
    let gens = p.generations();
    assert_eq!(gens.len(), 1, "{gens:?}");
    assert_eq!(kept_bytes(&p.store, &gens[0]), mine);
    assert_eq!(names_in(&p.store).len(), 1, "no leftover of the failed attempt: {:?}", names_in(&p.store));
}

#[test]
fn a_symlinked_config_directory_or_state_directory_loses_no_version() {
    let p = Profile::new(SUITE, "symlinked_parents");
    let real_dir = p.dir.join("real-config-dir");
    let mine = hand_written(&relabelled("Mine"));
    seed(&real_dir.join("config.toml"), &mine, 0o600);
    fs::create_dir_all(p.config.parent().unwrap().parent().unwrap()).unwrap();
    symlink(&real_dir, p.config.parent().unwrap()).unwrap();
    let real_state = p.dir.join("real-state");
    fs::create_dir_all(&real_state).unwrap();
    fs::create_dir_all(p.store.parent().unwrap().parent().unwrap()).unwrap();
    symlink(&real_state, p.store.parent().unwrap()).unwrap();
    let mut file = protected(&p);
    assert_eq!(file.load(&theme()).unwrap(), Loaded::Read);

    let saved = file.save(relabelled("Next"), &theme());

    match saved {
        Ok(()) => {
            assert_eq!(
                fs::read(real_dir.join("config.toml")).unwrap(),
                to_toml(&relabelled("Next")).unwrap().into_bytes()
            );
            let gens = p.generations();
            assert_eq!(gens.len(), 1, "{gens:?}");
            assert!(is_verified(&gens[0]));
            assert_eq!(kept_bytes(&p.store, &gens[0]), mine, "the replaced version is kept");
        }
        Err(e) => {
            assert_eq!(fs::read(real_dir.join("config.toml")).unwrap(), mine, "refused ({e}) and untouched");
            assert_eq!(Backups::at(&p.store).list(&p.config).unwrap_or_default(), vec![]);
        }
    }
    assert!(fs::symlink_metadata(p.config.parent().unwrap()).unwrap().file_type().is_symlink(), "link kept");
}

#[test]
fn a_store_path_that_is_a_file_is_refused_with_its_path() {
    let p = Profile::new(SUITE, "store_is_file");
    p.seed(&hand_written(&relabelled("Mine")), 0o600);
    fs::create_dir_all(p.store.parent().unwrap()).unwrap();
    fs::write(&p.store, b"not a directory").unwrap();
    let mut file = protected(&p);
    assert_eq!(file.load(&theme()).unwrap(), Loaded::Read);
    let before = snapshot(&p.dir);

    let saved = file.save(relabelled("Next"), &theme());

    assert!(is_backup_refusal(&saved), "{saved:?}");
    assert!(message(&saved).contains(&p.store.display().to_string()), "{}", message(&saved));
    assert_eq!(snapshot(&p.dir), before);
}
