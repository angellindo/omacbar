//! Shared validation rejects blank commands before `ConfigFile` writes anything.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod support {
    pub mod configurator;
    pub mod ui;
}
use std::fs;
use support::configurator::*;
use tb_core::{Action, Chord};

#[test]
fn empty_command_save_rejection_preserves_the_accepted_config_and_exact_file_bytes() {
    let path = fresh_dir("configurator_contract", "blank_command_file").join("config.toml");
    let mut file = tb_config::ConfigFile::new(&path);
    let config = fixture();
    file.save(config.clone(), &theme()).unwrap();
    let before = fs::read(&path).unwrap();
    let mut candidate = config.clone();
    candidate.layout.layer_mut(Chord::Fn).unwrap().groups[0].buttons[0].action =
        Action::Command { run: " \t ".into() };
    let error = file.save(candidate, &theme()).expect_err("whitespace-only command cannot be written");
    assert!(matches!(error, tb_config::ConfigFileError::Invalid(ref issues) if issues.len() == 1));
    assert_eq!(file.config(), &config);
    assert_eq!(fs::read(&path).unwrap(), before);
}

#[test]
fn command_diagnostics_remain_global_after_switching_to_a_valid_layer() {
    let (mut rt, path) = open_config("blank_command_ui", &fixture(), (1200.0, 760.0));
    let before = fs::read(&path).unwrap();
    operate(&mut rt, "row-a");
    operate(&mut rt, "action-command");
    edit(&mut rt, "field-action", " \t ");
    operate(&mut rt, "tab-fn");
    operate(&mut rt, "row-other");
    assert_blocked(&rt, &["Default", "a"]);
    save_shortcut(&mut rt);
    assert_eq!(fs::read(&path).unwrap(), before);
    operate(&mut rt, "tab-default");
    operate(&mut rt, "row-a");
    operate(&mut rt, "action-none");
    assert!(!disabled(&rt, "save"), "none is the supported no-op");
}
