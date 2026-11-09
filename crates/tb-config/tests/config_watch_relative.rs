//! A config file named only by its file name lives in the current directory, and a watch on it
//! watches that directory. This file holds a single test because it changes the process's
//! current directory, which would disturb tests running in parallel.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::fs;
use std::path::Path;
use std::sync::mpsc::channel;
use std::thread;
use std::time::Duration;

use tb_config::{ConfigFile, Loaded};
use tb_core::BarTheme;
use tb_core::config::{Config, ThemeSource, to_toml};

#[test]
fn a_config_file_named_without_a_directory_is_watched_in_the_current_directory() {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join("config_watch_relative");
    if dir.exists() {
        fs::remove_dir_all(&dir).unwrap();
    }
    fs::create_dir_all(&dir).unwrap();
    std::env::set_current_dir(&dir).unwrap();
    let theme = BarTheme::omarchy_fallback();
    let mut file = ConfigFile::new("config.toml");
    assert_eq!(file.load(&theme).unwrap(), Loaded::WroteDefault);
    assert!(dir.join("config.toml").is_file());

    let mut watch = match file.watch() {
        Ok(watch) => watch,
        Err(e) => panic!("a config file in the current directory must be watchable, got {e:?}"),
    };
    let (tx, changes) = channel();
    thread::spawn(move || {
        let _ = tx.send(watch.next_change());
    });

    let mine = Config { theme: ThemeSource::File { path: "colors.toml".to_owned() }, ..Config::default() };
    fs::write("config.toml", to_toml(&mine).unwrap()).unwrap();

    match changes.recv_timeout(Duration::from_secs(10)) {
        Ok(Ok(())) => {}
        other => panic!("expected a change, got {other:?}"),
    }
    assert_eq!(file.load(&theme).unwrap(), Loaded::Read);
    assert_eq!(file.config(), &mine);
}
