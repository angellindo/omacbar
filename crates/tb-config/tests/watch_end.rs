//! The watch ends with a typed error when the config's directory is removed, so the caller can
//! re-arm it instead of blocking forever.
#![allow(clippy::unwrap_used, clippy::panic)]

use std::fs;
use std::path::Path;

use tb_config::{ConfigFile, ConfigFileError};
use tb_core::BarTheme;

#[test]
fn removing_the_watched_directory_ends_the_watch() {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join("watch_end").join("dir_removed");
    if dir.exists() {
        fs::remove_dir_all(&dir).unwrap();
    }
    let mut file = ConfigFile::new(dir.join("config.toml"));
    file.load(&BarTheme::omarchy_fallback()).unwrap();
    let mut watch = file.watch().unwrap();

    fs::remove_dir_all(&dir).unwrap();

    match watch.next_change() {
        Err(ConfigFileError::WatchEnded { dir: ended }) => assert_eq!(ended, dir),
        other => panic!("expected the watch to end, got {other:?}"),
    }
}
