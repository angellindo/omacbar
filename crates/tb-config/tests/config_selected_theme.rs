//! Selected-theme loads commit only valid candidates, and reload never recreates a missing file.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::fs;
use std::path::{Path, PathBuf};

use tb_config::{ConfigFile, ConfigFileError};
use tb_core::BarTheme;
use tb_core::config::{Config, ThemeSource, to_toml};

fn path(name: &str) -> PathBuf {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR"))
        .join("selected_theme")
        .join(format!("{name}-{}", std::process::id()));
    fs::create_dir_all(&dir).unwrap();
    dir.join("config.toml")
}

#[test]
fn a_missing_reload_keeps_the_accepted_config_without_creating_a_default() {
    let path = path("missing-reload");
    let mut saved = Config::default();
    saved.layout.layers[0].groups[0].buttons[0].label = "saved label".to_owned();
    fs::write(&path, to_toml(&saved).unwrap()).unwrap();
    let mut file = ConfigFile::new(&path);
    file.load_resolved(&BarTheme::omarchy_fallback()).unwrap();
    fs::remove_file(&path).unwrap();
    assert!(matches!(file.reload_resolved(&BarTheme::omarchy_fallback()), Err(ConfigFileError::Io { .. })));
    assert_eq!(file.config(), &saved);
    assert!(!path.exists());
}

#[test]
fn an_invalid_selected_theme_rejects_the_candidate_without_committing_its_layout() {
    let path = path("invalid-theme");
    let mut file = ConfigFile::new(&path);
    let active = BarTheme::omarchy_fallback();
    file.load_resolved(&active).unwrap();
    let colors = path.with_extension("colors");
    fs::write(&colors, "background = '#000000'\n").unwrap();
    let mut candidate = Config {
        theme: ThemeSource::File { path: colors.to_string_lossy().into_owned() },
        ..Config::default()
    };
    candidate.layout.layers[0].groups[0].buttons[0].label = "candidate".to_owned();
    fs::write(&path, to_toml(&candidate).unwrap()).unwrap();
    let error = file.reload_resolved(&active).unwrap_err();
    assert!(matches!(error, ConfigFileError::Theme(_)));
    assert!(error.to_string().contains("accent") || error.to_string().contains("foreground"));
    assert_eq!(file.config(), &Config::default());
    assert_eq!(fs::read_to_string(&path).unwrap(), to_toml(&candidate).unwrap());
}
