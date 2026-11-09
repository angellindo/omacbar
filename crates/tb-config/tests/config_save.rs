//! Saving the config file is atomic: a valid config is written to a temp file in the same
//! directory and renamed over the file (no temp left behind); an invalid config is refused and
//! nothing on disk or in memory changes. Each test owns one temp directory.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::fs;
use std::os::unix::fs::MetadataExt;
use std::path::{Path, PathBuf};

use tb_config::{ConfigFile, ConfigFileError, Loaded};
use tb_core::config::{Config, ConfigIssue, parse};
use tb_core::{BarTheme, Chord};

/// An empty directory for one test, under cargo's temp dir (never `$HOME`).
fn fresh_dir(test: &str) -> PathBuf {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join("config_save").join(test);
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

/// A config file whose default was just written by a load.
fn loaded(dir: &Path) -> ConfigFile {
    let mut file = ConfigFile::new(dir.join("config.toml"));
    assert_eq!(file.load(&theme()).unwrap(), Loaded::WroteDefault);
    file
}

#[test]
fn saving_a_valid_config_renames_a_new_file_over_the_old_and_makes_it_current() {
    let dir = fresh_dir("valid_replaces_by_rename");
    let mut file = loaded(&dir);
    let path = file.path().to_path_buf();
    let inode_before = fs::metadata(&path).unwrap().ino();
    let mine = relabelled("f5", "bright");

    file.save(mine.clone(), &theme()).unwrap();

    let written = fs::read_to_string(&path).unwrap();
    assert_eq!(parse(&written).unwrap(), mine, "\n{written}");
    assert_eq!(file.config(), &mine);
    assert_ne!(fs::metadata(&path).unwrap().ino(), inode_before, "the file must be replaced by a rename");
    assert_eq!(names_in(&dir), vec!["config.toml".to_owned()], "no temp file may be left behind");
}

#[test]
fn a_saved_config_is_what_the_next_load_reads() {
    let dir = fresh_dir("saved_then_read");
    let mine = relabelled("f5", "bright");
    loaded(&dir).save(mine.clone(), &theme()).unwrap();

    let mut again = ConfigFile::new(dir.join("config.toml"));
    assert_eq!(again.load(&theme()).unwrap(), Loaded::Read);
    assert_eq!(again.config(), &mine);
}

#[test]
fn saving_an_invalid_config_writes_nothing_and_keeps_the_current_config() {
    let dir = fresh_dir("invalid_is_refused");
    let mut file = loaded(&dir);
    let path = file.path().to_path_buf();
    let bytes_before = fs::read(&path).unwrap();
    let inode_before = fs::metadata(&path).unwrap().ino();
    let mut bad = Config::default();
    let layer = bad.layout.layer_mut(Chord::Default).unwrap();
    let f6 = layer.groups[1].buttons.iter().find(|b| b.id.as_str() == "f6").unwrap().clone();
    let f5 = layer.groups[1].buttons.iter_mut().find(|b| b.id.as_str() == "f5").unwrap();
    f5.id = f6.id;

    match file.save(bad, &theme()) {
        Err(ConfigFileError::Invalid(issues)) => assert!(
            issues.iter().any(|i| matches!(i, ConfigIssue::DuplicateId { id, .. } if id.as_str() == "f6")),
            "{issues:?}"
        ),
        other => panic!("an invalid config must be refused, got {other:?}"),
    }
    assert_eq!(fs::read(&path).unwrap(), bytes_before);
    assert_eq!(fs::metadata(&path).unwrap().ino(), inode_before);
    assert_eq!(file.config(), &Config::default());
    assert_eq!(names_in(&dir), vec!["config.toml".to_owned()]);
}

#[test]
fn saving_into_a_missing_directory_creates_it() {
    let dir = fresh_dir("missing_directory").join("touchbar-customizer");
    let mut file = ConfigFile::new(dir.join("config.toml"));
    let mine = relabelled("f5", "bright");

    file.save(mine.clone(), &theme()).unwrap();

    assert_eq!(parse(&fs::read_to_string(dir.join("config.toml")).unwrap()).unwrap(), mine);
    assert_eq!(file.config(), &mine);
    assert_eq!(names_in(&dir), vec!["config.toml".to_owned()]);
}

#[test]
fn a_save_that_cannot_rename_is_an_io_error_and_leaves_no_temp_file() {
    let dir = fresh_dir("rename_fails");
    let path = dir.join("config.toml");
    fs::create_dir(&path).unwrap();
    fs::write(path.join("keep"), "x").unwrap();
    let mut file = ConfigFile::new(&path);

    let result = file.save(relabelled("f5", "bright"), &theme());

    assert!(matches!(result, Err(ConfigFileError::Io { .. })), "{result:?}");
    assert_eq!(names_in(&dir), vec!["config.toml".to_owned()], "no temp file may be left behind");
    assert_eq!(names_in(&path), vec!["keep".to_owned()]);
    assert_eq!(file.config(), &Config::default());
}

#[test]
fn saves_from_several_config_files_at_once_each_land_whole_and_leave_no_temp_file() {
    const WRITERS: usize = 4;
    const SAVES: usize = 40;
    let dir = fresh_dir("concurrent_saves");
    let path = dir.join("config.toml");
    let label = |writer: usize, save: usize| format!("w{writer}s{save}");

    let results: Vec<Vec<String>> = std::thread::scope(|scope| {
        let writers: Vec<_> = (0..WRITERS)
            .map(|writer| {
                let path = path.clone();
                scope.spawn(move || {
                    let mut file = ConfigFile::new(path);
                    (0..SAVES)
                        .filter_map(|save| {
                            file.save(relabelled("f5", &label(writer, save)), &theme())
                                .err()
                                .map(|e| format!("writer {writer} save {save}: {e:?}"))
                        })
                        .collect()
                })
            })
            .collect();
        writers.into_iter().map(|w| w.join().unwrap()).collect()
    });

    let failures: Vec<String> = results.into_iter().flatten().collect();
    assert!(failures.is_empty(), "every save must succeed: {failures:#?}");
    let written = fs::read_to_string(&path).unwrap();
    let last =
        parse(&written).unwrap_or_else(|e| panic!("the file must be one whole config: {e}\n{written}"));
    let saved: Vec<Config> = (0..WRITERS)
        .flat_map(|w| (0..SAVES).map(move |s| (w, s)))
        .map(|(w, s)| relabelled("f5", &label(w, s)))
        .collect();
    assert!(saved.contains(&last), "the file must be one of the saved configs\n{written}");
    assert_eq!(names_in(&dir), vec!["config.toml".to_owned()], "no temp file may be left behind");
}
