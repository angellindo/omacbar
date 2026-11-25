//! Loading the default buttons (a factory reset) in the configurator is a draft: only the editor
//! and its preview change, the file on disk, its permission bits and the generation store stay
//! exactly as they were, and the editor says whether the theme source of the shown config is kept
//! (Omarchy, or the path of the selected theme file). Cancel brings back exactly what the editor
//! held before the reset: the shown layer, the selected button, the text in its fields and the
//! pending edits of other buttons. Saving the reset (the Save control or Ctrl+S) first keeps a
//! verified `factory-reset` generation of the exact bytes and permission bits it replaces, keeps
//! those bits on the new file, and the status names the generation's path. A save refused because
//! the file changed on disk (a conflict) writes and keeps nothing: the newer file stays, the
//! draft stays, and cancel is still offered.
//!
//! Every case drives the real configurator headless through `rune::Runtime`, opened as startup
//! opens it (`State::open_protected`) on a hand-written config (comments, mode 0640) in its own
//! temp profile under cargo's temp dir, with its own generation store (never `$HOME`, never the
//! real state dir). The store is read straight from disk, in the layout ADR-0015 fixes:
//! `<store>/<id>/manifest.toml` (`reason`, `kind`, `mode`, ...) and `<store>/<id>/config.toml`.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod support {
    pub mod configurator;
    pub mod ui;
}

use std::collections::BTreeMap;
use std::fs::{self, Permissions};
use std::os::unix::fs::{MetadataExt, PermissionsExt};
use std::path::{Path, PathBuf};

use rune::{Env, Runtime};
use support::configurator::*;
use tb_app::State;
use tb_config::{Backups, Generation, GenerationKind, Health};
use tb_core::config::{Config, ThemeSource, to_toml};
use tb_core::model::Layout;
use tb_core::{BarTheme, ButtonId, Chord};

const SUITE: &str = "configurator_factory_reset";
const SIZE: (f32, f32) = (1200.0, 760.0);
const TOKYO_NIGHT: &str = include_str!("../../../fixtures/omarchy-themes/tokyo-night/colors.toml");

/// One case's throwaway profile: the config at `<dir>/home/.config/touchbar-customizer/config.toml`
/// and the store `<dir>/state/touchbar-customizer/backups`.
struct Profile {
    dir: PathBuf,
    config: PathBuf,
    store: PathBuf,
}

impl Profile {
    fn new(test: &str) -> Self {
        let dir = fresh_dir(SUITE, test);
        let config = dir.join("home/.config/touchbar-customizer/config.toml");
        let store = dir.join("state/touchbar-customizer/backups");
        Self { dir, config, store }
    }

    /// Writes `config` as a person leaves it after editing by hand (comments a re-serialisation
    /// would drop), with `mode`. Returns the exact bytes.
    fn seed(&self, config: &Config, mode: u32) -> Vec<u8> {
        let bytes = hand_written(config);
        fs::create_dir_all(self.config.parent().unwrap()).unwrap();
        fs::write(&self.config, &bytes).unwrap();
        fs::set_permissions(&self.config, Permissions::from_mode(mode)).unwrap();
        bytes
    }

    /// The configurator as startup opens it: the file protected by this profile's store.
    fn open(&self, theme: BarTheme) -> Runtime<State> {
        let state = State::open_protected(Some(self.config.clone()), Some(Backups::at(&self.store)), theme);
        runtime(state, SIZE)
    }

    /// Every generation of this config, newest first, read by a fresh `Backups`.
    fn generations(&self) -> Vec<Generation> {
        if !self.store.exists() {
            return Vec::new();
        }
        Backups::at(&self.store).list(&self.config).unwrap()
    }

    /// The `reason` a generation's manifest records, read from disk.
    fn reason(&self, generation: &Generation) -> String {
        let text =
            fs::read_to_string(self.store.join(generation.id.to_string()).join("manifest.toml")).unwrap();
        text.lines()
            .filter_map(|line| line.split_once('='))
            .find(|(key, _)| key.trim() == "reason")
            .map_or_else(
                || panic!("generation {} records no reason: {text}", generation.id),
                |(_, value)| value.trim().trim_matches('"').to_owned(),
            )
    }

    /// Paths a status may use to name a generation's directory: as given, or canonical.
    fn generation_paths(&self, generation: &Generation) -> Vec<String> {
        let id = generation.id.to_string();
        let mut paths = vec![self.store.join(&id).display().to_string()];
        if let Ok(real) = fs::canonicalize(&self.store) {
            paths.push(real.join(&id).display().to_string());
        }
        paths
    }
}

