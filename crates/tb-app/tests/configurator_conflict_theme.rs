//! Conflict resolutions resolve the newer file's theme source as the daemon does: an Omarchy
//! source against the actual active Omarchy theme (never the theme an external file selected), a
//! selected file as it is on disk at that moment. Covers a newer file that switches from an
//! external theme file to Omarchy, adopted clean (Save) or dirty (`conflict-reload`), valid and
//! invalid under the active theme, and an acknowledged overwrite after the selected file changed.
//! Every case builds its own `HOME` under cargo's temp dir and opens the editor as startup does;
//! observed through the real Runtime and the files, and compared with the daemon's
//! `ConfigFile::load_resolved` on the same home.
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
use tb_config::{ActiveTheme, ConfigFile, ConfigFileError, default_path};
use tb_core::config::{Config, ThemeSource, to_toml};
use tb_core::{BarTheme, Chord, Rgba};
use tb_render::view::color;

/// A colour token only the external theme defines.
const TOKEN: &str = "external_only";
const TOKEN_COLOR: Rgba = Rgba::opaque(0x12, 0x34, 0x56);
const CHANGED_COLOR: Rgba = Rgba::opaque(0x65, 0x43, 0x21);

/// The active Omarchy theme; it has no `external_only`.
const TOKYO_NIGHT: &str = include_str!("../../../fixtures/omarchy-themes/tokyo-night/colors.toml");
/// The external theme's base: visibly different from the active one (light background).
const CATPPUCCIN_LATTE: &str = include_str!("../../../fixtures/omarchy-themes/catppuccin-latte/colors.toml");

fn external_colors(token: &str) -> String {
    format!("{CATPPUCCIN_LATTE}\n{TOKEN} = \"{token}\"\n")
}

/// A home on Tokyo Night whose config selects an external Latte theme and fills its first
/// default-layer button with the external-only token. Returns home, config and that button's id.
fn external_home(test: &str) -> (PathBuf, Config, String) {
    let home = fresh_dir("configurator_conflict_theme", test);
    let active = home.join(".local/state/omarchy/current/theme");
    fs::create_dir_all(&active).unwrap();
    fs::write(active.join("colors.toml"), TOKYO_NIGHT).unwrap();
    let theme = external_path(&home);
    fs::create_dir_all(theme.parent().unwrap()).unwrap();
    fs::write(&theme, external_colors("#123456")).unwrap();
    let mut config =
        Config { theme: ThemeSource::File { path: theme.display().to_string() }, ..Config::default() };
    let button = &mut config.layout.layer_mut(Chord::Default).unwrap().groups[0].buttons[0];
    button.style.fill = Some(TOKEN.to_owned());
    let id = button.id.to_string();
    let path = default_path(&home);
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(&path, to_toml(&config).unwrap()).unwrap();
    (home, config, id)
}

fn external_path(home: &Path) -> PathBuf {
    home.join("themes/external/colors.toml")
}

fn active_theme(home: &Path) -> BarTheme {
    ActiveTheme::under_home(home).load().theme
}

/// The editor as startup opens it for `home`.
fn startup(home: &Path) -> Runtime<State> {
    runtime(State::open(Some(default_path(home)), active_theme(home)), tb_app::WINDOW)
}

/// What the daemon accepts on the same home: its config and resolved theme, or its refusal.
fn daemon(home: &Path) -> Result<(Config, BarTheme), ConfigFileError> {
    let mut file = ConfigFile::new(default_path(home));
    let theme = file.load_resolved(&active_theme(home))?;
    Ok((file.config().clone(), theme))
}

/// Another editor atomically replaces the config with `config`; returns the bytes it wrote.
fn external_save(home: &Path, config: &Config) -> Vec<u8> {
    let path = default_path(home);
    let text = format!("# written elsewhere\n{}", to_toml(config).unwrap());
    let staged = path.with_file_name(".other-editor.tmp");
    fs::write(&staged, &text).unwrap();
    fs::rename(&staged, &path).unwrap();
    text.into_bytes()
}

/// `base` switched to follow Omarchy, its first button relabelled; with `keep_token` the button
/// still uses the external-only token (invalid under the active theme).
fn following_omarchy(base: &Config, keep_token: bool) -> Config {
    let mut config = Config { theme: ThemeSource::Omarchy, ..base.clone() };
    let button = &mut config.layout.layer_mut(Chord::Default).unwrap().groups[0].buttons[0];
    "from elsewhere".clone_into(&mut button.label);
    if !keep_token {
        button.style.fill = None;
    }
    config
}

/// Makes the editor dirty with an unsaved label on button `id`.
fn draft(rt: &mut Runtime<State>, id: &str) {
    operate(rt, &format!("row-{id}"));
    edit(rt, "field-label", "my draft");
}

fn status(rt: &Runtime<State>) -> String {
    words(rt, "status").to_lowercase()
}

/// The editor shows exactly what the daemon accepts and draws it in the daemon's theme.
fn assert_agrees_with_daemon(rt: &Runtime<State>, home: &Path) {
    let (config, theme) = daemon(home).expect("the daemon accepts the file");
    assert_eq!(rt.state.config, config, "the editor shows the file the daemon reads");
    assert_eq!(rt.state.theme, theme, "the editor resolves the theme the daemon resolves");
    assert_eq!(element(rt, "preview").fill, color(theme.background), "the preview is drawn in it");
}

