//! `save_resolved` re-reads the candidate's selected theme file before writing.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::fs;
use std::path::PathBuf;

use tb_config::{ConfigFile, ConfigFileError};
use tb_core::BarTheme;
use tb_core::config::{Config, ThemeSource, to_toml};

fn dir(test: &str) -> PathBuf {
    let dir = PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join("config_save_resolved").join(test);
    if dir.exists() {
        fs::remove_dir_all(&dir).unwrap();
    }
    fs::create_dir_all(&dir).unwrap();
    dir
}

fn selecting(theme: &std::path::Path) -> Config {
    let mut config =
        Config { theme: ThemeSource::File { path: theme.display().to_string() }, ..Config::default() };
    config.layout.layers[0].groups[0].buttons[0].style.fill = Some("only_here".to_owned());
    config
}

#[test]
fn a_save_resolves_the_selected_theme_file_and_refuses_when_it_is_gone() {
    let dir = dir("gone");
    let theme = dir.join("colors.toml");
    let path = dir.join("config.toml");
    fs::write(&path, "# mine\n").unwrap();
    let mut file = ConfigFile::new(&path);
    let error = file.save_resolved(selecting(&theme), &BarTheme::omarchy_fallback()).unwrap_err();
    assert!(matches!(error, ConfigFileError::Theme(_)), "{error}");
    assert_eq!(fs::read_to_string(&path).unwrap(), "# mine\n", "nothing is written");

    fs::write(&theme, "background = \"#000000\"\nforeground = \"#ffffff\"\naccent = \"#4c8cf0\"\nred = \"#ff0000\"\nonly_here = \"#123456\"\n")
        .unwrap();
    let resolved = file.save_resolved(selecting(&theme), &BarTheme::omarchy_fallback()).unwrap();
    assert!(resolved.resolve("only_here").is_some());
    assert_eq!(fs::read_to_string(&path).unwrap(), to_toml(&selecting(&theme)).unwrap());
}