fn hand_written(config: &Config) -> Vec<u8> {
    format!("# my touch bar, edited by hand\n\n{}\n# keep this note\n", to_toml(config).unwrap()).into_bytes()
}

/// Type, permission bits, bytes and inode of every path under `root` (empty when it does not
/// exist), never following symlinks.
fn tree(root: &Path) -> BTreeMap<PathBuf, (u32, u64, Option<Vec<u8>>)> {
    let mut out = BTreeMap::new();
    let mut stack = vec![root.to_path_buf()];
    while let Some(path) = stack.pop() {
        let Ok(meta) = fs::symlink_metadata(&path) else { continue };
        let bytes = meta.file_type().is_file().then(|| fs::read(&path).unwrap());
        if meta.is_dir() {
            stack.extend(fs::read_dir(&path).unwrap().flatten().map(|e| e.path()));
        }
        out.insert(path, (meta.permissions().mode() & 0o7777, meta.ino(), bytes));
    }
    out
}

fn mode(path: &Path) -> u32 {
    fs::symlink_metadata(path).unwrap().permissions().mode() & 0o7777
}

fn status(rt: &Runtime<State>) -> String {
    words(rt, "status")
}

/// The factory layout the shown config would get under the editor's theme.
fn factory(rt: &Runtime<State>) -> Layout {
    Config::default_for(&rt.state.theme).layout
}

/// The reset is pending: the editor and its preview show the factory layout as unsaved changes.
fn assert_factory_draft(rt: &Runtime<State>, expected: &Layout, theme: &ThemeSource) {
    assert_eq!(&rt.state.config.layout, expected, "the draft is the factory layout");
    assert_eq!(&rt.state.config.theme, theme, "the reset keeps the config's theme source");
    let summary = words(rt, "dirty-summary").to_lowercase();
    assert!(summary.contains("pending") || summary.contains("unsaved"), "the reset is unsaved: {summary}");
    assert!(shown(rt, "reset-cancel"), "a pending reset can be cancelled");
}

fn keeps_word(notice: &str) -> bool {
    ["kept", "keep", "keeps", "retained", "unchanged"].iter().any(|w| notice.contains(w))
}

/// No generation and no leftover temp in the store (a refused write may have created the empty
/// store directory itself, to lock it).
fn assert_nothing_kept(profile: &Profile) {
    assert!(profile.generations().is_empty(), "a refused save keeps no generation");
    let names: Vec<_> = fs::read_dir(&profile.store)
        .map(|entries| entries.flatten().map(|e| e.file_name()).collect())
        .unwrap_or_default();
    assert!(names.is_empty(), "nothing was left in the store: {names:?}");
}

/// A third party replaces the file atomically (temp + rename, no lock).
fn external_replace(path: &Path, bytes: &[u8]) {
    let staged = path.with_file_name(".config.toml.editor-tmp");
    fs::write(&staged, bytes).unwrap();
    fs::rename(staged, path).unwrap();
}

#[test]
fn loading_default_buttons_changes_only_the_draft_and_the_preview() {
    let profile = Profile::new("draft_only");
    let seeded = profile.seed(&fixture(), 0o640);
    let mut rt = profile.open(theme());
    assert_eq!(rt.state.config, fixture(), "the hand-written config was opened");
    let expected = factory(&rt);
    assert!(
        shown(&rt, "preview-a") && !shown(&rt, "preview-f1"),
        "the preview first draws the opened config"
    );
    let before = tree(&profile.dir);

    operate(&mut rt, "reset-factory");

    assert_factory_draft(&rt, &expected, &ThemeSource::Omarchy);
    let first = &expected.layer(Chord::Default).unwrap().groups[1].buttons[0];
    assert!(
        shows_text_inside(&rt, &format!("preview-{}", first.id), &first.label),
        "the preview draws the factory layout"
    );
    assert!(!shown(&rt, "preview-a"), "the preview no longer draws the opened layout");
    assert_eq!(tree(&profile.dir), before, "no file, mode, temp or generation changed on disk");
    assert_eq!(fs::read(&profile.config).unwrap(), seeded);
    assert_eq!(mode(&profile.config), 0o640);
    assert!(profile.generations().is_empty(), "loading the default buttons keeps no generation");
}