fn assert_refused_and_kept(rt: &Runtime<State>, home: &Path, newer: &[u8], shown: &Config) {
    assert_eq!(fs::read(default_path(home)).unwrap(), newer, "the newer file is kept byte for byte");
    assert!(
        matches!(daemon(home), Err(ConfigFileError::Invalid(_))),
        "the daemon refuses the newer file under the active theme too"
    );
    assert_eq!(&rt.state.config, shown, "a file the active theme refuses is not adopted");
    assert_eq!(element(rt, "preview").fill, color(rt.state.theme.background));
    let status = status(rt);
    assert!(!status.starts_with("saved") && !status.starts_with("reloaded"), "{status}");
}

#[test]
fn a_clean_editor_never_adopts_an_omarchy_file_the_active_theme_refuses() {
    let (home, config, _) = external_home("clean_to_omarchy_invalid");
    let mut rt = startup(&home);
    let newer = external_save(&home, &following_omarchy(&config, true));

    operate(&mut rt, "save");

    assert_refused_and_kept(&rt, &home, &newer, &config);
    assert!(status(&rt).contains("nothing saved"), "{}", status(&rt));
}

#[test]
fn a_clean_editor_adopts_a_valid_omarchy_file_in_the_active_theme_and_saves_against_it() {
    let (home, config, id) = external_home("clean_to_omarchy_valid");
    let mut rt = startup(&home);
    let newer = following_omarchy(&config, false);
    external_save(&home, &newer);

    operate(&mut rt, "save");

    assert_eq!(rt.state.config, newer, "the newer file is shown");
    assert_clean(&rt);
    assert_agrees_with_daemon(&rt, &home);

    // A later ordinary save resolves Omarchy against the active theme, not the old external one.
    draft(&mut rt, &id);
    operate(&mut rt, "save");
    assert!(status(&rt).starts_with("saved"), "{}", status(&rt));
    assert_agrees_with_daemon(&rt, &home);
}

#[test]
fn reloading_never_accepts_an_omarchy_file_the_active_theme_refuses_and_keeps_the_draft() {
    let (home, config, id) = external_home("dirty_to_omarchy_invalid");
    let mut rt = startup(&home);
    draft(&mut rt, &id);
    let drafted = rt.state.config.clone();
    let newer = external_save(&home, &following_omarchy(&config, true));
    operate(&mut rt, "save");
    assert_eq!(fs::read(default_path(&home)).unwrap(), newer, "the dirty stale save wrote nothing");

    operate(&mut rt, "conflict-reload");

    assert_refused_and_kept(&rt, &home, &newer, &drafted);
    assert_eq!(words(&rt, "field-label"), "my draft", "the draft is kept");
    assert_dirty(&rt, 1);
    assert!(shown(&rt, "conflict-reload") && shown(&rt, "conflict-overwrite"), "still resolvable");
}

#[test]
fn reloading_a_valid_omarchy_file_shows_it_in_the_active_theme() {
    let (home, config, id) = external_home("dirty_to_omarchy_valid");
    let mut rt = startup(&home);
    draft(&mut rt, &id);
    let newer = following_omarchy(&config, false);
    external_save(&home, &newer);
    operate(&mut rt, "save");

    operate(&mut rt, "conflict-reload");

    assert!(status(&rt).starts_with("reloaded"), "{}", status(&rt));
    assert_eq!(rt.state.config, newer);
    assert_clean(&rt);
    assert_agrees_with_daemon(&rt, &home);
}

#[test]
fn an_overwrite_installs_the_selected_theme_it_validated_against() {
    let (home, config, id) = external_home("overwrite_changed_token");
    let mut rt = startup(&home);
    draft(&mut rt, &id);
    let mut theirs = config.clone();
    theirs.layout.layer_mut(Chord::Default).unwrap().groups[1].buttons[0].label = "theirs".to_owned();
    let replaced = external_save(&home, &theirs);
    operate(&mut rt, "save");
    assert!(shown(&rt, "conflict-overwrite"), "the conflict is offered: {}", status(&rt));
    fs::write(external_path(&home), external_colors("#654321")).unwrap();

    operate(&mut rt, "conflict-overwrite");

    assert!(status(&rt).starts_with("saved"), "{}", status(&rt));
    assert_clean(&rt);
    assert!(!shown(&rt, "conflict-overwrite"), "the conflict is resolved");
    assert_eq!(rt.state.theme.resolve(TOKEN), Some(CHANGED_COLOR), "the changed colour is installed");
    assert_eq!(element(&rt, &format!("preview-{id}")).fill, color(CHANGED_COLOR), "and previewed");
    assert_agrees_with_daemon(&rt, &home);
    let backup = default_path(&home).with_file_name("config.toml.bak");
    assert_eq!(fs::read(backup).unwrap(), replaced, "the backup holds the replaced version");
    assert_ne!(TOKEN_COLOR, CHANGED_COLOR);
}
