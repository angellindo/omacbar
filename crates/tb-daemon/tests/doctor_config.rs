//! Read-only doctor config checks cross the real native executable, independently of a GPU.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use tb_core::Action;
use tb_core::config::{Config, ThemeSource, to_toml};

const SECRET: &str = "private-command-token-do-not-collect";

fn home(name: &str) -> PathBuf {
    let path = Path::new(env!("CARGO_TARGET_TMPDIR"))
        .join("doctor_config")
        .join(format!("{name}-{}", std::process::id()));
    if path.exists() {
        fs::remove_dir_all(&path).unwrap();
    }
    fs::create_dir_all(&path).unwrap();
    path
}

fn config_path(home: &Path) -> PathBuf {
    home.join(".config/touchbar-customizer/config.toml")
}

fn save(home: &Path, text: &str) {
    let path = config_path(home);
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, text).unwrap();
}

fn probe(home: &Path) -> String {
    let binary = option_env!("CARGO_BIN_EXE_touchbar-doctor-probe")
        .expect("the doctor must provide its native config probe executable");
    let output = Command::new(binary)
        .args(["--check", "config", "--home"])
        .arg(home)
        .env("HOME", home)
        .env_remove("DISPLAY")
        .env_remove("WAYLAND_DISPLAY")
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(0), "findings are a valid native report");
    assert!(output.stderr.is_empty(), "JSON-only native output");
    let report = String::from_utf8(output.stdout).unwrap();
    assert!(!report.contains(SECRET), "config values and commands must stay private");
    report
}

fn status(report: &str, id: &str) -> String {
    let code = "import json,sys; r=json.load(sys.stdin); assert r['schema_version']==1; c=r['checks']; assert len({x['id'] for x in c})==len(c); x=next(x for x in c if x['id']==sys.argv[1]); assert isinstance(x['evidence'],list) and isinstance(x['next'],list) and x['message']; print(x['status'])";
    let mut child = Command::new("python3")
        .args(["-B", "-c", code, id])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    child.stdin.take().unwrap().write_all(report.as_bytes()).unwrap();
    let output = child.wait_with_output().unwrap();
    assert!(output.status.success(), "invalid native report: {}", String::from_utf8_lossy(&output.stderr));
    String::from_utf8(output.stdout).unwrap().trim().to_owned()
}

#[test]
fn a_missing_config_validates_first_run_defaults_without_creating_files() {
    let home = home("first-run");
    let report = probe(&home);
    assert_eq!(status(&report, "runtime.config"), "warning");
    assert_eq!(status(&report, "runtime.theme"), "warning");
    assert_eq!(fs::read_dir(home).unwrap().count(), 0, "doctor creates no config or theme");
}

#[test]
fn valid_config_and_active_theme_are_checked_without_exposing_commands() {
    let home = home("valid");
    let mut config = Config::default();
    config.layout.layers[0].groups[0].buttons[0].action = Action::Command { run: SECRET.to_owned() };
    let text = to_toml(&config).unwrap();
    save(&home, &text);
    let theme = home.join(".local/state/omarchy/current/theme/colors.toml");
    fs::create_dir_all(theme.parent().unwrap()).unwrap();
    fs::copy(
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/omarchy-themes/tokyo-night/colors.toml"),
        theme,
    )
    .unwrap();
    let report = probe(&home);
    assert_eq!(status(&report, "runtime.config"), "ok");
    assert_eq!(status(&report, "runtime.theme"), "ok");
    assert_eq!(fs::read_to_string(config_path(&home)).unwrap(), text);
}

#[test]
fn malformed_config_is_failed_without_echoing_its_source() {
    let home = home("syntax");
    let text = format!("version = [ '{SECRET}'\n");
    save(&home, &text);
    assert_eq!(status(&probe(&home), "runtime.config"), "failed");
    assert_eq!(fs::read_to_string(config_path(&home)).unwrap(), text);
}

#[test]
fn domain_issues_reject_a_parsed_config_and_keep_its_values_private() {
    let home = home("issues");
    let mut config = Config::default();
    config.version = 2;
    let button = &mut config.layout.layers[0].groups[0].buttons[0];
    button.label.clear();
    button.action = Action::Key { keys: vec![SECRET.to_owned()] };
    let text = to_toml(&config).unwrap();
    save(&home, &text);
    assert_eq!(status(&probe(&home), "runtime.config"), "failed");
    assert_eq!(fs::read_to_string(config_path(&home)).unwrap(), text);
}

#[test]
fn an_explicit_missing_or_malformed_theme_is_failed_instead_of_using_fallback() {
    let home = home("selected-theme");
    let theme = home.join("selected-colors.toml");
    let config = Config {
        theme: ThemeSource::File { path: theme.to_string_lossy().into_owned() },
        ..Config::default()
    };
    save(&home, &to_toml(&config).unwrap());
    assert_eq!(status(&probe(&home), "runtime.theme"), "failed");
    fs::write(&theme, format!("background = '{SECRET}'\n")).unwrap();
    assert_eq!(status(&probe(&home), "runtime.theme"), "failed");
}

#[test]
fn an_existing_unreadable_config_is_not_reported_as_a_first_run_default() {
    let home = home("unreadable");
    fs::create_dir_all(config_path(&home)).unwrap();
    assert_eq!(status(&probe(&home), "runtime.config"), "failed");
    assert!(config_path(&home).is_dir());
}
