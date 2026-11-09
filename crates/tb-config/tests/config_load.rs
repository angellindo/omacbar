//! Loading the config file: a missing file gets the default config written, a valid file becomes
//! the current config untouched, and an invalid file lists every issue while the last good config
//! stays current and the file is left as the user wrote it. Each test owns one temp directory.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::fs;
use std::path::{Path, PathBuf};

use tb_config::{ConfigFile, ConfigFileError, Loaded, default_path};
use tb_core::config::{Config, ConfigIssue, parse, to_toml};
use tb_core::{BarTheme, Chord};

/// An empty directory for one test, under cargo's temp dir (never `$HOME`).
fn fresh_dir(test: &str) -> PathBuf {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join("config_load").join(test);
    if dir.exists() {
        fs::remove_dir_all(&dir).unwrap();
    }
    fs::create_dir_all(&dir).unwrap();
    dir
}

fn theme() -> BarTheme {
    BarTheme::omarchy_fallback()
}

fn names_in(dir: &Path) -> Vec<String> {
    let mut names: Vec<String> =
        fs::read_dir(dir).unwrap().map(|e| e.unwrap().file_name().to_string_lossy().into_owned()).collect();
    names.sort();
    names
}

/// The default config with the default-layer button `id` given another label.
fn relabelled(id: &str, label: &str) -> Config {
    let mut config = Config::default();
    let layer = config.layout.layer_mut(Chord::Default).unwrap();
    let button =
        layer.groups.iter_mut().flat_map(|g| g.buttons.iter_mut()).find(|b| b.id.as_str() == id).unwrap();
    label.clone_into(&mut button.label);
    config
}

/// A valid file as a user would write it: the comment is lost if the file is ever rewritten.
fn hand_written(config: &Config) -> String {
    format!("# my bar\n{}", to_toml(config).unwrap())
}

#[test]
fn the_default_path_is_under_the_home_config_directory() {
    assert_eq!(
        default_path(Path::new("/home/u")),
        PathBuf::from("/home/u/.config/touchbar-customizer/config.toml")
    );
}

#[test]
fn a_new_config_file_touches_nothing_and_starts_with_the_default_config() {
    let dir = fresh_dir("new_touches_nothing");
    let path = dir.join("touchbar-customizer").join("config.toml");
    let file = ConfigFile::new(&path);
    assert_eq!(file.path(), path.as_path());
    assert_eq!(file.config(), &Config::default());
    assert!(!dir.join("touchbar-customizer").exists());
}

#[test]
fn loading_a_missing_file_writes_the_default_config_and_its_directory() {
    let dir = fresh_dir("missing_writes_default").join("touchbar-customizer");
    let path = dir.join("config.toml");
    let mut file = ConfigFile::new(&path);

    assert_eq!(file.load(&theme()).unwrap(), Loaded::WroteDefault);

    let written = fs::read_to_string(&path).unwrap();
    assert_eq!(parse(&written).unwrap(), Config::default(), "\n{written}");
    assert_eq!(file.config(), &Config::default());
    assert_eq!(names_in(&dir), vec!["config.toml".to_owned()]);
}

#[test]
fn loading_a_valid_file_makes_it_current_and_leaves_the_file_unchanged() {
    let path = fresh_dir("valid_is_read").join("config.toml");
    let mine = relabelled("f5", "bright");
    let text = hand_written(&mine);
    fs::write(&path, &text).unwrap();
    let mut file = ConfigFile::new(&path);

    assert_eq!(file.load(&theme()).unwrap(), Loaded::Read);

    assert_eq!(file.config(), &mine);
    assert_eq!(fs::read_to_string(&path).unwrap(), text);
}

