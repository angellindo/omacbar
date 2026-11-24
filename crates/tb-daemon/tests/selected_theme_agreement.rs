//! The daemon and the doctor read one temp `HOME` the way the configurator does: a first-run
//! default under an Omarchy theme without the optional `red` token is accepted by every process,
//! and a layout drawn in an external theme's unique token is drawn in that token's colour. Each
//! case builds its own home under cargo's temp dir (never the real one) and runs the real
//! binaries. `touchbard --sim` needs a Vulkan adapter (lavapipe is fine); pixels compare ±2.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::fs::{self, File};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};

use tb_config::{ConfigFile, default_path};
use tb_core::config::{Config, ThemeSource, to_toml};
use tb_core::{Action, BarSize, BarTheme, Button, Chord, Group, SpacingRules, Width, place};

/// An Omarchy theme with only the required tokens: no `red`.
const MINIMAL: &str = "background = \"#101418\"\nforeground = \"#d0d4d8\"\naccent = \"#4c8cf0\"\n";
const TOKYO_NIGHT: &str = include_str!("../../../fixtures/omarchy-themes/tokyo-night/colors.toml");

fn home_with_active(test: &str, colors: &str) -> PathBuf {
    let home = Path::new(env!("CARGO_TARGET_TMPDIR")).join("selected_theme_agreement").join(test);
    if home.exists() {
        fs::remove_dir_all(&home).unwrap();
    }
    let dir = home.join(".local/state/omarchy/current/theme");
    fs::create_dir_all(&dir).unwrap();
    fs::write(dir.join("colors.toml"), colors).unwrap();
    home
}

fn run_touchbard(home: &Path, sim: &Path) -> Output {
    fs::create_dir_all(sim).unwrap();
    let out = Command::new(env!("CARGO_BIN_EXE_touchbard"))
        .args(["--sim", sim.to_str().unwrap()])
        .env("HOME", home)
        // The generation store follows HOME, inside the temp home, never the caller's state.
        .env_remove("XDG_STATE_HOME")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .unwrap();
    assert!(out.status.success(), "exit {:?}, stderr: {}", out.status, String::from_utf8_lossy(&out.stderr));
    out
}

fn probe(home: &Path) -> String {
    let output = Command::new(env!("CARGO_BIN_EXE_touchbar-doctor-probe"))
        .args(["--check", "config", "--home"])
        .arg(home)
        .env("HOME", home)
        // The generation store follows HOME, inside the temp home, never the caller's state.
        .env_remove("XDG_STATE_HOME")
        .env_remove("DISPLAY")
        .env_remove("WAYLAND_DISPLAY")
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(0), "findings are a valid native report");
    String::from_utf8(output.stdout).unwrap()
}

fn status(report: &str, id: &str) -> String {
    let code = "import json,sys; r=json.load(sys.stdin); print(next(x for x in r['checks'] if x['id']==sys.argv[1])['status'])";
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

fn png_pixel(path: &Path, (x, y): (u32, u32)) -> [u8; 3] {
    let mut reader = png::Decoder::new(File::open(path).unwrap()).read_info().unwrap();
    let mut buf = vec![0; reader.output_buffer_size()];
    let info = reader.next_frame(&mut buf).unwrap();
    assert_eq!((info.width, info.height), (2170, 60), "{}", path.display());
    let i = ((y * info.width + x) * 4) as usize;
    [buf[i], buf[i + 1], buf[i + 2]]
}

/// The shipped default with every override the minimal theme cannot resolve dropped: the lock's
/// `red` fill.
fn minimal_default() -> Config {
    let mut config = Config::default();
    let lock = config
        .layout
        .layer_mut(Chord::FnCtrl)
        .unwrap()
        .groups
        .iter_mut()
        .flat_map(|g| &mut g.buttons)
        .find(|b| b.id.as_str() == "lock")
        .unwrap();
    assert_eq!(lock.style.fill.as_deref(), Some("red"));
    lock.style.fill = None;
    config
}

#[test]
fn the_doctor_accepts_the_first_run_default_under_a_theme_without_red() {
    let home = home_with_active("doctor_minimal", MINIMAL);
    let report = probe(&home);
    assert_eq!(status(&report, "runtime.config"), "warning", "a missing config is a validated first run");
    assert_eq!(status(&report, "runtime.theme"), "ok");
    assert!(!default_path(&home).exists(), "the doctor creates no config");
}

#[test]
fn the_daemon_first_run_under_a_theme_without_red_writes_a_default_it_accepts_on_reload() {
    let home = home_with_active("daemon_minimal", MINIMAL);
    run_touchbard(&home, &home.join("sim"));
    let minimal = BarTheme::from_omarchy_colors(MINIMAL).unwrap();
    let mut file = ConfigFile::new(default_path(&home));
    file.reload_resolved(&minimal).unwrap();
    assert_eq!(file.config(), &minimal_default());
    let report = probe(&home);
    assert_eq!(status(&report, "runtime.config"), "ok", "the doctor accepts what the daemon wrote");
    assert_eq!(status(&report, "runtime.theme"), "ok");
}

#[test]
fn the_daemon_draws_an_external_theme_token_from_the_same_home_the_doctor_accepts() {
    let home = home_with_active("daemon_external", TOKYO_NIGHT);
    let theme = home.join("themes/external/colors.toml");
    fs::create_dir_all(theme.parent().unwrap()).unwrap();
    fs::write(&theme, format!("{TOKYO_NIGHT}\nexternal_only = \"#123456\"\n")).unwrap();
    let mut config = Config {
        theme: ThemeSource::File { path: theme.to_string_lossy().into_owned() },
        ..Config::default()
    };
    let mut probe_button = Button::new("probe", "x", Action::None).with_width(Width::Px(400));
    probe_button.style.fill = Some("external_only".to_owned());
    let layer = config.layout.layer_mut(Chord::Default).unwrap();
    layer.groups = vec![Group { buttons: vec![probe_button] }];
    let rect = place(layer, BarSize::MACBOOK_PRO, &SpacingRules::STANDARD).unwrap()[0].rect;
    let path = default_path(&home);
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    let text = to_toml(&config).unwrap();
    fs::write(&path, &text).unwrap();

    let sim = home.join("sim");
    run_touchbard(&home, &sim);
    let at = (u32::from(rect.x) + 24, u32::from(rect.y) + u32::from(rect.height) / 2);
    let got = png_pixel(&sim.join("frame-0001.png"), at);
    assert!(
        got.iter().zip([0x12u8, 0x34, 0x56]).all(|(g, w)| g.abs_diff(w) <= 2),
        "the button is drawn in the external token: {got:?}"
    );
    let report = probe(&home);
    assert_eq!(status(&report, "runtime.config"), "ok");
    assert_eq!(status(&report, "runtime.theme"), "ok");
    assert_eq!(fs::read_to_string(&path).unwrap(), text, "neither process rewrites the file");
}
