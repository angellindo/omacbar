//! The configurator opens, previews, edits and saves against the theme the config's theme source
//! resolves to, as the daemon does. Every case builds its own `HOME` under cargo's temp dir (never
//! the real one) and opens the editor exactly as startup does: the active Omarchy theme of that
//! home (`ActiveTheme::load`) and the config at `default_path(home)`. Observed through the public
//! Runtime keys (`status`, `save-blocked-reason`, `save`, `preview-<id>`, `field-*`) and the
//! bytes on disk.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod support {
    pub mod configurator;
    pub mod ui;
}

use std::fs;
use std::path::{Path, PathBuf};

use rune::Runtime;
use support::configurator::*;
use tb_app::State;
use tb_config::{ActiveTheme, ConfigFile, default_path};
use tb_core::config::{Config, ThemeSource, parse, to_toml};
use tb_core::{BarTheme, Chord, Rgba};
use tb_render::view::color;

/// A colour token only the external theme defines, and its colour.
const TOKEN: &str = "external_only";
const TOKEN_COLOR: Rgba = Rgba::opaque(0x12, 0x34, 0x56);
/// A comment a rewrite of the file would drop: proves the original bytes were left alone.
const OWN_COMMENT: &str = "# my own layout, written by hand\n";

const TOKYO_NIGHT: &str = include_str!("../../../fixtures/omarchy-themes/tokyo-night/colors.toml");
const CATPPUCCIN_LATTE: &str = include_str!("../../../fixtures/omarchy-themes/catppuccin-latte/colors.toml");
/// An Omarchy theme with only the required tokens: no `red`.
const MINIMAL: &str = "background = \"#101418\"\nforeground = \"#d0d4d8\"\naccent = \"#4c8cf0\"\n";

/// An empty home for one test whose active Omarchy theme is `colors`.
fn home_with_active(test: &str, colors: &str) -> PathBuf {
    let home = fresh_dir("configurator_selected_theme", test);
    let dir = home.join(".local/state/omarchy/current/theme");
    fs::create_dir_all(&dir).unwrap();
    fs::write(dir.join("colors.toml"), colors).unwrap();
    home
}

/// The editor as startup opens it for `home`.
fn startup(home: &Path) -> Runtime<State> {
    let active = ActiveTheme::under_home(home).load().theme;
    runtime(State::open(Some(default_path(home)), active), tb_app::WINDOW)
}

/// A theme file outside the Omarchy state directory: Tokyo Night plus the unique token.
fn external_colors() -> String {
    format!("{TOKYO_NIGHT}\n{TOKEN} = \"#123456\"\n")
}

fn external_path(home: &Path) -> PathBuf {
    home.join("themes/external/colors.toml")
}

/// The default config selecting `theme` as its theme source, its first default-layer button
/// filled with the unique token. Returns the config and that button's id.
fn layout_on(theme: &Path) -> (Config, String) {
    let mut config = Config {
        theme: ThemeSource::File { path: theme.to_string_lossy().into_owned() },
        ..Config::default()
    };
    let button = &mut config.layout.layer_mut(Chord::Default).unwrap().groups[0].buttons[0];
    button.style.fill = Some(TOKEN.to_owned());
    let id = button.id.to_string();
    (config, id)
}

/// Writes the user's own config text (with a hand comment) and returns its exact bytes.
fn write_own(home: &Path, config: &Config) -> Vec<u8> {
    let path = default_path(home);
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    let text = format!("{OWN_COMMENT}{}", to_toml(config).unwrap());
    fs::write(&path, &text).unwrap();
    text.into_bytes()
}

/// A home on Tokyo Night whose config selects an external theme with the unique token.
fn external_home(test: &str) -> (PathBuf, Config, String, Vec<u8>) {
    let home = home_with_active(test, TOKYO_NIGHT);
    let theme = external_path(&home);
    fs::create_dir_all(theme.parent().unwrap()).unwrap();
    fs::write(&theme, external_colors()).unwrap();
    let (config, id) = layout_on(&theme);
    let bytes = write_own(&home, &config);
    (home, config, id, bytes)
}

fn on_disk(home: &Path) -> Config {
    parse(&fs::read_to_string(default_path(home)).unwrap()).unwrap()
}

fn assert_editing_without_issues(rt: &Runtime<State>) {
    let status = words(rt, "status");
    assert!(status.starts_with("editing"), "the config is opened for editing: {status:?}");
    assert!(
        !shown(rt, "save-blocked-reason"),
        "save is not blocked: {:?}",
        text_of(rt, "save-blocked-reason")
    );
    assert!(shown(rt, "no-issues"), "no issues are listed: {}", all_words(rt));
    assert!(!disabled(rt, "save"), "the Save control is enabled");
}

fn assert_bytes_kept(home: &Path, bytes: &[u8], when: &str) {
    assert_eq!(
        String::from_utf8_lossy(&fs::read(default_path(home)).unwrap()),
        String::from_utf8_lossy(bytes),
        "the user's layout bytes are untouched {when}"
    );
}

