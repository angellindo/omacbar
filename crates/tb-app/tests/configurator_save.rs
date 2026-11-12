//! Baseline dirty state and one guarded save operation, observed through UI and real files.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod support {
    pub mod configurator;
    pub mod ui;
}
use rune::Key;
use std::fs;
use support::configurator::*;
use tb_core::{
    Chord,
    config::{ThemeSource, to_toml},
};

#[test]
fn edit_then_revert_clears_dirty_and_marks_only_the_layers_that_still_differ() {
    let mut rt = open("dirty_revert_layers");
    assert_clean(&rt);
    operate(&mut rt, "row-a");
    edit(&mut rt, "field-label", "changed default");
    assert_dirty(&rt, 1);
    assert!(shown(&rt, "dirty-layer-default"));
    operate(&mut rt, "tab-fn");
    operate(&mut rt, "row-a");
    edit(&mut rt, "field-label", "changed fn");
    assert_dirty(&rt, 2);
    assert!(shown(&rt, "dirty-layer-fn"));
    operate(&mut rt, "tab-default");
    operate(&mut rt, "row-a");
    edit(&mut rt, "field-label", "a");
    assert_dirty(&rt, 1);
    assert!(!shown(&rt, "dirty-layer-default"));
    assert!(shown(&rt, "dirty-layer-fn"));
    operate(&mut rt, "tab-fn");
    operate(&mut rt, "row-a");
    edit(&mut rt, "field-label", "a");
    assert_clean(&rt);
}

#[test]
fn reversed_reorder_and_add_remove_restore_baseline_including_absent_optional_layers() {
    let mut rt = open("dirty_structural_revert");
    let baseline = rt.state.config.clone();
    operate(&mut rt, "row-b");
    alt(&mut rt, Key::Left);
    assert_dirty(&rt, 1);
    alt(&mut rt, Key::Right);
    assert_eq!(rt.state.config, baseline);
    assert_clean(&rt);
    operate(&mut rt, "add-button");
    assert_dirty(&rt, 1);
    operate(&mut rt, "remove-button");
    assert_eq!(rt.state.config, baseline);
    assert_clean(&rt);
    operate(&mut rt, "tab-fnalt");
    assert_clean(&rt);
    operate(&mut rt, "add-group");
    assert_dirty(&rt, 1);
    operate(&mut rt, "remove-button");
    assert_eq!(rt.state.config, baseline, "the absent optional layer is absent again");
    assert_clean(&rt);
}

#[test]
fn an_originally_present_empty_layer_keeps_its_presence_and_explicit_gap_after_add_remove() {
    let mut config = fixture();
    config.layout.layers.push(tb_core::Layer { chord: Chord::FnAlt, group_gap: Some(64), groups: vec![] });
    let (mut rt, _) = open_config("present_empty_revert", &config, (1200.0, 760.0));
    operate(&mut rt, "tab-fnalt");
    operate(&mut rt, "add-group");
    operate(&mut rt, "remove-button");
    assert_eq!(rt.state.config, config, "loaded empty layer and its gap are baseline data");
    assert_clean(&rt);
}

#[test]
fn invalid_width_alone_is_dirty_but_equivalent_width_spelling_and_inactive_action_memory_are_clean() {
    let mut rt = open("draft_dirty");
    operate(&mut rt, "row-a");
    edit(&mut rt, "field-width", "invalid");
    assert_dirty(&rt, 1);
    assert_eq!(rt.state.config, fixture());
    edit(&mut rt, "field-width", "  auto  ");
    assert_clean(&rt);
    operate(&mut rt, "action-command");
    edit(&mut rt, "field-action", "printf remembered");
    operate(&mut rt, "action-none");
    assert_eq!(rt.state.config, fixture());
    assert_clean(&rt);
}