#[test]
fn an_invalid_file_lists_every_issue_and_keeps_the_last_good_config() {
    let path = fresh_dir("invalid_keeps_last_good").join("config.toml");
    let good = relabelled("f5", "bright");
    fs::write(&path, hand_written(&good)).unwrap();
    let mut file = ConfigFile::new(&path);
    assert_eq!(file.load(&theme()).unwrap(), Loaded::Read);

    let mut bad = relabelled("f5", "");
    bad.version = 2;
    let bad_text = to_toml(&bad).unwrap();
    fs::write(&path, &bad_text).unwrap();

    let Loaded::Rejected(issues) = file.load(&theme()).unwrap() else {
        panic!("an invalid file must be rejected")
    };
    assert!(issues.contains(&ConfigIssue::Version(2)), "{issues:?}");
    assert!(
        issues.iter().any(
            |i| matches!(i, ConfigIssue::EmptyLabel { chord: Chord::Default, id } if id.as_str() == "f5")
        ),
        "{issues:?}"
    );
    assert_eq!(file.config(), &good);
    assert_eq!(fs::read_to_string(&path).unwrap(), bad_text);
}

#[test]
fn a_file_that_is_not_valid_toml_is_rejected_with_one_syntax_issue() {
    let path = fresh_dir("broken_toml").join("config.toml");
    fs::write(&path, "version = ").unwrap();
    let mut file = ConfigFile::new(&path);

    let Loaded::Rejected(issues) = file.load(&theme()).unwrap() else {
        panic!("broken TOML must be rejected")
    };
    assert_eq!(issues.len(), 1, "{issues:?}");
    assert!(matches!(issues[0], ConfigIssue::Syntax(_)), "{issues:?}");
}

#[test]
fn an_invalid_file_on_the_first_load_keeps_the_default_config_and_the_file() {
    let path = fresh_dir("invalid_first_load").join("config.toml");
    fs::write(&path, "version = ").unwrap();
    let mut file = ConfigFile::new(&path);

    assert!(matches!(file.load(&theme()).unwrap(), Loaded::Rejected(_)));

    assert_eq!(file.config(), &Config::default());
    assert_eq!(fs::read_to_string(&path).unwrap(), "version = ");
}

#[test]
fn a_style_colour_the_theme_lacks_is_rejected_as_an_unknown_colour() {
    let path = fresh_dir("unknown_colour").join("config.toml");
    let mut config = Config::default();
    let layer = config.layout.layer_mut(Chord::Default).unwrap();
    let f5 =
        layer.groups.iter_mut().flat_map(|g| g.buttons.iter_mut()).find(|b| b.id.as_str() == "f5").unwrap();
    f5.style.fill = Some("chartreuse".to_owned());
    fs::write(&path, to_toml(&config).unwrap()).unwrap();
    let mut file = ConfigFile::new(&path);

    let Loaded::Rejected(issues) = file.load(&theme()).unwrap() else {
        panic!("an unknown colour must be rejected")
    };
    assert!(
        issues.iter().any(|i| matches!(i, ConfigIssue::UnknownColor { id, color, .. }
            if id.as_str() == "f5" && color == "chartreuse")),
        "{issues:?}"
    );
    assert_eq!(file.config(), &Config::default());
}

#[test]
fn a_file_that_cannot_be_read_is_an_io_error_naming_it_and_keeps_the_current_config() {
    let dir = fresh_dir("unreadable");
    let path = dir.join("config.toml");
    let good = relabelled("f5", "bright");
    fs::write(&path, hand_written(&good)).unwrap();
    let mut file = ConfigFile::new(&path);
    assert_eq!(file.load(&theme()).unwrap(), Loaded::Read);

    fs::remove_file(&path).unwrap();
    fs::create_dir(&path).unwrap();

    match file.load(&theme()) {
        Err(ConfigFileError::Io { path: named, .. }) => assert_eq!(named, path),
        other => panic!("expected an Io error naming the file, got {other:?}"),
    }
    assert_eq!(file.config(), &good);
    assert!(path.is_dir(), "a failed load must not replace what is at the path");
}
