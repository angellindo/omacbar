//! Recovery in the configurator: the editor shows where the generations of the config are kept
//! (`backups-location`, readable text), lists the newest five of this config, newest first
//! (`recover-<id>`; one kept before a factory reset says "before factory reset"; an absence or a
//! damaged copy is listed with why and is never restored), and points to `touchbar-recover list`
//! for the rest (`recovery-more`). Tapping a listed generation restores exactly its bytes and
//! permission bits through the guarded write: the file it replaces is kept as a new generation,
//! the editor shows the restored file and the status names where the replaced one is kept.
//! A restore is refused while the editor has unsaved changes or a pending factory reset; over a
//! file changed or removed on disk it writes and keeps nothing (the editor's conflict rules).
//! The default buttons, restoring the personal file and the physical return to the key row are
//! three separate, labelled things; the last one is only a pointer to the human procedure.
//! Every control is reachable at the required window sizes, and every passage of text in the
//! region (the location, the pointer to the rest, the key-row pointer) can be read whole there,
//! scrolling through it when it is taller than the sidebar: also with store paths of 179, 212 and
//! 256 bytes, whatever cargo's temp dir is (`support/reading.rs`).
//!
//! Every case drives the real configurator headless through `rune::Runtime`, opened as startup
//! opens it (`State::open_protected`) on hand-written configs (comments, mode 0640) in its own
//! temp profile under cargo's temp dir, with its own generation store (never `$HOME`, never the
//! real state dir). Generation history is made by the real protected library writes, and the store
//! is read back from disk in the layout ADR-0015 fixes (`<store>/<id>/config.toml`).
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod support {
    pub mod configurator;
    pub mod reading;
    pub mod ui;
}

use std::collections::BTreeMap;
use std::fs::{self, Permissions};
use std::os::unix::fs::{MetadataExt, PermissionsExt};
use std::path::{Path, PathBuf};

use rune::{Cx, El, Env, Runtime, ScrollSpec, col, el, text};
use support::configurator::*;
use support::reading::{Reading, read_passage, reveal_control};
use tb_app::State;
use tb_config::{Backups, ConfigFile, Generation, GenerationKind, Health, Loaded, Reason};
use tb_core::Chord;
use tb_core::config::{Config, to_toml};

/// No word a restore message uses (`restore`, `recover`, `pending`) appears in the temp paths.
const SUITE: &str = "configurator_history";
const WINDOW: (f32, f32) = (1200.0, 760.0);
/// The window sizes the configurator must stay operable at (`configurator_shell.rs`).
const SIZES: [(f32, f32); 4] = [(1200.0, 760.0), (1200.0, 500.0), (900.0, 600.0), (640.0, 400.0)];
/// Store path lengths, in bytes, every size is also checked with, whatever cargo's temp dir is.
/// 179 and 212 are the lengths seen in review; at 640x400 the location then fits the sidebar with
/// less than a wheel notch to spare. 256 is the shortest padded length whose location (at most 20
/// characters a line) is always taller than the 283-point sidebar there, so it is read in views.
const LONG_STORES: [usize; 3] = [179, 212, 256];
/// The passages of the recovery region: text a person reads, never taps.
const PASSAGES: [&str; 3] = ["backups-location", "recovery-more", "key-row-return"];

/// One case's throwaway profile: the config at `<dir>/home/.config/touchbar-customizer/config.toml`
/// and the store `<dir>/state/touchbar-customizer/backups` (what `XDG_STATE_HOME=<dir>/state` names).
struct Profile {
    dir: PathBuf,
    config: PathBuf,
    store: PathBuf,
}

impl Profile {
    fn new(test: &str) -> Self {
        Self::at(fresh_dir(SUITE, test))
    }

    fn at(dir: PathBuf) -> Self {
        let config = dir.join("home/.config/touchbar-customizer/config.toml");
        let store = dir.join("state/touchbar-customizer/backups");
        Self { dir, config, store }
    }

