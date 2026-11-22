//! Two configurators opened on one config file, and an intervening atomic save. A stale editor
//! never replaces the newer file: clean, it shows the newer file and writes nothing; dirty, it
//! reports the conflict, keeps its draft (fields, preview, pending changes) through Save and
//! Ctrl+S, and offers `conflict-reload` (show the newer file, drop the draft) or
//! `conflict-overwrite` (save the draft, keep the replaced bytes as `config.toml.bak`; refused
//! again if the file changed once more; a backup already there is never replaced). Symlinks and
//! removed files are left alone; a newer file that was removed or whose selected theme fails is
//! never adopted and never replaced by defaults, and a dirty editor keeps its draft. Conflict-free
//! saves still reach the daemon's watch exactly once. Observed through the real Runtime and files.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod support {
    pub mod configurator;
    pub mod ui;
}

use std::fs::{self, Permissions};
use std::os::unix::fs::{MetadataExt, PermissionsExt, symlink};
use std::path::{Path, PathBuf};
use std::sync::mpsc::{Receiver, RecvTimeoutError, channel};
use std::thread;
use std::time::Duration;

use rune::Runtime;
use support::configurator::*;
use tb_app::State;
use tb_config::{ConfigFile, ConfigFileError, ConfigWatch};
use tb_core::Chord;
use tb_core::config::{Config, ThemeSource, to_toml};

const SIZE: (f32, f32) = (1200.0, 760.0);
const CHANGE: Duration = Duration::from_secs(10);
const QUIET: Duration = Duration::from_millis(500);

/// A second configurator opened on the same file, independently of the first.
fn second_editor(path: &Path) -> Runtime<State> {
    runtime(State::open(Some(path.to_path_buf()), theme()), SIZE)
}

/// Relabels button `id` of the shown layer and presses the actual Save control.
fn save_label(rt: &mut Runtime<State>, id: &str, label: &str) {
    operate(rt, &format!("row-{id}"));
    edit(rt, "field-label", label);
    operate(rt, "save");
}

fn status(rt: &Runtime<State>) -> String {
    words(rt, "status").to_lowercase()
}

fn with_label(config: &Config, id: &str, label: &str) -> Config {
    let mut config = config.clone();
    let layer = config.layout.layer_mut(Chord::Default).unwrap();
    let button =
        layer.groups.iter_mut().flat_map(|g| g.buttons.iter_mut()).find(|b| b.id.as_str() == id).unwrap();
    label.clone_into(&mut button.label);
    config
}

/// `config` with comments and blank lines a re-serialisation would drop.
fn hand_written(config: &Config) -> String {
    format!("# edited by hand\n\n{}\n# keep this note\n", to_toml(config).unwrap())
}

/// An editor outside the configurator replacing the file atomically (plain temp + rename).
fn external_rename(path: &Path, text: &str) {
    let staged = path.with_file_name(".external-editor.tmp");
    fs::write(&staged, text).unwrap();
    fs::rename(&staged, path).unwrap();
}

fn ino(path: &Path) -> u64 {
    fs::symlink_metadata(path).unwrap().ino()
}

fn bak_of(path: &Path) -> PathBuf {
    path.with_file_name("config.toml.bak")
}

/// Editor B holds an unsaved label `draft B` on `b`; editor A then relabels `a` to `from A` and
/// saves; B presses Save. Returns B, the path and the newer bytes A wrote.
fn conflicted(test: &str) -> (Runtime<State>, PathBuf, Vec<u8>) {
    let (mut a, path) = open_config(test, &fixture(), SIZE);
    let mut b = second_editor(&path);
    operate(&mut b, "row-b");
    edit(&mut b, "field-label", "draft B");
    save_label(&mut a, "a", "from A");
    let newer = fs::read(&path).unwrap();

    operate(&mut b, "save");

    assert_eq!(fs::read(&path).unwrap(), newer, "a dirty stale save must not replace the newer file");
    (b, path, newer)
}

/// The draft is still everywhere B shows it, and the conflict can be resolved.
fn assert_draft_kept_in_conflict(b: &Runtime<State>) {
    let status = status(b);
    assert!(status.contains("not saved") && status.contains("changed on disk"), "{status}");
    assert_eq!(words(b, "field-label"), "draft B");
    assert_eq!(button(b, Chord::Default, "b").label, "draft B");
    assert_eq!(button(b, Chord::Default, "a").label, "a", "B's view is not silently replaced");
    assert!(shows_text_inside(b, "preview-b", "draft B"), "the preview still draws the draft");
    assert_dirty(b, 1);
    assert!(shown(b, "conflict-reload") && shown(b, "conflict-overwrite"), "both resolutions offered");
}

/// The status says the file is gone.
fn says_missing(status: &str) -> bool {
    ["removed", "missing", "not found", "no such file"].iter().any(|w| status.contains(w))
}

