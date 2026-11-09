//! Functional: the configurator opens the config file it was given (writing the default when it
//! is missing, as `ConfigFile::load` does), saves through `tb_config::ConfigFile::save` when the
//! save button is pressed, and says in `status` what happened. The same drag and save as the flow
//! script `scripts/drag-f3-before-f1.json`, run in-process through `rune::Runtime` (testing
//! standard rule 5). Each test owns a config file under cargo's temp dir.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod support {
    pub mod ui;
}

use std::fs;

use support::ui::*;
use tb_config::{ConfigFile, Loaded};
use tb_core::Chord;
use tb_core::config::{Config, to_toml};

const F3_FIRST: [&str; 12] = ["f3", "f1", "f2", "f4", "f5", "f6", "f7", "f8", "f9", "f10", "f11", "f12"];

/// The default config with the default layer's F-key group in this order, built by hand.
fn default_with_f_keys(order: &[&str]) -> Config {
    let mut config = Config::default();
    let layer = config.layout.layer_mut(Chord::Default).unwrap();
    let f_keys = layer.groups[1].buttons.clone();
    layer.groups[1].buttons =
        order.iter().map(|id| f_keys.iter().find(|b| b.id.as_str() == *id).cloned().unwrap()).collect();
    config
}

/// The config the file at `path` holds, read the way the bar reads it.
fn on_disk(path: &std::path::Path) -> Config {
    let mut file = ConfigFile::new(path);
    assert_eq!(file.load(&theme()).unwrap(), Loaded::Read, "{} is a valid config", path.display());
    file.config().clone()
}

#[test]
fn opening_a_missing_config_edits_the_default_and_writes_it() {
    let path = fresh_dir("save_in_configurator", "missing").join("config.toml");
    let rt = configurator(path.clone());
    assert_eq!(rt.state.config, Config::default());
    assert_eq!(on_disk(&path), Config::default());
}

#[test]
fn opening_a_saved_config_edits_what_the_file_holds() {
    let path = fresh_dir("save_in_configurator", "reopen").join("config.toml");
    let saved = default_with_f_keys(&F3_FIRST);
    fs::write(&path, to_toml(&saved).unwrap()).unwrap();
    let rt = configurator(path);
    assert_eq!(rt.state.config, saved);
    let (f3, f1) = (rect(&rt, "preview-f3"), rect(&rt, "preview-f1"));
    assert!(f3[0] < f1[0], "the preview shows the file's order");
}

#[test]
fn dragging_f3_before_f1_and_pressing_save_writes_that_config_and_says_saved() {
    let path = fresh_dir("save_in_configurator", "drag_and_save").join("config.toml");
    let mut rt = configurator(path.clone());
    let f1 = rect(&rt, "preview-f1");
    drag_to(&mut rt, "preview-f3", f1[0] + f1[2] * 0.25);
    tap(&mut rt, "save");
    let expected = default_with_f_keys(&F3_FIRST);
    assert_eq!(rt.state.config, expected);
    assert_eq!(on_disk(&path), expected, "the file reads back equal to what was saved");
    let status = text_of(&rt, "status").unwrap_or_default();
    assert!(status.starts_with("saved"), "the status says saved: {status:?}");
}

#[test]
fn a_config_with_an_issue_is_not_saved_and_the_file_is_untouched() {
    let path = fresh_dir("save_in_configurator", "issue_blocks_save").join("config.toml");
    let mut rt = configurator(path.clone());
    let before = fs::read(&path).expect("opening wrote the default config");
    tap(&mut rt, "preview-f1");
    type_into(&mut rt, "field-label", "");
    tap(&mut rt, "save");
    assert_eq!(fs::read(&path).unwrap(), before, "the file is byte for byte as it was");
    let status = text_of(&rt, "status").unwrap_or_default();
    assert!(status.contains("not saved"), "the status says not saved: {status:?}");
    assert!(status.contains("1 issue"), "the status counts the issue: {status:?}");
}

#[test]
fn an_invalid_config_file_is_left_untouched_and_the_editor_shows_the_default() {
    let path = fresh_dir("save_in_configurator", "invalid_file").join("config.toml");
    let mut future = default_with_f_keys(&F3_FIRST);
    future.version = 99;
    let text = to_toml(&future).unwrap();
    fs::write(&path, &text).unwrap();
    let rt = configurator(path.clone());
    assert_eq!(rt.state.config, Config::default(), "the editor shows the default layout");
    assert_eq!(fs::read_to_string(&path).unwrap(), text, "opening does not touch the invalid file");
    let status = text_of(&rt, "status").unwrap_or_default();
    assert!(status.contains("1 issue"), "the status names the issue count: {status:?}");
}
