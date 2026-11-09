//! Theme watch cases that start child processes, kept apart from the "not a change" cases in
//! `theme_watch.rs`. A spawned child gets a copy of every handle the other test threads have open
//! at that moment and keeps it until it execs. If one of them is a setup write handle on
//! `current/theme.name`, the child does its last close after that test's watch has started, and
//! inotify reports it as a `theme.name` write: a change the test never made. So no case that must
//! see no change shares a test binary with a spawn.
//!
//! Each test builds its own home under cargo's temp dir (`theme_hook/<test>`) from the theme
//! fixtures, never the real `$HOME`. Waits are long (loaded laptop).
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::mpsc::{Receiver, channel};
use std::thread;
use std::time::Duration;

use tb_config::{ActiveTheme, ThemeFileError, ThemeWatch};

const CHANGE: Duration = Duration::from_secs(10);

/// The negative theme watch cases, which must stay free of child processes.
const THEME_WATCH_CASES: &str = include_str!("theme_watch.rs");

fn fixture(theme: &str) -> String {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../fixtures/omarchy-themes")
        .join(theme)
        .join("colors.toml");
    fs::read_to_string(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()))
}

fn hook() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../contrib/omarchy/hooks/theme-set.d/touchbar-customizer")
}

/// An empty home directory for one test.
fn fresh_home(test: &str) -> PathBuf {
    let home = Path::new(env!("CARGO_TARGET_TMPDIR")).join("theme_hook").join(test);
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

#[test]
fn the_theme_watch_cases_start_no_child_process() {
    for word in ["std::process", "Command"] {
        assert!(
            !THEME_WATCH_CASES.contains(word),
            "theme_watch.rs holds cases that must see no change, so it must not start a child process \
             (found `{word}`): a spawned child keeps another test's setup write handle on \
             `current/theme.name` until it execs, and that last close after the watch has started is \
             reported as a `theme.name` write. Cases that spawn belong in theme_hook.rs."
        );
    }
}

#[test]
fn running_the_shipped_hook_is_a_change_and_records_the_theme_name() {
    let home = omarchy_home("hook", "tokyo-night");
    let state = home.join(".local/state/touchbar-customizer");
    assert!(!state.exists());
    let changes = watched(&home);

    let run = Command::new("bash").arg(hook()).arg("catppuccin-latte").env("HOME", &home).output().unwrap();

    assert!(
        run.status.success(),
        "hook exit {:?}, stderr: {}",
        run.status,
        String::from_utf8_lossy(&run.stderr)
    );
    one_change(&changes);
    assert_eq!(fs::read_to_string(state.join("theme-set")).unwrap(), "catppuccin-latte\n");
}

#[test]
fn a_theme_name_handle_closed_after_the_watch_starts_is_a_change() {
    let home = omarchy_home("late_close", "tokyo-night");
    let name = current(&home).join("theme.name");
    // A child holds the only write handle on `theme.name` (as a spawn holds a sibling test's
    // setup handle) and closes it when its stdin closes. The parent's copy is gone after spawn.
    let handle = fs::OpenOptions::new().append(true).open(&name).unwrap();
    let mut holder = Command::new("cat").stdin(Stdio::piped()).stdout(Stdio::from(handle)).spawn().unwrap();
    let changes = watched(&home);

    // Nothing is written; the child only closes its handle, after the watch has started.
    drop(holder.stdin.take());
    let exit = holder.wait().unwrap();

    assert!(exit.success(), "holder exit {exit:?}");
    one_change(&changes);
    assert_eq!(fs::read_to_string(&name).unwrap(), "tokyo-night\n", "the holder wrote nothing");
}