/// The fixture layout following a selected theme file that does not exist.
fn following_a_missing_theme(dir: &Path) -> String {
    let path = dir.join("missing-colors.toml").display().to_string();
    hand_written(&Config { theme: ThemeSource::File { path }, ..fixture() })
}

fn on_a_thread(mut watch: ConfigWatch) -> Receiver<Result<(), ConfigFileError>> {
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

fn one_change(changes: &Receiver<Result<(), ConfigFileError>>) {
    match changes.recv_timeout(CHANGE) {
        Ok(Ok(())) => {}
        other => panic!("expected a change, got {other:?}"),
    }
}

fn no_change(changes: &Receiver<Result<(), ConfigFileError>>) {
    match changes.recv_timeout(QUIET) {
        Err(RecvTimeoutError::Timeout) => {}
        other => panic!("expected no change, got {other:?}"),
    }
}

#[test]
fn a_clean_stale_editor_pressing_save_keeps_the_newer_file_and_shows_it() {
    let (mut a, path) = open_config("conflict_clean_stale", &fixture(), SIZE);
    let mut b = second_editor(&path);
    save_label(&mut a, "a", "from A");
    let newer = fs::read(&path).unwrap();
    let inode = ino(&path);
    assert_clean(&b);

    operate(&mut b, "save");

    assert_eq!(fs::read(&path).unwrap(), newer, "the newer file is kept byte for byte");
    assert_eq!(ino(&path), inode, "nothing was renamed over it");
    assert_eq!(button(&b, Chord::Default, "a").label, "from A", "the clean editor shows the newer file");
    assert_eq!(b.state.config, disk(&path));
    assert_clean(&b);
    let status = status(&b);
    assert!(status.contains("changed on disk") && status.contains("nothing saved"), "{status}");
    assert!(!shown(&b, "conflict-reload") && !shown(&b, "conflict-overwrite"), "nothing to resolve");

    save_label(&mut b, "b", "from B");

    assert_eq!(disk(&path), b.state.config, "the next save after adopting is an ordinary save");
    assert_eq!(button(&b, Chord::Default, "a").label, "from A", "A's edit survives B's save");
}

#[test]
fn a_clean_stale_editor_keeps_a_hand_written_newer_file_byte_for_byte() {
    let (mut b, path) = open_config("conflict_clean_by_hand", &fixture(), SIZE);
    let newer = hand_written(&with_label(&fixture(), "a", "by hand"));
    external_rename(&path, &newer);
    fs::set_permissions(&path, Permissions::from_mode(0o600)).unwrap();
    let inode = ino(&path);

    operate(&mut b, "save");

    assert_eq!(fs::read_to_string(&path).unwrap(), newer, "comments and formatting are kept");
    assert_eq!(fs::metadata(&path).unwrap().permissions().mode() & 0o7777, 0o600);
    assert_eq!(ino(&path), inode);
    assert!(fs::symlink_metadata(&path).unwrap().file_type().is_file());
    assert_eq!(button(&b, Chord::Default, "a").label, "by hand");
    assert_clean(&b);
}

#[test]
fn a_dirty_stale_editor_reports_the_conflict_and_keeps_its_draft_through_save_and_ctrl_s() {
    let (mut b, path, newer) = conflicted("conflict_dirty_stale");
    assert_draft_kept_in_conflict(&b);

    operate(&mut b, "field-label");
    save_shortcut(&mut b);

    assert_eq!(fs::read(&path).unwrap(), newer, "Ctrl+S repeats the same refused check");
    assert_draft_kept_in_conflict(&b);
    assert!(!bak_of(&path).exists(), "a refused save makes no backup");
}

#[test]
fn reloading_after_a_conflict_shows_the_newer_file_and_drops_the_draft() {
    let (mut b, path, newer) = conflicted("conflict_reload");

    operate(&mut b, "conflict-reload");

    assert_eq!(fs::read(&path).unwrap(), newer, "reloading writes nothing");
    assert_eq!(b.state.config, disk(&path));
    assert_eq!(button(&b, Chord::Default, "a").label, "from A");
    assert_eq!(button(&b, Chord::Default, "b").label, "b");
    assert!(!all_words(&b).contains("draft B"), "the draft is gone everywhere");
    assert_clean(&b);
    assert!(!shown(&b, "conflict-reload") && !shown(&b, "conflict-overwrite"));
    assert!(!bak_of(&path).exists());
}

#[test]
fn overwriting_after_a_conflict_saves_the_draft_and_keeps_the_replaced_bytes_as_bak() {
    let (mut b, path, newer) = conflicted("conflict_overwrite");

    operate(&mut b, "conflict-overwrite");

    assert_eq!(disk(&path), b.state.config, "the draft is saved");
    assert_eq!(button(&b, Chord::Default, "b").label, "draft B");
    assert_eq!(fs::read(bak_of(&path)).unwrap(), newer, "the replaced version, byte for byte");
    assert_clean(&b);
    assert!(status(&b).starts_with("saved config"), "{}", status(&b));
    assert!(!shown(&b, "conflict-reload") && !shown(&b, "conflict-overwrite"));
    let reopened = second_editor(&path);
    assert_eq!(button(&reopened, Chord::Default, "b").label, "draft B");
}

#[test]
fn an_overwrite_after_a_further_change_is_refused_again_and_a_second_overwrite_wins() {
    let (mut b, path, _) = conflicted("conflict_overwrite_retry");
    let third = hand_written(&with_label(&fixture(), "c", "third"));
    external_rename(&path, &third);

    operate(&mut b, "conflict-overwrite");

    assert_eq!(fs::read_to_string(&path).unwrap(), third, "an overwrite is never a blind force");
    assert!(!bak_of(&path).exists(), "a refused overwrite makes no backup");
    assert_draft_kept_in_conflict(&b);

    operate(&mut b, "conflict-overwrite");

    assert_eq!(disk(&path), b.state.config);
    assert_eq!(fs::read_to_string(bak_of(&path)).unwrap(), third, "the version acknowledged last");
    assert_clean(&b);
}

#[test]
fn a_clean_stale_editor_whose_file_was_removed_writes_nothing_and_says_so() {
    let (mut b, path) = open_config("conflict_clean_removed", &fixture(), SIZE);
    fs::remove_file(&path).unwrap();

    operate(&mut b, "save");

    assert!(fs::symlink_metadata(&path).is_err(), "nothing is written in place of a removed file");
    assert!(fs::read_dir(path.parent().unwrap()).unwrap().next().is_none());
    assert_eq!(b.state.config, fixture());
    let status = status(&b);
    assert!(status.contains("removed") && status.contains("nothing saved"), "{status}");
}

#[test]
fn a_dirty_stale_editor_whose_file_was_removed_keeps_its_draft_and_overwrite_creates_it_without_bak() {
    let (mut b, path) = open_config("conflict_dirty_removed", &fixture(), SIZE);
    operate(&mut b, "row-b");
    edit(&mut b, "field-label", "draft B");
    fs::remove_file(&path).unwrap();

    operate(&mut b, "save");

    assert!(fs::symlink_metadata(&path).is_err());
    assert!(status(&b).contains("not saved"), "{}", status(&b));
    assert_eq!(words(&b, "field-label"), "draft B");
    assert_dirty(&b, 1);

    operate(&mut b, "conflict-overwrite");

    assert_eq!(disk(&path), b.state.config);
    assert!(!bak_of(&path).exists(), "nothing was replaced, so nothing is backed up");
    assert_clean(&b);
}

#[test]
fn a_clean_stale_editor_facing_an_invalid_newer_file_writes_nothing_and_names_the_issues() {
    let (mut b, path) = open_config("conflict_clean_invalid", &fixture(), SIZE);
    external_rename(&path, "not valid TOML");

    operate(&mut b, "save");

    assert_eq!(fs::read_to_string(&path).unwrap(), "not valid TOML");
    assert_eq!(b.state.config, fixture(), "the editor keeps what it shows");
    let status = status(&b);
    assert!(status.contains("issue") && status.contains("nothing saved"), "{status}");
}

#[test]
fn stale_editors_through_a_symlink_leave_the_link_and_its_target_untouched() {
    let dir = fresh_dir("configurator_contract", "conflict_symlink");
    let real = dir.join("dotfiles");
    fs::create_dir(&real).unwrap();
    let target = real.join("config.toml");
    fs::write(&target, to_toml(&fixture()).unwrap()).unwrap();
    let link = dir.join("config.toml");
    symlink(&target, &link).unwrap();
    let mut dirty = second_editor(&link);
    let mut clean = second_editor(&link);
    assert_eq!(dirty.state.config, fixture());
    operate(&mut dirty, "row-b");
    edit(&mut dirty, "field-label", "draft B");
    let newer = hand_written(&with_label(&fixture(), "a", "dotfiles"));
    fs::write(&target, &newer).unwrap();

    operate(&mut dirty, "save");
    operate(&mut clean, "save");

    assert!(fs::symlink_metadata(&link).unwrap().file_type().is_symlink(), "the link stays a link");
    assert_eq!(fs::read_link(&link).unwrap(), target);
    assert_eq!(fs::read_to_string(&target).unwrap(), newer);
    assert_draft_kept_in_conflict(&dirty);
    assert_eq!(button(&clean, Chord::Default, "a").label, "dotfiles");
    assert_clean(&clean);
}

#[test]
fn conflict_free_saves_reach_the_daemon_once_and_a_refused_save_does_not() {
    let (mut a, path) = open_config("conflict_daemon_view", &fixture(), SIZE);
    let mut b = second_editor(&path);
    let mut daemon = ConfigFile::new(&path);
    daemon.load_resolved(&theme()).unwrap();
    let changes = on_a_thread(daemon.watch().unwrap());
    operate(&mut b, "row-b");
    edit(&mut b, "field-label", "draft B");

    save_label(&mut a, "a", "from A");

    one_change(&changes);
    no_change(&changes);
    daemon.reload_resolved(&theme()).unwrap();
    assert_eq!(daemon.config(), &a.state.config);

    operate(&mut b, "save");

    no_change(&changes);

    operate(&mut b, "conflict-overwrite");

    one_change(&changes);
    no_change(&changes);
    daemon.reload_resolved(&theme()).unwrap();
    assert_eq!(daemon.config(), &b.state.config);
}

#[test]
fn an_overwrite_never_replaces_a_backup_that_was_already_there() {
    let (mut b, path, newer) = conflicted("conflict_overwrite_existing_bak");
    let dir = path.parent().unwrap().to_path_buf();
    let unrelated = "my own backup, not made by the configurator\n";
    fs::write(bak_of(&path), unrelated).unwrap();

    operate(&mut b, "conflict-overwrite");

    assert_eq!(fs::read_to_string(bak_of(&path)).unwrap(), unrelated, "the earlier backup is kept as it was");
    let status = status(&b);
    if status.starts_with("saved config") {
        assert_eq!(disk(&path), b.state.config, "the draft is saved");
        let names: Vec<String> = fs::read_dir(&dir)
            .unwrap()
            .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
            .filter(|n| n != "config.toml" && n != "config.toml.bak")
            .filter(|n| fs::read(dir.join(n)).ok().as_deref() == Some(newer.as_slice()))
            .collect();
        assert_eq!(names.len(), 1, "the replaced version is kept in one new file");
        assert!(status.contains(&names[0].to_lowercase()), "the status names the new backup: {status}");
        assert_clean(&b);
    } else {
        assert_eq!(fs::read(&path).unwrap(), newer, "a refused overwrite writes nothing");
        assert!(status.contains("not saved"), "{status}");
        assert_eq!(words(&b, "field-label"), "draft B");
        assert_dirty(&b, 1);
    }
}

#[test]
fn reloading_after_a_conflict_when_the_file_was_removed_writes_no_default_and_keeps_the_draft() {
    let (mut b, path, _) = conflicted("conflict_reload_removed");
    fs::remove_file(&path).unwrap();

    operate(&mut b, "conflict-reload");

    assert!(fs::symlink_metadata(&path).is_err(), "no default is written in place of the removed file");
    assert!(fs::read_dir(path.parent().unwrap()).unwrap().next().is_none(), "nothing is created");
    assert_eq!(words(&b, "field-label"), "draft B");
    assert_eq!(button(&b, Chord::Default, "b").label, "draft B");
    assert_dirty(&b, 1);
    let status = status(&b);
    assert!(says_missing(&status), "the status says the file is gone: {status}");
}

#[test]
fn a_clean_stale_editor_never_adopts_a_newer_file_whose_selected_theme_fails() {
    let (mut b, path) = open_config("conflict_clean_theme_fails", &fixture(), SIZE);
    let newer = following_a_missing_theme(path.parent().unwrap());
    external_rename(&path, &newer);

    operate(&mut b, "save");

    assert_eq!(fs::read_to_string(&path).unwrap(), newer, "the newer file is kept byte for byte");
    assert_eq!(b.state.config, fixture(), "a layout whose theme does not resolve is not accepted");
    let status = status(&b);
    assert!(status.contains("nothing saved") || status.contains("not saved"), "{status}");
    assert!(!status.starts_with("saved"), "{status}");
}

#[test]
fn reloading_after_a_conflict_never_accepts_a_newer_file_whose_selected_theme_fails() {
    let (mut b, path, _) = conflicted("conflict_reload_theme_fails");
    let newer = following_a_missing_theme(path.parent().unwrap());
    external_rename(&path, &newer);

    operate(&mut b, "conflict-reload");

    assert_eq!(fs::read_to_string(&path).unwrap(), newer, "reloading writes nothing");
    assert_eq!(words(&b, "field-label"), "draft B", "the draft is kept when the newer file is refused");
    assert_eq!(button(&b, Chord::Default, "b").label, "draft B");
    assert_eq!(button(&b, Chord::Default, "a").label, "a", "the refused layout is not shown");
    assert_dirty(&b, 1);
    assert!(status(&b).contains("missing-colors.toml"), "the status names the theme file: {}", status(&b));
}