#[test]
fn another_layers_invalid_draft_blocks_the_actual_save_and_focused_save_shortcut_until_repaired() {
    let (mut rt, path) = open_config("global_save_guard", &fixture(), (1200.0, 760.0));
    let bytes = fs::read(&path).unwrap();
    operate(&mut rt, "tab-fn");
    operate(&mut rt, "row-a");
    edit(&mut rt, "field-width", "bad-fn-width");
    operate(&mut rt, "tab-default");
    operate(&mut rt, "row-b");
    edit(&mut rt, "field-label", "changed");
    assert_blocked(&rt, &["Fn", "a"]);
    for field in ["field-label", "field-width", "field-fill", "field-text", "field-border"] {
        operate(&mut rt, field);
        assert!(element(&rt, field).focused);
        save_shortcut(&mut rt);
        assert_eq!(fs::read(&path).unwrap(), bytes, "blocked Ctrl+S from focused {field}");
    }
    operate(&mut rt, "action-command");
    edit(&mut rt, "field-action", "printf valid");
    save_shortcut(&mut rt);
    assert_eq!(fs::read(&path).unwrap(), bytes, "command focus uses same global guard");
    operate(&mut rt, "save");
    assert_eq!(fs::read(&path).unwrap(), bytes, "disabled button is inert");
    let reason = format!("{} {}", words(&rt, "status"), words(&rt, "save-blocked-reason"));
    assert!(reason.to_lowercase().contains("not saved") && reason.contains('1'), "{reason}");
    operate(&mut rt, "tab-fn");
    operate(&mut rt, "row-a");
    edit(&mut rt, "field-width", "auto");
    operate(&mut rt, "tab-default");
    operate(&mut rt, "row-b");
    operate(&mut rt, "field-label");
    save_shortcut(&mut rt);
    assert_eq!(disk(&path), rt.state.config);
    assert_ne!(fs::read(&path).unwrap(), bytes);
    assert_clean(&rt);
}

#[test]
fn save_shortcut_works_from_each_field_without_swallowing_ordinary_text_or_select_all() {
    let (mut rt, path) = open_config("focused_shortcut", &fixture(), (1200.0, 760.0));
    operate(&mut rt, "row-a");
    for (field, value) in [
        ("field-label", "renamed"),
        ("field-width", "px 180"),
        ("field-fill", "accent"),
        ("field-text", "foreground"),
        ("field-border", "red"),
    ] {
        edit(&mut rt, field, value);
        assert!(element(&rt, field).focused);
        save_shortcut(&mut rt);
        assert_eq!(disk(&path), rt.state.config, "Ctrl+S from {field} saves actual current value");
        assert_clean(&rt);
        assert_eq!(words(&rt, field), value, "shortcut does not insert s or consume field text");
    }
    operate(&mut rt, "action-command");
    edit(&mut rt, "field-action", "printf command");
    save_shortcut(&mut rt);
    assert_eq!(disk(&path), rt.state.config);
    assert_clean(&rt);
    // Real select-all/backspace/typing stays available after the global shortcut.
    edit(&mut rt, "field-action", "printf replacement");
    assert_eq!(words(&rt, "field-action"), "printf replacement");
}

#[test]
fn successful_save_updates_the_baseline_and_reports_only_file_state_and_no_acknowledgement() {
    let (mut rt, path) = open_config("save_baseline", &fixture(), (1200.0, 760.0));
    assert!(words(&rt, "last-saved").to_lowercase().contains("not saved this session"));
    operate(&mut rt, "row-a");
    edit(&mut rt, "field-label", "new label");
    operate(&mut rt, "save");
    assert_eq!(disk(&path), rt.state.config);
    assert_clean(&rt);
    let status = words(&rt, "file-status").to_lowercase();
    assert!(status.contains("saved") && status.contains("file"), "{status}");
    assert!(status.contains("daemon") && status.contains("watch"), "{status}");
    assert!(
        status.contains("acknowledgement unavailable") || status.contains("acknowledgment unavailable"),
        "{status}"
    );
    for false_claim in ["simulator current", "simulator connected", "simulator synchronized"] {
        assert!(!all_words(&rt).to_lowercase().contains(false_claim));
    }
    edit(&mut rt, "field-label", "a");
    assert_dirty(&rt, 1 /* saved new label is now the baseline */);
}

