//! After `touchbar-recover restore <id>` brings back a kept file byte for byte, it says whether the
//! restored file validates with its theme source, or why the daemon would not accept it: the
//! parser's own diagnostic for a file that is not valid TOML, the theme file error for a selected
//! theme file that is not there. The restore itself still succeeds (exit 0, exact bytes and mode,
//! a `recover` generation of what it replaced) whatever the restored file holds: the check only
//! reads. It never writes: no default, no theme file, no extra generation; restoring an absence
//! with `--remove-file` leaves no file behind. Nothing claims a daemon is running.
//!
//! Every run is the real binary in one test's throwaway profile (`HOME=<dir>/home`,
//! `XDG_STATE_HOME=<dir>/state`, no display, no GPU); the expected diagnostics are what the
//! library itself reports for the restored file, read without writing (`reload_resolved` against
//! the active theme of that home).
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod support {
    pub mod generations;
    pub mod recovery;
}

use std::fs;

use support::generations::*;
use support::recovery::*;
use tb_config::{ActiveTheme, ConfigFile, ConfigFileError, GenerationKind, Loaded};
use tb_core::config::{Config, ThemeSource};

const SUITE: &str = "recover_terminal_validation";
const TOKYO_NIGHT: &str = include_str!("../../../fixtures/omarchy-themes/tokyo-night/colors.toml");

/// Keeps `bytes` (mode `mode`) as a generation by seeding them and saving another config over
/// them through the protected library, as an editor would. Returns the kept generation's id; the
/// config then holds a valid `current` layout.
fn kept_generation(profile: &Profile, bytes: &[u8], mode: u32) -> String {
    profile.seed(bytes, mode);
    let mut file = ConfigFile::protected(&profile.config, profile.backups());
    file.load(&theme()).unwrap();
    file.save(relabelled("current"), &theme()).unwrap();
    assert_ne!(fs::read(&profile.config).unwrap(), bytes);
    generation_holding(profile, bytes).id.to_string()
}

/// Restores generation `id`; the restore itself must succeed with the exact bytes and mode, and
/// keep exactly one new generation (the `recover` one, holding what it replaced). Returns the run.
fn restore_exactly(profile: &Profile, id: &str, bytes: &[u8], mode_bits: u32) -> Run {
    let replaced = fs::read(&profile.config).unwrap();
    let earlier = profile.generations().len();
    let run = recover(profile, &["restore", id]);
    run.exits(0);
    assert_eq!(fs::read(&profile.config).unwrap(), bytes, "restored byte for byte; {run:?}");
    assert_eq!(mode(&profile.config), mode_bits, "restored with its permission bits; {run:?}");
    assert_eq!(profile.generations().len(), earlier + 1, "only the `recover` generation was added; {run:?}");
    assert_eq!(kept(&profile.store, &only_recover_generation(&profile.store)), replaced);
    assert_eq!(
        names_in(profile.config.parent().unwrap()),
        ["config.toml"],
        "nothing else was written; {run:?}"
    );
    run
}

/// What the library reports for the file now on disk, read without writing.
fn library_verdict(profile: &Profile) -> Result<(), ConfigFileError> {
    let active = ActiveTheme::under_home(&profile.dir.join("home")).load().theme;
    ConfigFile::new(&profile.config).reload_resolved(&active).map(|_| ())
}

/// stdout and stderr of a run, one after the other.
fn said(run: &Run) -> String {
    format!("{}\n{}", run.stdout, run.stderr)
}

/// A line holding the whole word `valid` (not `invalid`) and `naming`.
fn says_valid_naming(run: &Run, naming: &str) -> bool {
    said(run).lines().any(|line| {
        line.to_lowercase().contains(&naming.to_lowercase())
            && words(line).any(|w| w.eq_ignore_ascii_case("valid"))
    })
}

fn names_the_daemon(run: &Run) -> bool {
    let text = said(run).to_lowercase();
    text.contains("touchbard") || text.contains("daemon")
}

fn claims_a_running_daemon(run: &Run) -> bool {
    let text = said(run).to_lowercase();
    ["is running", "has reloaded", "reloaded the", "now draws", "is drawing"].iter().any(|w| text.contains(w))
}

#[test]
fn a_restored_valid_config_is_reported_valid_with_its_omarchy_theme_source() {
    let profile = Profile::new(SUITE, "valid_omarchy");
    let bytes = hand_written(&relabelled("mine"));
    let id = kept_generation(&profile, &bytes, 0o640);

    let run = restore_exactly(&profile, &id, &bytes, 0o640);

    library_verdict(&profile).unwrap();
    assert!(
        says_valid_naming(&run, "omarchy"),
        "says the restored file is valid with the Omarchy theme; {run:?}"
    );
    assert!(!claims_a_running_daemon(&run), "{run:?}");
}

