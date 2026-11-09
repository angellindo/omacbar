//! A theme watch reports a theme switch (as `omarchy theme set` makes it: stage `next-theme`,
//! remove `theme`, rename `next-theme` to `theme`, write `theme.generation` and `theme.name`);
//! other changes under the Omarchy state directory are not switches. No test in this file may
//! start a child process: a spawned child holds the other tests' setup write handles until it
//! execs, and that late last close on `theme.name` looks like a theme switch to a watch that has
//! already started. Cases that spawn live in `theme_hook.rs`. Each test builds its own home under
//! cargo's temp dir from the theme fixtures (never the real `$HOME`). Positive waits are long
//! (loaded laptop), negative waits short.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::mpsc::{Receiver, RecvTimeoutError, channel};
use std::thread;
use std::time::Duration;

use tb_config::{ActiveTheme, ThemeFileError, ThemeWatch};
use tb_core::{BarTheme, Rgba};

const CHANGE: Duration = Duration::from_secs(10);
const QUIET: Duration = Duration::from_millis(500);

fn fixture(theme: &str) -> String {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../fixtures/omarchy-themes")
        .join(theme)
        .join("colors.toml");
    fs::read_to_string(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()))
}

/// An empty home directory for one test.
fn fresh_home(test: &str) -> PathBuf {
    let home = Path::new(env!("CARGO_TARGET_TMPDIR")).join("theme_watch").join(test);
    if home.exists() {
        fs::remove_dir_all(&home).unwrap();
    }
    fs::create_dir_all(&home).unwrap();
    home
}

fn current(home: &Path) -> PathBuf {
    home.join(".local/state/omarchy/current")
}

/// A home where `omarchy theme set <theme>` has run: `current/theme/colors.toml`, a background
/// link into the theme, `theme.generation` and `theme.name`.
fn omarchy_home(test: &str, theme: &str) -> PathBuf {
    let home = fresh_home(test);
    let current = current(&home);
    fs::create_dir_all(current.join("theme/backgrounds")).unwrap();
    fs::write(current.join("theme/colors.toml"), fixture(theme)).unwrap();
    fs::write(current.join("theme/backgrounds/1.png"), b"not a real image").unwrap();
    std::os::unix::fs::symlink(current.join("theme/backgrounds/1.png"), current.join("background")).unwrap();
    fs::write(current.join("theme.generation"), "1-1\n").unwrap();
    fs::write(current.join("theme.name"), format!("{theme}\n")).unwrap();
    home
}

/// The steps of `omarchy theme set <theme>` that touch `current/`, in its order.
fn switch_theme(home: &Path, theme: &str) {
    let current = current(home);
    let next = current.join("next-theme");
    fs::create_dir_all(&next).unwrap();
    fs::write(next.join("colors.toml"), fixture(theme)).unwrap();
    fs::remove_dir_all(current.join("theme")).unwrap();
    fs::rename(&next, current.join("theme")).unwrap();
    fs::write(current.join("theme.generation"), "2-2\n").unwrap();
    fs::write(current.join("theme.name"), format!("{theme}\n")).unwrap();
}

/// Blocks on `next_change` on its own thread and sends every result.
fn on_a_thread(mut watch: ThemeWatch) -> Receiver<Result<(), ThemeFileError>> {
    let (tx, rx) = channel();
    thread::spawn(move || {
        loop {
            let change = watch.next_change();
            let ended = change.is_err();
            if tx.send(change).is_err() || ended {
                break;
            }
        }
    });
    rx
}

fn watched(home: &Path) -> Receiver<Result<(), ThemeFileError>> {
    on_a_thread(ActiveTheme::under_home(home).watch().unwrap())
}

fn one_change(changes: &Receiver<Result<(), ThemeFileError>>) {
    match changes.recv_timeout(CHANGE) {
        Ok(Ok(())) => {}
        other => panic!("expected a change, got {other:?}"),
    }
}

fn no_change(changes: &Receiver<Result<(), ThemeFileError>>) {
    match changes.recv_timeout(QUIET) {
        Err(RecvTimeoutError::Timeout) => {}
        other => panic!("expected no change, got {other:?}"),
    }
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

#[test]
fn a_theme_switch_is_a_change_and_the_next_load_reads_the_new_theme() {
    let home = omarchy_home("switch", "tokyo-night");
    let changes = watched(&home);

    switch_theme(&home, "catppuccin-latte");

    one_change(&changes);
    let loaded = ActiveTheme::under_home(&home).load();
    assert!(loaded.fallback.is_none(), "{:?}", loaded.fallback);
    assert_eq!(loaded.theme.background, Rgba::parse_hex("#eff1f5").unwrap());
    assert_eq!(loaded.theme, BarTheme::from_omarchy_colors(&fixture("catppuccin-latte")).unwrap());
}

#[test]
fn staging_the_next_theme_is_not_a_change() {
    let home = omarchy_home("staging", "tokyo-night");
    let changes = watched(&home);

    let next = current(&home).join("next-theme");
    fs::create_dir_all(&next).unwrap();
    fs::write(next.join("colors.toml"), fixture("catppuccin-latte")).unwrap();

    no_change(&changes);
}

#[test]
fn writing_the_theme_generation_alone_is_not_a_change() {
    let home = omarchy_home("generation", "tokyo-night");
    let changes = watched(&home);

    fs::write(current(&home).join("theme.generation"), "3-3\n").unwrap();

    no_change(&changes);
}

#[test]
fn replacing_the_background_link_is_not_a_change() {
    let home = omarchy_home("background", "tokyo-night");
    let changes = watched(&home);
    let current = current(&home);

    let staged = current.join("background.next");
    std::os::unix::fs::symlink(current.join("theme/backgrounds/1.png"), &staged).unwrap();
    fs::rename(&staged, current.join("background")).unwrap();

    no_change(&changes);
}

#[test]
fn editing_the_active_colours_file_in_place_is_not_a_change() {
    let home = omarchy_home("edit_in_place", "tokyo-night");
    let changes = watched(&home);

    fs::write(current(&home).join("theme/colors.toml"), fixture("catppuccin-latte")).unwrap();

    no_change(&changes);
}

#[test]
fn watching_a_home_without_an_omarchy_theme_is_an_error_and_creates_nothing() {
    let home = fresh_home("no_theme");

    let watch = ActiveTheme::under_home(&home).watch();

    assert!(watch.is_err(), "no `.local/state/omarchy/current/` to watch");
    assert_eq!(entries(&home), Vec::<PathBuf>::new(), "a failed watch creates nothing");
}

#[test]
fn removing_the_omarchy_current_directory_ends_the_watch_naming_it() {
    let home = omarchy_home("removed", "tokyo-night");
    let changes = watched(&home);

    fs::remove_dir_all(current(&home)).unwrap();

    // Changes reported on the way (files going away with the directory) do not matter; the watch
    // must end, naming the directory.
    loop {
        match changes.recv_timeout(CHANGE) {
            Ok(Ok(())) => {}
            Ok(Err(ThemeFileError::WatchEnded { dir })) => break assert_eq!(dir, current(&home)),
            other => panic!("expected WatchEnded naming {}, got {other:?}", current(&home).display()),
        }
    }
}