#[test]
fn the_reset_notice_says_the_omarchy_theme_source_is_kept() {
    let profile = Profile::new("notice_omarchy");
    profile.seed(&fixture(), 0o640);
    let mut rt = profile.open(theme());

    operate(&mut rt, "reset-factory");

    assert_eq!(rt.state.config.theme, ThemeSource::Omarchy);
    let notice = words(&rt, "reset-notice").to_lowercase();
    assert!(
        notice.contains("omarchy") && keeps_word(&notice),
        "the notice says the Omarchy source is kept: {notice}"
    );
}

#[test]
fn the_reset_notice_names_the_selected_theme_file_it_keeps() {
    let profile = Profile::new("notice_theme_file");
    let colors = profile.dir.join("themes/external/colors.toml");
    fs::create_dir_all(colors.parent().unwrap()).unwrap();
    fs::write(&colors, TOKYO_NIGHT).unwrap();
    let source = ThemeSource::File { path: colors.display().to_string() };
    let own = Config { theme: source.clone(), ..fixture() };
    profile.seed(&own, 0o640);
    let mut rt = profile.open(theme());
    assert_eq!(rt.state.config, own, "the config selecting a theme file was opened");
    let expected = factory(&rt);

    operate(&mut rt, "reset-factory");

    assert_factory_draft(&rt, &expected, &source);
    let notice = words(&rt, "reset-notice");
    assert!(
        notice.contains(&colors.display().to_string()) && keeps_word(&notice.to_lowercase()),
        "the notice names the selected theme file it keeps: {notice}"
    );
}

#[test]
fn cancel_brings_back_the_layer_selection_and_pending_edits_exactly() {
    let profile = Profile::new("cancel");
    let seeded = profile.seed(&fixture(), 0o640);
    let mut rt = profile.open(theme());
    operate(&mut rt, "row-b");
    edit(&mut rt, "field-label", "pending b");
    operate(&mut rt, "row-c");
    edit(&mut rt, "field-width", "bad-c");
    operate(&mut rt, "tab-fn");
    operate(&mut rt, "row-other");
    edit(&mut rt, "field-label", "draft other");
    let config = rt.state.config.clone();
    let summary = words(&rt, "dirty-summary");
    let blocked = words(&rt, "save-blocked-reason");
    assert!(blocked.contains('c'), "the invalid width of c blocks saving: {blocked}");
    let expected = factory(&rt);
    let before = tree(&profile.dir);

    operate(&mut rt, "reset-factory");
    assert_factory_draft(&rt, &expected, &ThemeSource::Omarchy);
    operate(&mut rt, "reset-cancel");

    assert_eq!(rt.state.config, config, "the pre-reset draft is back, pending edits included");
    assert_eq!(rt.state.chord, Chord::Fn, "the shown layer is back");
    assert_eq!(rt.state.selected, Some(ButtonId::new("other")), "the selection is back");
    assert_eq!(words(&rt, "field-label"), "draft other", "the selected button's field text is back");
    assert_eq!(words(&rt, "dirty-summary"), summary);
    assert_eq!(words(&rt, "save-blocked-reason"), blocked, "the invalid width draft still blocks saving");
    assert!(shows_text_inside(&rt, "preview-other", "draft other"), "the preview draws the pre-reset draft");
    assert!(!shown(&rt, "reset-cancel"), "nothing is left to cancel");
    operate(&mut rt, "tab-default");
    operate(&mut rt, "row-c");
    assert_eq!(words(&rt, "field-width"), "bad-c", "another button's pending width text is back");
    operate(&mut rt, "row-b");
    assert_eq!(words(&rt, "field-label"), "pending b", "another button's pending label is back");
    assert_eq!(tree(&profile.dir), before, "reset and cancel changed nothing on disk");
    assert_eq!(fs::read(&profile.config).unwrap(), seeded);
}

