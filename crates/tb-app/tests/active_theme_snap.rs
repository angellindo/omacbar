//! Functional: the configurator draws the active Omarchy theme of `$HOME`. Each test runs
//! `touchbar-customizer --snap … --tree …` headless with `HOME` set to its own home under cargo's
//! temp dir, built from the theme fixtures (never the real `$HOME`), and reads the window's fill
//! from the element tree. Needs a GPU adapter (lavapipe is fine). With `TB_WRITE_SNAPSHOTS=1` the
//! snapshots are copied to `out/app-<theme>.png`.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

fn fresh_dir(test: &str, what: &str) -> PathBuf {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join("active_theme_snap").join(test).join(what);
    if dir.exists() {
        fs::remove_dir_all(&dir).unwrap();
    }
    fs::create_dir_all(&dir).unwrap();
    dir
}

/// A home whose active theme is the fixture `theme`.
fn home_with_theme(theme: &str) -> PathBuf {
    let src = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../fixtures/omarchy-themes")
        .join(theme)
        .join("colors.toml");
    let home = fresh_dir(theme, "home");
    let dir = home.join(".local/state/omarchy/current/theme");
    fs::create_dir_all(&dir).unwrap();
    fs::copy(&src, dir.join("colors.toml")).unwrap_or_else(|e| panic!("{}: {e}", src.display()));
    home
}

/// The snapshot and element tree of the configurator run with `HOME` = the `theme` fixture home.
fn snap(theme: &str) -> (PathBuf, String) {
    let home = home_with_theme(theme);
    let out = fresh_dir(theme, "out");
    let (png, tree) = (out.join("app.png"), out.join("app.json"));
    let run = Command::new(env!("CARGO_BIN_EXE_touchbar-customizer"))
        .arg("--snap")
        .arg(&png)
        .arg("--tree")
        .arg(&tree)
        .env("HOME", &home)
        .output()
        .unwrap();
    assert!(run.status.success(), "exit {:?}, stderr: {}", run.status, String::from_utf8_lossy(&run.stderr));
    assert!(fs::metadata(&png).unwrap().len() > 0, "{} is written", png.display());
    if std::env::var_os("TB_WRITE_SNAPSHOTS").is_some() {
        let dest = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../out");
        fs::create_dir_all(&dest).unwrap();
        fs::copy(&png, dest.join(format!("app-{theme}.png"))).unwrap();
    }
    (png, fs::read_to_string(&tree).unwrap())
}

/// The fill of the window's root element: the first element of the tree dump.
fn window_fill(tree: &str) -> &str {
    let elements = tree.find("\"elements\"").expect("the tree lists its elements");
    let rest = &tree[elements..];
    let start = rest.find("\"fill\": \"").expect("the root element has a fill") + "\"fill\": \"".len();
    let len = rest[start..].find('"').unwrap();
    &rest[start..start + len]
}

#[test]
fn the_configurator_draws_the_active_catppuccin_latte_theme_of_its_home() {
    let (_, tree) = snap("catppuccin-latte");
    // catppuccin-latte `background` = #eff1f5; the bundled Tokyo Night's is #1a1b26.
    assert_eq!(window_fill(&tree), "#eff1f5ff");
    assert!(!tree.contains("#1a1b26ff"), "no Tokyo Night background anywhere in the window");
}

#[test]
fn the_configurator_draws_the_active_tokyo_night_theme_of_its_home() {
    let (_, tree) = snap("tokyo-night");
    assert_eq!(window_fill(&tree), "#1a1b26ff");
}