#[test]
fn deterministic_write_failure_preserves_edits_baseline_and_previous_file_bytes() {
    let (mut rt, path) = open_config("write_failure", &fixture(), (1200.0, 760.0));
    operate(&mut rt, "row-a");
    edit(&mut rt, "field-label", "success baseline");
    operate(&mut rt, "save");
    let accepted_bytes = fs::read(&path).unwrap();
    let old_time = words(&rt, "last-saved");
    let dir = path.parent().unwrap().to_owned();
    let backup = dir.with_extension("accepted");
    if backup.exists() {
        fs::remove_dir_all(&backup).unwrap();
    }
    fs::rename(&dir, &backup).unwrap();
    fs::write(&dir, "a regular file blocks directory creation").unwrap();
    edit(&mut rt, "field-label", "failed edit remains");
    operate(&mut rt, "save");
    let preserved_bytes = fs::read(backup.join("config.toml")).unwrap();
    fs::remove_file(&dir).unwrap();
    fs::rename(&backup, &dir).unwrap();
    assert_dirty(&rt, 1);
    assert_eq!(button(&rt, Chord::Default, "a").label, "failed edit remains");
    assert_eq!(words(&rt, "last-saved"), old_time);
    assert_eq!(preserved_bytes, accepted_bytes);
    let status = words(&rt, "status");
    assert!(status.contains("not saved") && status.contains(&path.display().to_string()), "{status}");
    edit(&mut rt, "field-label", "success baseline");
    assert_clean(&rt /* failed write did not change accepted baseline */);
}

#[test]
fn invalid_startup_and_missing_home_explain_the_unsaved_replacement_and_unavailable_save() {
    let path = fresh_dir("configurator_contract", "invalid_startup").join("config.toml");
    fs::write(&path, "not valid TOML").unwrap();
    let rt = runtime(tb_app::State::open(Some(path.clone()), theme()), (1200.0, 760.0));
    assert_eq!(fs::read_to_string(&path).unwrap(), "not valid TOML");
    assert_eq!(rt.state.config, tb_core::config::Config::default());
    let summary = words(&rt, "dirty-summary").to_lowercase();
    assert!(summary.contains("unsaved") && summary.contains("replacement"), "{summary}");
    assert!(words(&rt, "last-saved").to_lowercase().contains("not saved this session"));
    let no_home = runtime(tb_app::State::open(None, theme()), (1200.0, 760.0));
    assert!(disabled(&no_home, "save"));
    assert!(words(&no_home, "save-blocked-reason").contains("HOME"));
}

#[test]
fn whole_config_changes_remain_dirty_even_when_no_layer_differs() {
    let mut rt = open("whole_config_dirty");
    rt.update(|s, _| {
        s.config.theme = ThemeSource::File { path: "palette.toml".into() };
    });
    settle(&mut rt);
    let summary = words(&rt, "dirty-summary").to_lowercase();
    assert!(summary.contains('0') && summary.contains("layer"), "layer count is zero: {summary}");
    assert!(
        summary.contains("theme") || summary.contains("config"),
        "whole-config change is explained: {summary}"
    );
    rt.update(|s, _| {
        s.config.theme = ThemeSource::Omarchy;
    });
    settle(&mut rt);
    assert_clean(&rt);
}

#[test]
fn warning_only_legacy_keys_round_trip_version_one_with_style_and_explicit_group_gaps() {
    let mut config = fixture();
    config.layout.layer_mut(Chord::FnCtrl).unwrap().group_gap = Some(64);
    config.layout.layer_mut(Chord::Default).unwrap().groups[0].buttons[0].style.fill = Some("accent".into());
    let (mut rt, path) = open_config("legacy_round_trip", &config, (1200.0, 760.0));
    assert!(!disabled(&rt, "save"));
    operate(&mut rt, "save");
    assert_eq!(disk(&path), config);
    assert_eq!(tb_core::config::parse(&fs::read_to_string(path).unwrap()).unwrap(), config);
    assert_eq!(config.version, 1);
    assert!(to_toml(&config).unwrap().contains("group-gap = 64"));
}