/// Seeds the hand-written fixture (0640), loads the default buttons, saves with `save`, and checks
/// the saved file, the one new generation and the status.
fn reset_then_save(test: &str, save: fn(&mut Runtime<State>)) {
    let profile = Profile::new(test);
    let seeded = profile.seed(&fixture(), 0o640);
    let mut rt = profile.open(theme());
    let expected = factory(&rt);
    let earlier = profile.generations();
    operate(&mut rt, "reset-factory");

    save(&mut rt);

    let saved = disk(&profile.config);
    assert_eq!(saved.layout, expected, "the factory layout was saved");
    assert_eq!(saved.theme, ThemeSource::Omarchy, "the theme source was kept");
    assert_eq!(saved.version, fixture().version);
    assert_eq!(mode(&profile.config), 0o640, "the saved file keeps the replaced file's permission bits");
    let generations = profile.generations();
    assert_eq!(generations.len(), earlier.len() + 1, "exactly one generation was kept: {generations:?}");
    let kept = &generations[0];
    assert!(!earlier.iter().any(|g| g.id == kept.id), "the newest generation is the new one");
    assert_eq!(profile.reason(kept), "factory-reset", "{kept:?}");
    assert!(matches!(kept.kind, GenerationKind::File), "{kept:?}");
    assert!(matches!(kept.health, Health::Verified), "{kept:?}");
    assert_eq!(kept.mode, 0o640, "the generation records the replaced permission bits");
    let data = profile.store.join(kept.id.to_string()).join("config.toml");
    assert!(fs::symlink_metadata(&data).unwrap().file_type().is_file());
    assert_eq!(fs::read(&data).unwrap(), seeded, "the generation holds the exact replaced bytes");
    Backups::at(&profile.store).verify(&kept.id).unwrap();
    let status = status(&rt);
    assert!(
        profile.generation_paths(kept).iter().any(|p| status.contains(p.as_str())),
        "the status names the generation's path {:?}: {status}",
        profile.generation_paths(kept)
    );
    assert!(!status.to_lowercase().contains("not saved"), "{status}");
    assert_clean(&rt);
    assert!(!shown(&rt, "reset-cancel"), "a saved reset has nothing to cancel");
}

#[test]
fn save_after_a_reset_keeps_a_verified_factory_reset_generation_and_names_its_path() {
    reset_then_save("save_control", |rt| operate(rt, "save"));
}

#[test]
fn ctrl_s_after_a_reset_keeps_a_verified_factory_reset_generation_and_names_its_path() {
    reset_then_save("ctrl_s", save_shortcut);
}

#[test]
fn a_conflict_on_reset_save_keeps_the_newer_file_the_draft_and_cancel() {
    let profile = Profile::new("conflict");
    profile.seed(&fixture(), 0o640);
    let mut rt = profile.open(theme());
    let expected = factory(&rt);
    operate(&mut rt, "reset-factory");
    let mut newer_config = fixture();
    "newer"
        .clone_into(&mut newer_config.layout.layer_mut(Chord::Default).unwrap().groups[0].buttons[0].label);
    let newer = hand_written(&newer_config);
    external_replace(&profile.config, &newer);
    let home = profile.dir.join("home");
    let before = tree(&home);

    operate(&mut rt, "save");

    let said = status(&rt).to_lowercase();
    assert!(said.contains("not saved") && said.contains("changed on disk"), "{said}");
    assert_eq!(tree(&home), before, "the refused save wrote nothing in the profile");
    assert_nothing_kept(&profile);
    assert_eq!(fs::read(&profile.config).unwrap(), newer);
    assert_factory_draft(&rt, &expected, &ThemeSource::Omarchy);
    assert!(shown(&rt, "conflict-reload") && shown(&rt, "conflict-overwrite"), "both resolutions offered");

    save_shortcut(&mut rt);
    assert_eq!(tree(&home), before, "Ctrl+S in the conflict wrote nothing");
    assert_nothing_kept(&profile);
    assert_factory_draft(&rt, &expected, &ThemeSource::Omarchy);

    operate(&mut rt, "reset-cancel");
    assert_eq!(rt.state.config, fixture(), "cancel brings back the pre-reset draft");
    assert_eq!(tree(&home), before, "cancel changed nothing on disk");
    assert_nothing_kept(&profile);
}

#[test]
fn the_reset_scene_shows_the_factory_draft() {
    let profile = Profile::new("scene");
    let seeded = profile.seed(&fixture(), 0o640);
    let mut state =
        State::open_protected(Some(profile.config.clone()), Some(Backups::at(&profile.store)), theme());
    let expected = Config::default_for(&state.theme).layout;
    let spec = tb_app::spec();
    let (_, scene) =
        spec.scenes.iter().find(|(name, _)| *name == "reset").expect("the configurator has a `reset` scene");
    let before = tree(&profile.dir);

    scene(&mut state);

    let mut rt = Runtime::new(state, tb_app::view, SIZE, Env::new(true, 1.0));
    rt.on_key = Some(tb_app::on_key);
    rt.frame(0.0);
    settle(&mut rt);
    assert_factory_draft(&rt, &expected, &ThemeSource::Omarchy);
    assert!(shown(&rt, "reset-notice"), "the scene shows the reset notice");
    assert_eq!(tree(&profile.dir), before);
    assert_eq!(fs::read(&profile.config).unwrap(), seeded);
}
