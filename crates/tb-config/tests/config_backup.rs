//! Backups an acknowledged overwrite keeps (ADR-0013): a read-only replaced file gets a read-only
//! backup, and taken backup names are skipped in order, never replaced.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::fs::{self, Permissions};
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};

use tb_config::{ConfigFile, ConfigFileError, Loaded};
use tb_core::config::{Config, to_toml};
use tb_core::{BarTheme, Chord};

fn fresh_dir(test: &str) -> PathBuf {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join("config_backup").join(test);
    if dir.exists() {
        fs::remove_dir_all(&dir).unwrap();
    }
    fs::create_dir_all(&dir).unwrap();
    dir
}

fn relabelled(label: &str) -> Config {
    let mut config = Config::default();
    let layer = config.layout.layer_mut(Chord::Default).unwrap();
    label.clone_into(&mut layer.groups[1].buttons[0].label);
    config
}

/// Saves `newer` over a file `b` read, then overwrites it from `b`; returns the backup path.
fn overwrite(path: &Path, newer: &str, mode: u32) -> PathBuf {
    let theme = BarTheme::omarchy_fallback();
    fs::write(path, to_toml(&Config::default()).unwrap()).unwrap();
    let mut b = ConfigFile::new(path);
    assert_eq!(b.load(&theme).unwrap(), Loaded::Read);
    fs::write(path, newer).unwrap();
    fs::set_permissions(path, Permissions::from_mode(mode)).unwrap();
    let Err(ConfigFileError::Changed(seen)) = b.save(relabelled("mine"), &theme) else { panic!("stale") };
    b.save_over(relabelled("mine"), &theme, &seen).unwrap();
    b.backup().unwrap().to_path_buf()
}

#[test]
fn a_read_only_file_gets_a_read_only_backup() {
    let path = fresh_dir("read_only").join("config.toml");
    let newer = to_toml(&relabelled("theirs")).unwrap();

    let backup = overwrite(&path, &newer, 0o444);

    assert_eq!(fs::read_to_string(&backup).unwrap(), newer);
    assert_eq!(fs::metadata(&backup).unwrap().permissions().mode() & 0o777, 0o400);
}

#[test]
fn taken_backup_names_are_skipped_in_order() {
    let dir = fresh_dir("taken_names");
    let path = dir.join("config.toml");
    fs::write(dir.join("config.toml.bak"), "first").unwrap();
    fs::write(dir.join("config.toml.bak.1"), "second").unwrap();
    let newer = to_toml(&relabelled("theirs")).unwrap();

    let backup = overwrite(&path, &newer, 0o644);

    assert_eq!(backup, dir.join("config.toml.bak.2"));
    assert_eq!(fs::read_to_string(dir.join("config.toml.bak")).unwrap(), "first");
    assert_eq!(fs::read_to_string(dir.join("config.toml.bak.1")).unwrap(), "second");
    assert_eq!(fs::metadata(&backup).unwrap().permissions().mode() & 0o777, 0o600);
}
