//! The active Omarchy theme is read from `<home>/.local/state/omarchy/current/theme/colors.toml`.
//! When that file is missing, unreadable or not a valid Omarchy theme, the bar shows the bundled
//! Tokyo Night and says why, naming the file. Each test builds its own home under cargo's temp
//! dir from the theme fixtures (never the real `$HOME`).
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::fs;
use std::io::ErrorKind;
use std::path::{Path, PathBuf};

use tb_config::{ActiveTheme, LoadedTheme, ThemeFileError};
use tb_core::theme::ThemeError;
use tb_core::{BarTheme, Rgba};

/// An empty home directory for one test.
fn fresh_home(test: &str) -> PathBuf {
    let home = Path::new(env!("CARGO_TARGET_TMPDIR")).join("active_theme").join(test);
    if home.exists() {
        fs::remove_dir_all(&home).unwrap();
    }
    fs::create_dir_all(&home).unwrap();
    home
}

/// The text of a theme fixture's `colors.toml` (copied from Omarchy's `themes/<name>/`).
fn fixture(theme: &str) -> String {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../fixtures/omarchy-themes")
        .join(theme)
        .join("colors.toml");
    fs::read_to_string(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()))
}

/// The active theme's directory, as `omarchy theme set` leaves it.
fn current_theme_dir(home: &Path) -> PathBuf {
    home.join(".local/state/omarchy/current/theme")
}

/// A home whose active theme's `colors.toml` holds `colors`.
fn home_with_colors(test: &str, colors: &str) -> PathBuf {
    let home = fresh_home(test);
    let dir = current_theme_dir(&home);
    fs::create_dir_all(&dir).unwrap();
    fs::write(dir.join("colors.toml"), colors).unwrap();
    home
}

fn hex(s: &str) -> Rgba {
    Rgba::parse_hex(s).unwrap()
}

fn entries(dir: &Path) -> Vec<PathBuf> {
    let mut all = Vec::new();
    for e in fs::read_dir(dir).unwrap() {
        let path = e.unwrap().path();
        if path.is_dir() {
            all.extend(entries(&path));
        }
        all.push(path);
    }
    all.sort();
    all
}

/// The fallback with its reason; panics when the theme came from the file.
fn fallback_reason(loaded: LoadedTheme) -> ThemeFileError {
    assert_eq!(loaded.theme, BarTheme::omarchy_fallback(), "a fallback shows the bundled Tokyo Night");
    loaded.fallback.expect("the fallback says why")
}

#[test]
fn the_colours_file_is_under_the_omarchy_state_directory_of_the_home() {
    let active = ActiveTheme::under_home(Path::new("/home/u"));
    assert_eq!(active.colors_path(), Path::new("/home/u/.local/state/omarchy/current/theme/colors.toml"));
}

#[test]
fn an_active_catppuccin_latte_theme_is_drawn_in_its_own_colours() {
    let home = home_with_colors("catppuccin_latte", &fixture("catppuccin-latte"));

    let loaded = ActiveTheme::under_home(&home).load();

    assert!(loaded.fallback.is_none(), "read from the file, got fallback {:?}", loaded.fallback);
    let theme = loaded.theme;
    assert_eq!(theme.background, hex("#eff1f5"));
    assert_eq!(theme.button_text, hex("#4c4f69"));
    assert_eq!(theme.pressed_fill, hex("#1e66f5"));
    assert_eq!(theme.accent, hex("#1e66f5"));
    assert_eq!(theme.pressed_text, hex("#eff1f5"));
    assert_eq!(theme.resolve("red"), Some(hex("#d20f39")));
}

#[test]
fn an_active_tokyo_night_theme_is_read_from_the_file_and_equals_the_bundled_one() {
    let home = home_with_colors("tokyo_night", &fixture("tokyo-night"));

    let loaded = ActiveTheme::under_home(&home).load();

    assert!(loaded.fallback.is_none(), "read from the file, got fallback {:?}", loaded.fallback);
    assert_eq!(loaded.theme, BarTheme::omarchy_fallback());
    assert_eq!(loaded.theme.background, hex("#1a1b26"));
}

#[test]
fn a_home_without_an_omarchy_theme_shows_the_fallback_naming_the_missing_file_and_creates_nothing() {
    let home = fresh_home("no_theme");
    let active = ActiveTheme::under_home(&home);

    let reason = fallback_reason(active.load());

    match &reason {
        ThemeFileError::Io { path, source } => {
            assert_eq!(path, active.colors_path());
            assert_eq!(source.kind(), ErrorKind::NotFound, "{source}");
        }
        other => panic!("expected an Io error naming the colours file, got {other:?}"),
    }
    assert!(
        reason.to_string().contains(&active.colors_path().display().to_string()),
        "the message names the file: {reason}"
    );
    assert_eq!(entries(&home), Vec::<PathBuf>::new(), "load creates nothing");
}

#[test]
fn a_colours_path_that_is_a_directory_shows_the_fallback_naming_it() {
    let home = fresh_home("colors_is_a_directory");
    fs::create_dir_all(current_theme_dir(&home).join("colors.toml")).unwrap();
    let active = ActiveTheme::under_home(&home);

    match fallback_reason(active.load()) {
        ThemeFileError::Io { path, .. } => assert_eq!(path, active.colors_path()),
        other => panic!("expected an Io error naming the colours file, got {other:?}"),
    }
}

#[test]
fn a_colours_file_without_an_accent_shows_the_fallback_with_the_missing_token() {
    let without_accent: String = fixture("catppuccin-latte")
        .lines()
        .filter(|l| !l.starts_with("accent"))
        .flat_map(|l| [l, "\n"])
        .collect();
    let home = home_with_colors("missing_accent", &without_accent);
    let active = ActiveTheme::under_home(&home);

    let reason = fallback_reason(active.load());

    match &reason {
        ThemeFileError::Invalid { path, source: ThemeError::Missing("accent") } => {
            assert_eq!(path, active.colors_path());
        }
        other => panic!("expected Invalid with the missing `accent`, got {other:?}"),
    }
    assert!(
        reason.to_string().contains(&active.colors_path().display().to_string()),
        "the message names the file: {reason}"
    );
}

#[test]
fn a_colours_file_that_is_not_toml_shows_the_fallback_with_the_parse_error() {
    let home = home_with_colors("not_toml", "this is = = not toml [\n");
    let active = ActiveTheme::under_home(&home);

    match fallback_reason(active.load()) {
        ThemeFileError::Invalid { path, source: ThemeError::Toml(_) } => {
            assert_eq!(path, active.colors_path());
        }
        other => panic!("expected Invalid with a TOML error, got {other:?}"),
    }
}