    /// A profile whose store path is at least `len` bytes, the same on every run: when cargo's
    /// temp dir leaves it shorter, the case's directory gains one fixed segment of `x`.
    fn long(test: &str, len: usize) -> Self {
        let dir = fresh_dir(SUITE, test);
        let natural = Self::at(dir.clone()).store.as_os_str().len();
        let profile = if natural < len {
            Self::at(dir.join("x".repeat((len - natural - 1).max(1))))
        } else {
            Self::at(dir)
        };
        assert!(profile.store.as_os_str().len() >= len, "{}", profile.store.display());
        profile
    }

    /// The store path as given, or canonical.
    fn store_paths(&self) -> Vec<String> {
        let mut paths = vec![self.store.display().to_string()];
        paths.extend(fs::canonicalize(&self.store).ok().map(|p| p.display().to_string()));
        paths
    }

    fn backups(&self) -> Backups {
        Backups::at(&self.store)
    }

    /// The configurator as startup opens it, at `size`: the file protected by this store.
    fn open(&self, size: (f32, f32)) -> Runtime<State> {
        runtime(State::open_protected(Some(self.config.clone()), Some(self.backups()), theme()), size)
    }

    /// Every generation of this config, newest first, read by a fresh `Backups`.
    fn generations(&self) -> Vec<Generation> {
        if !self.store.exists() {
            return Vec::new();
        }
        self.backups().list(&self.config).unwrap()
    }

    fn kept_bytes(&self, generation: &Generation) -> Vec<u8> {
        fs::read(self.store.join(generation.id.to_string()).join("config.toml")).unwrap()
    }