#[test]
fn a_restored_valid_config_names_the_theme_file_it_validates_with() {
    let profile = Profile::new(SUITE, "valid_theme_file");
    let colors = profile.dir.join("themes/external/colors.toml");
    fs::create_dir_all(colors.parent().unwrap()).unwrap();
    fs::write(&colors, TOKYO_NIGHT).unwrap();
    let theme_path = colors.display().to_string();
    let own = Config { theme: ThemeSource::File { path: theme_path.clone() }, ..relabelled("mine") };
    let bytes = hand_written(&own);
    let id = kept_generation(&profile, &bytes, 0o600);

    let run = restore_exactly(&profile, &id, &bytes, 0o600);

    library_verdict(&profile).unwrap();
    assert!(
        says_valid_naming(&run, &theme_path),
        "says the file is valid with the theme file {theme_path}; {run:?}"
    );
    assert_eq!(fs::read_to_string(&colors).unwrap(), TOKYO_NIGHT, "the theme file is only read");
    assert!(!claims_a_running_daemon(&run), "{run:?}");
}

#[test]
fn a_restored_file_that_is_not_toml_is_restored_and_the_parser_diagnostic_is_shown() {
    let profile = Profile::new(SUITE, "invalid_toml");
    let bytes = b"# restored by hand\n[[layers\nthis is = = not toml\n".to_vec();
    let id = kept_generation(&profile, &bytes, 0o640);

    let run = restore_exactly(&profile, &id, &bytes, 0o640);

    let Err(ConfigFileError::Invalid(issues)) = library_verdict(&profile) else {
        panic!("the library rejects the restored file as invalid")
    };
    let diagnostic = issues[0].to_string();
    let first = diagnostic.lines().next().unwrap();
    assert!(said(&run).contains(first), "shows the parser's diagnostic `{first}`; {run:?}");
    assert!(names_the_daemon(&run), "says what the daemon does with it; {run:?}");
    assert!(!claims_a_running_daemon(&run), "{run:?}");
}

#[test]
fn a_restored_config_whose_theme_file_is_missing_is_restored_and_says_why_the_daemon_refuses_it() {
    let profile = Profile::new(SUITE, "missing_theme_file");
    let missing = profile.dir.join("themes/gone/colors.toml");
    let own =
        Config { theme: ThemeSource::File { path: missing.display().to_string() }, ..relabelled("mine") };
    let bytes = hand_written(&own);
    let id = kept_generation(&profile, &bytes, 0o640);

    let run = restore_exactly(&profile, &id, &bytes, 0o640);

    let Err(error @ ConfigFileError::Theme(_)) = library_verdict(&profile) else {
        panic!("the library refuses the restored file for its theme file")
    };
    let why = error.to_string();
    assert!(said(&run).contains(&why), "shows why: `{why}`; {run:?}");
    assert!(said(&run).contains(&missing.display().to_string()), "names the missing theme file; {run:?}");
    assert!(names_the_daemon(&run), "says what the daemon does with it; {run:?}");
    assert!(!missing.exists() && !missing.parent().unwrap().exists(), "the theme file is never created");
    assert!(!claims_a_running_daemon(&run), "{run:?}");
}

#[test]
fn restoring_an_absence_with_remove_file_never_recreates_the_file() {
    let profile = Profile::new(SUITE, "absence");
    let dir = profile.config.parent().unwrap().to_path_buf();
    fs::create_dir_all(&dir).unwrap();
    let mut file = ConfigFile::protected(&profile.config, profile.backups());
    assert_eq!(file.load(&theme()).unwrap(), Loaded::WroteDefault);
    let absence =
        profile.generations().into_iter().find(|g| matches!(g.kind, GenerationKind::Absent { .. })).unwrap();
    let default = fs::read(&profile.config).unwrap();
    let earlier = profile.generations().len();

    let run = recover(&profile, &["restore", &absence.id.to_string(), "--remove-file"]);

    run.exits(0);
    assert!(fs::symlink_metadata(&profile.config).is_err(), "the config file stays removed; {run:?}");
    assert!(dir.is_dir(), "its directory is kept");
    assert!(names_in(&dir).is_empty(), "nothing was created in its place; {run:?}");
    assert_eq!(profile.generations().len(), earlier + 1, "only the `recover` generation was added; {run:?}");
    assert_eq!(kept(&profile.store, &only_recover_generation(&profile.store)), default);
    assert!(!claims_a_running_daemon(&run), "{run:?}");
    let listed = recover(&profile, &["list"]);
    listed.exits(0);
    assert!(fs::symlink_metadata(&profile.config).is_err(), "a later `list` creates nothing; {listed:?}");
    assert_eq!(profile.generations().len(), earlier + 1);
}
