//! Functional: one snapshot per editing state of the configurator, each from the real
//! `touchbar-customizer` binary (`--snap` + `--tree`, a named `--scene` or the flow script) with
//! its own temp `HOME`, checked by one fact of that state in the element tree. The PNGs are for a
//! human: with `TB_WRITE_SNAPSHOTS=1` they are copied to `out/app-<state>.png`. Needs a GPU
//! adapter (lavapipe is fine).
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod support {
    pub mod bin;
}

use std::ffi::OsStr;
use std::path::PathBuf;

use support::bin::*;

/// A fresh temp home and output directory for one state.
fn dirs(state: &str) -> (PathBuf, PathBuf) {
    let dir = fresh_dir("editing_snapshots", state);
    let home = dir.join("home");
    std::fs::create_dir_all(&home).unwrap();
    (home, dir)
}

fn scene(state: &str) -> Vec<Element> {
    let (home, dir) = dirs(state);
    snap(&home, &dir, state, &["--scene".as_ref(), OsStr::new(state)])
}

#[test]
fn the_idle_snapshot_has_nothing_selected() {
    let (home, dir) = dirs("idle");
    let tree = snap(&home, &dir, "idle", &[]);
    assert!(keyed(&tree, "panel-empty").is_some(), "the edit panel shows its empty hint");
    assert!(keyed(&tree, "field-label").is_none());
}

#[test]
fn the_selected_snapshot_shows_f3_in_the_edit_panel() {
    let tree = scene("selected");
    assert_eq!(text_of(&tree, "field-label").as_deref(), Some("F3"));
    assert_eq!(text_of(&tree, "panel-id").as_deref(), Some("f3"));
}

#[test]
fn the_dragging_snapshot_shows_f3_lifted_left_of_its_place() {
    let (home, dir) = dirs("dragging");
    let idle = snap(&home, &dir, "dragging-idle", &[]);
    let dragging = snap(&home, &dir, "dragging", &["--scene".as_ref(), "dragging".as_ref()]);
    let (rest, lifted) = (x_of(&idle, "preview-f3"), x_of(&dragging, "preview-f3"));
    assert!(lifted < rest, "the dragged F3 is drawn left of its place: {lifted} vs {rest}");
    assert_eq!(x_of(&dragging, "preview-f1"), x_of(&idle, "preview-f1"), "nothing is reordered yet");
}

#[test]
fn the_reordered_snapshot_shows_f3_before_f1() {
    let tree = scene("reordered");
    assert!(x_of(&tree, "preview-f3") < x_of(&tree, "preview-f1"));
    assert_eq!(text_of(&tree, "panel-id").as_deref(), Some("f3"), "F3 is still selected");
}

#[test]
fn the_added_snapshot_shows_the_new_button() {
    let tree = scene("added");
    assert!(keyed(&tree, "preview-button-1").is_some());
    assert_eq!(text_of(&tree, "field-label").as_deref(), Some("new"));
}

#[test]
fn the_issues_snapshot_lists_two_issues() {
    let tree = scene("issues");
    assert!(keyed(&tree, "issue-0").is_some());
    assert!(keyed(&tree, "issue-1").is_some());
    assert!(keyed(&tree, "no-issues").is_none());
}

#[test]
fn the_saved_snapshot_ends_the_flow_script_with_a_saved_status() {
    let (home, dir) = dirs("saved");
    let script = drag_f3_before_f1_script();
    let tree = snap(
        &home,
        &dir,
        "saved",
        &["--script".as_ref(), script.as_os_str(), "--at".as_ref(), "mark:end".as_ref()],
    );
    let status = text_of(&tree, "status").unwrap_or_default();
    assert!(status.starts_with("saved"), "status: {status:?}");
}

#[test]
fn the_fn_scene_still_previews_the_media_layer() {
    let tree = scene("fn");
    assert!(keyed(&tree, "preview-play-pause").is_some());
    assert!(keyed(&tree, "preview-f1").is_none());
}

#[test]
fn the_fn_ctrl_scene_still_previews_the_omarchy_layer() {
    let tree = scene("fn-ctrl");
    assert!(keyed(&tree, "preview-lock").is_some());
    assert!(keyed(&tree, "preview-f1").is_none());
}