    /// Each of `versions` in turn is the file a person left (`mode`), then replaced by a protected
    /// save, so the store keeps one generation of each, exactly; `current` is left on disk.
    fn history(&self, versions: &[Vec<u8>], current: &[u8], mode: u32) {
        for bytes in versions {
            by_hand(&self.config, bytes, mode);
            let mut file = ConfigFile::protected(&self.config, self.backups());
            assert_eq!(file.load(&theme()).unwrap(), Loaded::Read, "the hand-written version is valid");
            file.save(version(9_999).0, &theme()).unwrap();
        }
        by_hand(&self.config, current, mode);
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

/// Version `n` of the fixture as a person writes it: button `a` of the default layer reads `v<n>`,
/// with comments a re-serialisation would drop.
fn version(n: u32) -> (Config, Vec<u8>) {
    let mut config = fixture();
    config.layout.layer_mut(Chord::Default).unwrap().groups[0].buttons[0].label = format!("v{n}");
    let bytes = format!("# version {n}, written by hand\n\n{}\n# keep note {n}\n", to_toml(&config).unwrap());
    (config, bytes.into_bytes())
}

fn versions(range: std::ops::Range<u32>) -> Vec<Vec<u8>> {
    range.map(|n| version(n).1).collect()
}

/// Writes `bytes` with `mode`, as an editor outside the configurator would.
fn by_hand(path: &Path, bytes: &[u8], mode: u32) {
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, bytes).unwrap();
    fs::set_permissions(path, Permissions::from_mode(mode)).unwrap();
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

fn key(generation: &Generation) -> String {
    format!("recover-{}", generation.id)
}

/// Brings the control into view whole at `size` with real wheel input, then taps it.
fn press_at(rt: &mut Runtime<State>, key: &str, size: (f32, f32)) {
    reveal_control(rt, key, size);
    tap(rt, key);
}

/// Reads the passage keyed `key` whole at `size`; when it fits its viewport it is also fully
/// visible in that one view.
fn read_at(rt: &mut Runtime<State>, key: &str, size: (f32, f32), at: &str) -> Reading {
    let reading = read_passage(rt, key, size);
    let fits = reading.height <= reading.viewport + 1.0;
    assert_eq!(reading.views == 1, fits, "{key} takes one view exactly when it fits, at {at}: {reading:?}");
    assert!(!fits || fully_visible(rt, key, size), "{key} is reachable at {at}");
    reading
}

/// A third party replaces the file atomically (temp + rename, no lock).
fn external_replace(path: &Path, bytes: &[u8]) {
    let staged = path.with_file_name(".config.toml.editor-tmp");
    fs::write(&staged, bytes).unwrap();
    fs::rename(staged, path).unwrap();
}

fn numbers(text: &str) -> Vec<&str> {
    text.split(|c: char| !c.is_ascii_digit()).filter(|n| !n.is_empty()).collect()
}

/// Every element text, lowercased, one per entry.
fn texts(rt: &Runtime<State>) -> Vec<String> {
    rt.elements()
        .iter()
        .filter(|e| !e.exiting)
        .filter_map(|e| e.text.as_deref())
        .map(str::to_lowercase)
        .collect()
}

/// A successful restore of `chosen` over `replaced`: exact bytes and bits back, the replaced file
/// kept as the newest, verified `recover` generation, every earlier generation still listed and
/// verified, and the editor showing the restored file with the kept path in its status.
fn assert_restored(
    profile: &Profile,
    rt: &Runtime<State>,
    chosen: &[u8],
    replaced: &[u8],
    earlier: &[Generation],
) {
    assert_eq!(fs::read(&profile.config).unwrap(), chosen, "the chosen generation's exact bytes are back");
    assert_eq!(mode(&profile.config), 0o640, "the restored file has the kept permission bits");
    let now = profile.generations();
    assert_eq!(now.len(), earlier.len() + 1, "exactly one generation was kept: {now:?}");
    let kept = &now[0];
    assert!(!earlier.iter().any(|g| g.id == kept.id), "the newest generation is the new one");
    assert_eq!(kept.reason, Reason::Recover, "{kept:?}");
    assert_eq!(kept.kind, GenerationKind::File, "{kept:?}");
    assert_eq!(kept.health, Health::Verified, "{kept:?}");
    assert_eq!(kept.mode, 0o640, "{kept:?}");
    assert_eq!(profile.kept_bytes(kept), replaced, "the replaced file is kept, byte for byte");
    for g in earlier {
        let same = now.iter().find(|n| n.id == g.id).expect("no earlier generation is lost");
        assert_eq!(same.health, Health::Verified, "{same:?}");
    }
    let said = status(rt);
    assert!(
        said.to_lowercase().contains("restored") && !said.to_lowercase().contains("not restored"),
        "{said}"
    );
    assert!(
        profile.generation_paths(kept).iter().any(|p| said.contains(p.as_str())),
        "the status names where the replaced file is kept {:?}: {said}",
        profile.generation_paths(kept)
    );
    assert!(shown(rt, &key(kept)), "the new generation is listed at once");
}

#[test]
fn the_backups_location_is_shown_as_readable_text() {
    let profile = Profile::new("location");
    profile.history(&versions(0..1), &version(1).1, 0o640);
    let mut rt = profile.open(WINDOW);

    let reading = read_at(&mut rt, "backups-location", WINDOW, "1200x760");

    let shown = words(&rt, "backups-location");
    let paths = profile.store_paths();
    assert!(
        paths.iter().any(|p| shown.contains(p.as_str())),
        "the full store path {paths:?} is shown: {shown}"
    );
    assert_eq!(reading.views, 1, "{reading:?}");
    assert!(fully_visible(&rt, "backups-location", WINDOW));
}

#[test]
fn the_newest_five_generations_are_listed_newest_first_and_the_rest_point_to_the_terminal() {
    let profile = Profile::new("newest_five");
    profile.history(&versions(0..3), &version(3).1, 0o640);
    let other = profile.config.with_file_name("other.toml");
    let mut foreign = ConfigFile::protected(&other, profile.backups());
    foreign.load(&theme()).unwrap();
    foreign.save(version(50).0, &theme()).unwrap();
    profile.history(&versions(3..7), &version(7).1, 0o640);
    let all = profile.generations();
    assert_eq!(all.len(), 7, "{all:?}");
    let foreign_ids = profile.backups().list(&other).unwrap();
    assert_eq!(foreign_ids.len(), 2, "the other config kept its absence and its file: {foreign_ids:?}");

    let rt = profile.open(WINDOW);

    let tops: Vec<f32> = all[..5].iter().map(|g| rect(&rt, &key(g))[1]).collect();
    assert!(tops.windows(2).all(|w| w[0] < w[1]), "newest first, top to bottom: {tops:?}");
    for older in &all[5..] {
        assert!(!shown(&rt, &key(older)), "only the newest five are listed; {} is not", older.id);
    }
    for g in &foreign_ids {
        assert!(!shown(&rt, &key(g)), "generation {} of another config is never listed", g.id);
    }
    let more = words(&rt, "recovery-more");
    assert!(more.contains("touchbar-recover list"), "{more}");
    assert!(numbers(&more).contains(&"2"), "it says two more are kept: {more}");
}

#[test]
fn five_generations_are_all_listed_and_nothing_more_is_claimed() {
    let profile = Profile::new("exactly_five");
    profile.history(&versions(0..5), &version(5).1, 0o640);
    let all = profile.generations();
    assert_eq!(all.len(), 5);

    let rt = profile.open(WINDOW);

    for g in &all {
        assert!(shown(&rt, &key(g)), "{} is listed", g.id);
    }
    assert!(!shown(&rt, "recovery-more"), "nothing more to point to");
}

#[test]
fn restoring_a_chosen_generation_brings_back_its_exact_bytes_and_mode_and_keeps_the_replaced_file() {
    let profile = Profile::new("chosen");
    let (current, history) = (version(3).1, versions(0..3));
    profile.history(&history, &current, 0o640);
    let earlier = profile.generations();
    let middle = earlier.iter().find(|g| profile.kept_bytes(g) == history[1]).unwrap().clone();
    assert_ne!(middle.id, earlier[0].id, "the chosen one is not the newest");
    let mut rt = profile.open(WINDOW);

    press_at(&mut rt, &key(&middle), WINDOW);

    assert_restored(&profile, &rt, &history[1], &current, &earlier);
    assert_eq!(rt.state.config, version(1).0, "the editor shows the restored file");
    assert_clean(&rt);
}

#[test]
fn a_restore_is_listed_and_undone_after_the_configurator_reopens() {
    let profile = Profile::new("reopen");
    let (current, history) = (version(2).1, versions(0..2));
    profile.history(&history, &current, 0o640);
    let first = profile.generations();
    let mut rt = profile.open(WINDOW);
    press_at(&mut rt, &key(&first[1]), WINDOW);
    assert_restored(&profile, &rt, &history[0], &current, &first);
    drop(rt);
    let second = profile.generations();

    let mut reopened = profile.open(WINDOW);
    assert_eq!(reopened.state.config, version(0).0, "the reopened editor shows the restored file");
    for g in second.iter().take(5) {
        assert!(shown(&reopened, &key(g)), "{} survives the restart", g.id);
    }
    press_at(&mut reopened, &key(&second[0]), WINDOW);

    assert_restored(&profile, &reopened, &current, &history[0], &second);
    assert_eq!(reopened.state.config, version(2).0);
}

#[test]
fn a_factory_reset_generation_is_labelled_and_restores_the_personal_file() {
    let profile = Profile::new("factory_reset");
    let (mine, bytes) = version(7);
    by_hand(&profile.config, &bytes, 0o640);
    let mut rt = profile.open(WINDOW);
    operate(&mut rt, "reset-factory");
    operate(&mut rt, "save");
    let earlier = profile.generations();
    assert_eq!(earlier.len(), 1, "{earlier:?}");
    let before_reset = &earlier[0];
    assert_eq!(before_reset.reason, Reason::FactoryReset);
    let label = text_of(&rt, &key(before_reset))
        .unwrap_or_else(|| panic!("{} is listed with text", key(before_reset)))
        .to_lowercase();
    assert!(label.contains("before factory reset"), "{label}");
    let factory = fs::read(&profile.config).unwrap();

    press_at(&mut rt, &key(before_reset), WINDOW);

    assert_restored(&profile, &rt, &bytes, &factory, &earlier);
    assert_eq!(rt.state.config, mine, "the personal layout is back in the editor");
    let newest = &profile.generations()[0];
    let label = text_of(&rt, &key(newest))
        .unwrap_or_else(|| panic!("{} is listed with text", key(newest)))
        .to_lowercase();
    assert!(!label.contains("before factory reset"), "a restore's generation is not a reset's: {label}");
}

/// The restore of `id` did nothing: same disk, same editor; something says why, naming the restore.
fn assert_restore_refused(profile: &Profile, rt: &mut Runtime<State>, id: &str) {
    let before = tree(&profile.dir);
    let config = rt.state.config.clone();
    if shown(rt, id) && !disabled(rt, id) {
        press_at(rt, id, WINDOW);
    }
    assert_eq!(tree(&profile.dir), before, "nothing was written or kept");
    assert_eq!(rt.state.config, config, "the draft is kept");
    let why = texts(rt).into_iter().any(|t| {
        (t.contains("restore") || t.contains("recover"))
            && ["save or cancel", "unsaved", "pending"].iter().any(|w| t.contains(w))
    });
    assert!(why, "the editor says to save or cancel before restoring: {}", all_words(rt));
}

#[test]
fn restore_is_refused_while_there_are_unsaved_changes_or_a_pending_reset() {
    let profile = Profile::new("dirty");
    profile.history(&versions(0..1), &version(1).1, 0o640);
    let kept = profile.generations()[0].clone();

    let mut rt = profile.open(WINDOW);
    operate(&mut rt, "row-b");
    edit(&mut rt, "field-label", "pending b");
    assert_restore_refused(&profile, &mut rt, &key(&kept));
    assert_eq!(rt.state.config.layout.layer(Chord::Default).unwrap().groups[0].buttons[1].label, "pending b");

    let mut rt = profile.open(WINDOW);
    operate(&mut rt, "reset-factory");
    assert_restore_refused(&profile, &mut rt, &key(&kept));
    assert!(shown(&rt, "reset-cancel"), "the pending reset can still be cancelled");
}

#[test]
fn a_restore_over_a_file_changed_on_disk_keeps_the_newer_file_and_shows_it() {
    let profile = Profile::new("changed");
    profile.history(&versions(0..1), &version(1).1, 0o640);
    let earlier = profile.generations();
    let mut rt = profile.open(WINDOW);
    let (newer, newer_bytes) = version(2);
    external_replace(&profile.config, &newer_bytes);
    let store = tree(&profile.store);

    press_at(&mut rt, &key(&earlier[0]), WINDOW);

    assert_eq!(fs::read(&profile.config).unwrap(), newer_bytes, "the newer file stays");
    assert_eq!(tree(&profile.store), store, "nothing was kept");
    assert_eq!(profile.generations(), earlier);
    let said = status(&rt).to_lowercase();
    assert!(said.contains("changed on disk"), "{said}");
    assert_eq!(rt.state.config, newer, "the clean editor shows the newer file");
}

#[test]
fn a_restore_after_the_file_was_removed_writes_nothing_and_says_so() {
    let profile = Profile::new("removed");
    profile.history(&versions(0..1), &version(1).1, 0o640);
    let earlier = profile.generations();
    let mut rt = profile.open(WINDOW);
    fs::remove_file(&profile.config).unwrap();
    let before = tree(&profile.dir);

    press_at(&mut rt, &key(&earlier[0]), WINDOW);

    assert!(fs::symlink_metadata(&profile.config).is_err(), "no file was written");
    assert_eq!(tree(&profile.dir), before, "nothing was written or kept");
    let said = status(&rt).to_lowercase();
    assert!(said.contains("removed"), "{said}");
}

#[test]
fn damaged_and_absent_generations_are_shown_with_why_and_never_restored() {
    let profile = Profile::new("damaged_absent");
    let mut first = ConfigFile::protected(&profile.config, profile.backups());
    assert_eq!(first.load(&theme()).unwrap(), Loaded::WroteDefault);
    profile.history(&versions(0..1), &version(1).1, 0o640);
    let all = profile.generations();
    let absent = all.iter().find(|g| matches!(g.kind, GenerationKind::Absent { .. })).unwrap().clone();
    let damaged = all.iter().find(|g| g.kind == GenerationKind::File).unwrap().clone();
    let data = profile.store.join(damaged.id.to_string()).join("config.toml");
    fs::write(&data, b"# not what was kept\n").unwrap();
    assert!(matches!(
        profile.generations().iter().find(|g| g.id == damaged.id).unwrap().health,
        Health::Damaged(_)
    ));
    let mut rt = profile.open(WINDOW);

    let said = text_of(&rt, &key(&absent))
        .unwrap_or_else(|| panic!("{} is listed with text", key(&absent)))
        .to_lowercase();
    assert!(
        ["no file", "absent", "did not exist", "no config"].iter().any(|w| said.contains(w)),
        "the absence says there was no file: {said}"
    );
    let said = text_of(&rt, &key(&damaged))
        .unwrap_or_else(|| panic!("{} is listed with text", key(&damaged)))
        .to_lowercase();
    assert!(said.contains("damaged"), "the damaged copy says so: {said}");
    let config = rt.state.config.clone();
    let before = tree(&profile.dir);
    press_at(&mut rt, &key(&absent), WINDOW);
    press_at(&mut rt, &key(&damaged), WINDOW);

    assert_eq!(tree(&profile.dir), before, "neither was restored, nothing was kept");
    assert_eq!(rt.state.config, config);
}

#[test]
fn restoring_a_file_that_does_not_validate_is_exact_and_says_so() {
    let profile = Profile::new("invalid");
    let broken = b"# by hand, half edited\nnot = [ valid toml\n".to_vec();
    by_hand(&profile.config, &broken, 0o640);
    let mut file = ConfigFile::protected(&profile.config, profile.backups());
    assert!(matches!(file.load(&theme()).unwrap(), Loaded::Rejected(_)));
    file.save(version(1).0, &theme()).unwrap();
    let current = fs::read(&profile.config).unwrap();
    let earlier = profile.generations();
    let kept = earlier.iter().find(|g| profile.kept_bytes(g) == broken).unwrap().clone();
    let mut rt = profile.open(WINDOW);

    press_at(&mut rt, &key(&kept), WINDOW);

    assert_restored(&profile, &rt, &broken, &current, &earlier);
    let said = status(&rt).to_lowercase();
    assert!(
        ["issue", "not valid", "invalid"].iter().any(|w| said.contains(w)),
        "the status says the restored file does not validate: {said}"
    );
}

#[test]
fn the_default_buttons_the_personal_restore_and_the_key_row_return_are_separate_and_labelled() {
    let profile = Profile::new("three_things");
    profile.history(&versions(0..1), &version(1).1, 0o640);
    let kept = profile.generations()[0].clone();
    let mut rt = profile.open(WINDOW);
    let restore = key(&kept);

    let reset = words(&rt, "reset-factory").to_lowercase();
    assert!(
        (reset.contains("default") || reset.contains("factory")) && !reset.contains("restore"),
        "{reset}"
    );
    let restoring =
        text_of(&rt, &restore).unwrap_or_else(|| panic!("{restore} is listed with text")).to_lowercase();
    assert!(restoring.contains("restore") && !restoring.contains("default buttons"), "{restoring}");
    let back = words(&rt, "key-row-return");
    let lower = back.to_lowercase();
    assert!(back.contains("docs/hardware-runbook.md"), "names the runbook: {back}");
    assert!(lower.contains("physical return to key mode"), "names its section: {back}");
    assert!(
        ["does not", "never", "doesn't"].iter().any(|w| lower.contains(w)),
        "the editor never does it: {back}"
    );
    assert!(
        lower.contains("layout") && ["backup", "generation", "copies"].iter().any(|w| lower.contains(w)),
        "{back}"
    );
    assert!(
        ["stay", "kept", "remain"].iter().any(|w| lower.contains(w)),
        "the layout and copies stay: {back}"
    );
    let runbook = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../docs/hardware-runbook.md");
    assert!(fs::read_to_string(runbook).unwrap().contains("### Physical return to key mode"));
    let rects = ["reset-factory", restore.as_str(), "key-row-return"].map(|k| rect(&rt, k));
    for (i, a) in rects.iter().enumerate() {
        for b in &rects[i + 1..] {
            let apart =
                a[0] + a[2] <= b[0] || b[0] + b[2] <= a[0] || a[1] + a[3] <= b[1] || b[1] + b[3] <= a[1];
            assert!(apart, "the three controls never overlap: {rects:?}");
        }
    }
    let (before, config) = (tree(&profile.dir), rt.state.config.clone());
    press_at(&mut rt, "key-row-return", WINDOW);
    assert_eq!(tree(&profile.dir), before, "the key-row text changes nothing");
    assert_eq!(rt.state.config, config);
}

#[test]
fn every_recovery_control_is_reachable_and_restore_works_at_the_required_window_sizes() {
    for (n, size) in SIZES.into_iter().enumerate() {
        reach_and_restore(&Profile::new(&format!("reach_{n}")), size);
        for len in LONG_STORES {
            reach_and_restore(&Profile::long(&format!("reach_{n}_{len}"), len), size);
        }
    }
}

/// At `size`: every passage reads whole, every control is reached whole, and tapping the fourth
/// newest generation restores it.
fn reach_and_restore(profile: &Profile, size: (f32, f32)) {
    let history = versions(0..7);
    profile.history(&history, &version(7).1, 0o640);
    let all = profile.generations();
    let mut rt = profile.open(size);
    let at = format!("{size:?} with a {}-byte store path", profile.store.as_os_str().len());
    for k in PASSAGES {
        read_at(&mut rt, k, size, &at);
    }
    let shown = words(&rt, "backups-location");
    assert!(
        profile.store_paths().iter().any(|p| shown.contains(p.as_str())),
        "the full path at {at}: {shown}"
    );
    let mut keys = vec!["reset-factory".to_owned()];
    keys.extend(all[..5].iter().map(key));
    for k in &keys {
        reveal_control(&mut rt, k, size);
        assert!(fully_visible(&rt, k, size), "{k} is reachable at {at}");
    }
    let chosen = &all[3];
    let bytes = profile.kept_bytes(chosen);

    press_at(&mut rt, &key(chosen), size);

    assert_eq!(fs::read(&profile.config).unwrap(), bytes, "restore works at {at}");
    assert_eq!(profile.generations()[0].reason, Reason::Recover, "at {at}");
}

#[test]
fn the_recovery_scene_shows_the_location_and_the_generations() {
    let profile = Profile::new("scene");
    profile.history(&versions(0..2), &version(2).1, 0o640);
    let all = profile.generations();
    let mut state = State::open_protected(Some(profile.config.clone()), Some(profile.backups()), theme());
    let spec = tb_app::spec();
    let (_, scene) = spec
        .scenes
        .iter()
        .find(|(name, _)| *name == "recovery")
        .expect("the configurator has a `recovery` scene");
    let before = tree(&profile.dir);

    scene(&mut state);

    let mut rt = Runtime::new(state, tb_app::view, WINDOW, Env::new(true, 1.0));
    rt.on_key = Some(tb_app::on_key);
    rt.frame(0.0);
    settle(&mut rt);
    for k in ["backups-location", "key-row-return"] {
        read_at(&mut rt, k, WINDOW, "the scene");
    }
    for k in [key(&all[0]), key(&all[1])] {
        reveal_control(&mut rt, &k, WINDOW);
        assert!(fully_visible(&rt, &k, WINDOW), "the scene shows {k}");
    }
    assert_eq!(tree(&profile.dir), before, "the scene writes nothing");
}

// The reading checks themselves, on a sidebar-like panel (208x283 scroll viewport in a 640x400
// window, as the configurator's at 640x400): a passage or control a person cannot see whole fails.

/// What the panel holds below a spacer, and above a tail that leaves room to scroll.
enum Panel {
    Passage,
    PassageInClip,
    ClampedPassage,
    InvisiblePassage,
    SnugControl,
    ControlInClip,
    TallControl,
}

const PANEL: (f32, f32) = (640.0, 400.0);

fn passage() -> El {
    let words = "a passage that wraps onto many lines of the sidebar, read from its first line to its last; ";
    text(words.repeat(8)).key("passage").w_full().wrap_anywhere()
}

fn panel_view(panel: &Panel, _cx: &mut Cx<Panel>) -> El {
    let held = match panel {
        Panel::Passage => passage(),
        Panel::PassageInClip => col().w_full().h(120.0).clip_soft(0.0).child(passage()),
        Panel::ClampedPassage => passage().max_lines(3),
        Panel::InvisiblePassage => passage().opacity(0.0),
        Panel::SnugControl => el().key("control").size(168.0, 262.5),
        Panel::ControlInClip => {
            col().w_full().h(10.0).clip_soft(0.0).child(el().key("control").size(168.0, 40.0))
        }
        Panel::TallControl => el().key("control").size(168.0, 300.0),
    };
    let content = col()
        .w(208.0)
        .px(20.0)
        .pt(18.0)
        .pb(20.0)
        .child(el().size(168.0, 600.0))
        .child(held)
        .child(el().size(168.0, 400.0));
    col().size(PANEL.0, PANEL.1).child(
        col()
            .key("panel-scroll")
            .size(208.0, 283.0)
            .scroll(ScrollSpec { stick_bottom: false })
            .child(content),
    )
}

fn panel(kind: Panel) -> Runtime<Panel> {
    let mut rt = Runtime::new(kind, panel_view, PANEL, Env::new(true, 1.0));
    rt.frame(0.0);
    support::reading::rest(&mut rt);
    rt
}

#[test]
fn a_passage_taller_than_its_viewport_is_read_in_views_and_a_snug_control_is_reached_whole() {
    let mut rt = panel(Panel::Passage);
    let reading = read_passage(&mut rt, "passage", PANEL);
    assert!(reading.height > reading.viewport && reading.views >= 2, "{reading:?}");

    let mut rt = panel(Panel::SnugControl);
    reveal_control(&mut rt, "control", PANEL);
    assert!(support::reading::whole(&support::reading::live(&rt, "control"), PANEL));
}

/// The message `check` panics with; it must panic.
fn refusal(check: impl FnOnce()) -> String {
    let payload =
        std::panic::catch_unwind(std::panic::AssertUnwindSafe(check)).expect_err("the check must fail");
    payload
        .downcast_ref::<String>()
        .cloned()
        .or_else(|| payload.downcast_ref::<&str>().map(|s| (*s).to_owned()))
        .unwrap_or_default()
}

fn refused_passage(kind: Panel, why: &str) {
    let said = refusal(|| {
        read_passage(&mut panel(kind), "passage", PANEL);
    });
    assert!(said.starts_with("passage cannot be read whole") && said.contains(why), "{said}");
}

fn refused_control(kind: Panel, why: &str) {
    let said = refusal(|| reveal_control(&mut panel(kind), "control", PANEL));
    assert!(said.starts_with("control cannot be reached whole") && said.contains(why), "{said}");
}

#[test]
fn a_passage_partly_hidden_by_a_clip_is_not_read_whole() {
    refused_passage(Panel::PassageInClip, "are never shown");
}

#[test]
fn a_clamped_passage_is_not_read_whole() {
    refused_passage(Panel::ClampedPassage, "truncated or clamped");
}

#[test]
fn an_invisible_passage_is_not_read_whole() {
    refused_passage(Panel::InvisiblePassage, "is not opaque");
}

#[test]
fn a_control_hidden_by_a_clip_is_not_reached() {
    refused_control(Panel::ControlInClip, "yet clipped");
}

#[test]
fn a_control_taller_than_its_viewport_is_not_reached() {
    refused_control(Panel::TallControl, "larger than its viewport");
}