fn assert_names(rt: &Runtime<State>, key: &str, what: &[&str]) {
    let text = words(rt, key);
    for part in what {
        assert!(text.contains(part), "{key} names {part:?}: {text:?}");
    }
}

#[test]
fn an_external_theme_token_opens_for_editing_and_previews_its_resolved_colour() {
    let (home, config, id, _) = external_home("external_open");
    let rt = startup(&home);
    assert_editing_without_issues(&rt);
    assert_eq!(rt.state.config, config, "the user's own layout is shown, not the default");
    assert_eq!(
        element(&rt, &format!("preview-{id}")).fill,
        color(TOKEN_COLOR),
        "the preview draws the token as the selected theme resolves it"
    );
    let mut rt = rt;
    operate(&mut rt, &format!("row-{id}"));
    assert_eq!(words(&rt, "field-fill"), TOKEN, "the edit panel shows the saved token");
}

#[test]
fn an_external_theme_layout_survives_edit_save_and_reopen_with_the_same_token_and_colour() {
    let (home, config, id, _) = external_home("external_round_trip");
    let mut rt = startup(&home);
    operate(&mut rt, &format!("row-{id}"));
    edit(&mut rt, "field-label", "relabelled");
    operate(&mut rt, "save");
    let status = words(&rt, "status");
    assert!(status.starts_with("saved"), "the explicit save succeeds: {status:?}");

    let mut expected = config;
    expected.layout.layer_mut(Chord::Default).unwrap().groups[0].buttons[0].label = "relabelled".to_owned();
    assert_eq!(on_disk(&home), expected, "the saved file keeps the layout, token and theme source");

    let reopened = startup(&home);
    assert_editing_without_issues(&reopened);
    assert_eq!(reopened.state.config, expected, "a reopen shows what was saved");
    assert_clean(&reopened);
    let shown_button = button(&reopened, Chord::Default, &id);
    assert_eq!(shown_button.label, "relabelled");
    assert_eq!(shown_button.style.fill.as_deref(), Some(TOKEN));
    assert_eq!(element(&reopened, &format!("preview-{id}")).fill, color(TOKEN_COLOR));

    // The daemon's path on the same home accepts the saved file and resolves the same colour.
    let active = ActiveTheme::under_home(&home).load().theme;
    let mut daemon_view = ConfigFile::new(default_path(&home));
    let resolved = daemon_view.load_resolved(&active).unwrap();
    assert_eq!(daemon_view.config(), &expected);
    assert_eq!(resolved.resolve(TOKEN), Some(TOKEN_COLOR));
}

/// Opens a home whose selected theme fails, makes an edit, then tries Save and Ctrl+S.
fn assert_open_failure_keeps_bytes(
    rt: &mut Runtime<State>,
    home: &Path,
    bytes: &[u8],
    id: &str,
    cause: &[&str],
) {
    assert_names(rt, "status", cause);
    assert_blocked(rt, cause);
    operate(rt, &format!("row-{id}"));
    edit(rt, "field-label", "local edit");
    tap(rt, "save");
    assert_bytes_kept(home, bytes, "after pressing Save");
    save_shortcut(rt);
    assert_bytes_kept(home, bytes, "after Ctrl+S");
    assert_names(rt, "status", cause);
    assert!(!words(rt, "status").starts_with("saved"), "nothing reports a save");
}

#[test]
fn a_missing_selected_theme_names_the_file_and_never_replaces_the_layout() {
    let (home, _, id, bytes) = external_home("missing_selected");
    let theme = external_path(&home);
    fs::remove_file(&theme).unwrap();
    let mut rt = startup(&home);
    assert_open_failure_keeps_bytes(&mut rt, &home, &bytes, &id, &[theme.to_str().unwrap()]);
    let status = words(&rt, "status").to_lowercase();
    assert!(
        status.contains("no such file") || status.contains("not found") || status.contains("missing"),
        "the status says the theme file is missing: {status:?}"
    );
    assert!(!theme.exists(), "no theme file is invented");
}

#[test]
fn a_malformed_selected_theme_names_the_file_and_the_missing_token_and_never_replaces_the_layout() {
    let (home, _, id, bytes) = external_home("malformed_selected");
    let theme = external_path(&home);
    let without_accent: String =
        external_colors().lines().filter(|l| !l.starts_with("accent")).flat_map(|l| [l, "\n"]).collect();
    fs::write(&theme, &without_accent).unwrap();
    let mut rt = startup(&home);
    assert_open_failure_keeps_bytes(&mut rt, &home, &bytes, &id, &[theme.to_str().unwrap(), "accent"]);
    assert_eq!(fs::read_to_string(&theme).unwrap(), without_accent, "the theme file is untouched");
}

