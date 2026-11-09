//! A config file with a key name the key vocabulary does not know is rejected when it is loaded or
//! saved, like any other issue: the last good config stays current and the file is left as the
//! user wrote it. Each test owns one temp directory under cargo's temp dir (never `$HOME`).
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::fs;
use std::path::{Path, PathBuf};

use tb_config::{ConfigFile, ConfigFileError, Loaded};
use tb_core::config::{Config, ConfigIssue, to_toml};
use tb_core::{Action, BarTheme, ButtonId, Chord};

fn fresh_dir(test: &str) -> PathBuf {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join("config_unknown_key").join(test);
    if dir.exists() {
        fs::remove_dir_all(&dir).unwrap();
    }
    fs::create_dir_all(&dir).unwrap();
    dir
}

fn theme() -> BarTheme {
    BarTheme::omarchy_fallback()
}

/// The default config whose default-layer button `id` presses `keys`.
fn pressing(id: &str, keys: &[&str]) -> Config {
    let mut config = Config::default();
    let layer = config.layout.layer_mut(Chord::Default).unwrap();
    let button =
        layer.groups.iter_mut().flat_map(|g| g.buttons.iter_mut()).find(|b| b.id.as_str() == id).unwrap();
    button.action = Action::Key { keys: keys.iter().map(|k| (*k).to_owned()).collect() };
    config
}

fn unknown_f55() -> ConfigIssue {
    ConfigIssue::UnknownKey { chord: Chord::Default, id: ButtonId::new("f5"), key: "F55".to_owned() }
}

#[test]
fn loading_a_file_with_an_unknown_key_rejects_it_and_keeps_the_last_good_config() {
    let path = fresh_dir("load").join("config.toml");
    let good = pressing("f5", &["LeftCtrl", "F5"]);
    fs::write(&path, to_toml(&good).unwrap()).unwrap();
    let mut file = ConfigFile::new(&path);
    assert_eq!(file.load(&theme()).unwrap(), Loaded::Read);

    let bad_text = to_toml(&pressing("f5", &["F55"])).unwrap();
    fs::write(&path, &bad_text).unwrap();
    assert_eq!(file.load(&theme()).unwrap(), Loaded::Rejected(vec![unknown_f55()]));
    assert_eq!(file.config(), &good);
    assert_eq!(fs::read_to_string(&path).unwrap(), bad_text, "the file is left as written");
}

#[test]
fn saving_a_config_with_an_unknown_key_is_refused_and_writes_nothing() {
    let path = fresh_dir("save").join("config.toml");
    let mut file = ConfigFile::new(&path);
    match file.save(pressing("f5", &["F55"]), &theme()) {
        Err(ConfigFileError::Invalid(issues)) => assert_eq!(issues, vec![unknown_f55()]),
        other => panic!("expected Invalid, got {other:?}"),
    }
    assert!(!path.exists());
    assert_eq!(file.config(), &Config::default());
}