/// Opens a valid external-theme home, edits a label, breaks the theme with `break_it`, tries
/// Ctrl+S, then repairs the theme and retries with the Save control.
fn save_time_failure_then_retry(test: &str, break_it: impl Fn(&Path), cause: impl Fn(&Path) -> Vec<String>) {
    let (home, config, id, bytes) = external_home(test);
    let theme = external_path(&home);
    let mut rt = startup(&home);
    assert_editing_without_issues(&rt);
    operate(&mut rt, &format!("row-{id}"));
    edit(&mut rt, "field-label", "kept edit");

    break_it(&theme);
    save_shortcut(&mut rt);
    assert_bytes_kept(&home, &bytes, "when the selected theme broke after opening");
    let status = words(&rt, "status");
    assert!(!status.starts_with("saved"), "the save is refused: {status:?}");
    for part in cause(&theme) {
        assert!(status.contains(&part), "the status names {part:?}: {status:?}");
    }
    assert_eq!(
        button(&rt, Chord::Default, &id).label,
        "kept edit",
        "the local edit is kept after the refused save"
    );
    assert_eq!(words(&rt, "field-label"), "kept edit");

    fs::write(&theme, external_colors()).unwrap();
    operate(&mut rt, "save");
    let status = words(&rt, "status");
    assert!(status.starts_with("saved"), "a retry after the repair saves: {status:?}");
    let mut expected = config;
    expected.layout.layer_mut(Chord::Default).unwrap().groups[0].buttons[0].label = "kept edit".to_owned();
    assert_eq!(on_disk(&home), expected);
}

#[test]
fn a_selected_theme_removed_after_opening_refuses_the_save_keeps_edits_and_saves_after_repair() {
    save_time_failure_then_retry(
        "removed_after_open",
        |theme| fs::remove_file(theme).unwrap(),
        |theme| vec![theme.to_string_lossy().into_owned()],
    );
}

#[test]
fn a_selected_theme_malformed_after_opening_refuses_the_save_keeps_edits_and_saves_after_repair() {
    save_time_failure_then_retry(
        "malformed_after_open",
        |theme| {
            fs::write(theme, "background = \"#000000\"\nforeground = \"#ffffff\"\naccent = \"#zz\"\n")
                .unwrap()
        },
        |theme| vec![theme.to_string_lossy().into_owned(), "accent".to_owned()],
    );
}

#[test]
fn a_selected_theme_that_loses_a_used_token_after_opening_refuses_the_save_and_names_the_token() {
    save_time_failure_then_retry(
        "token_lost_after_open",
        |theme| fs::write(theme, TOKYO_NIGHT).unwrap(),
        |_| vec![TOKEN.to_owned()],
    );
}

#[test]
fn a_first_run_under_a_theme_without_red_writes_a_default_every_process_accepts() {
    let home = home_with_active("minimal_first_run", MINIMAL);
    let rt = startup(&home);
    assert_editing_without_issues(&rt);
    let minimal = BarTheme::from_omarchy_colors(MINIMAL).unwrap();
    let mut expected = Config::default();
    let lock = expected
        .layout
        .layer_mut(Chord::FnCtrl)
        .unwrap()
        .groups
        .iter_mut()
        .flat_map(|g| &mut g.buttons)
        .find(|b| b.id.as_str() == "lock")
        .unwrap();
    assert_eq!(lock.style.fill.as_deref(), Some("red"), "the shipped default fills the lock red");
    lock.style.fill = None;
    assert_eq!(on_disk(&home), expected, "only the override the theme cannot resolve is dropped");
    assert_eq!(rt.state.config, expected);

    let reopened = startup(&home);
    assert_editing_without_issues(&reopened);
    assert_clean(&reopened);
    let mut daemon_view = ConfigFile::new(default_path(&home));
    daemon_view.load_resolved(&minimal).unwrap();
    assert_eq!(daemon_view.config(), &expected);
}

#[test]
fn a_first_run_under_shipped_omarchy_themes_writes_the_unchanged_default() {
    for (n, colors) in [TOKYO_NIGHT, CATPPUCCIN_LATTE].into_iter().enumerate() {
        let home = home_with_active(&format!("shipped_first_run_{n}"), colors);
        let rt = startup(&home);
        assert_editing_without_issues(&rt);
        assert_eq!(fs::read_to_string(default_path(&home)).unwrap(), to_toml(&Config::default()).unwrap());
        assert_eq!(button(&rt, Chord::FnCtrl, "lock").style.fill.as_deref(), Some("red"));
    }
}

#[test]
fn an_invalid_config_following_the_active_theme_still_offers_the_default_and_saving_replaces_it() {
    let home = home_with_active("legacy_invalid", TOKYO_NIGHT);
    let mut config = Config::default();
    config.layout.layer_mut(Chord::Default).unwrap().groups[0].buttons[0].label.clear();
    let bytes = write_own(&home, &config);
    let mut rt = startup(&home);
    let status = words(&rt, "status");
    assert!(
        status.contains("issue(s): editing the default; saving replaces it"),
        "the existing fallback text: {status:?}"
    );
    assert_bytes_kept(&home, &bytes, "before an explicit save");
    save_shortcut(&mut rt);
    assert!(words(&rt, "status").starts_with("saved"));
    assert_eq!(fs::read_to_string(default_path(&home)).unwrap(), to_toml(&Config::default()).unwrap());
}
