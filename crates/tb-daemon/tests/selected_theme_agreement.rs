//! The daemon and the doctor read one temp `HOME` the way the configurator does: a first-run
//! default under an Omarchy theme without the optional `red` token is accepted by every process,
//! and a layout drawn in an external theme's unique token is drawn in that token's colour. Each
//! case builds its own home under cargo's temp dir (never the real one) and runs the real
//! binaries. Held keys come from a file the test writes (never `--keys auto` or a `/dev` path).
//! `touchbard --sim` needs a Vulkan adapter (lavapipe is fine); pixels compare ±2 unless named.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::fs::{self, File};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};
use std::thread;
use std::time::{Duration, Instant};

use tb_config::{ConfigFile, default_path};
use tb_core::config::{Config, ThemeSource, to_toml};
use tb_core::{Action, BarSize, BarTheme, Button, Chord, Group, SpacingRules, Width, place};
use tb_render::build_scene;

/// An Omarchy theme with only the required tokens: no `red`.
const MINIMAL: &str = "background = \"#101418\"\nforeground = \"#d0d4d8\"\naccent = \"#4c8cf0\"\n";
const TOKYO_NIGHT: &str = include_str!("../../../fixtures/omarchy-themes/tokyo-night/colors.toml");
/// A label drawn in the button text shows at least this many pixels within `LABEL_TOLERANCE` of
/// it. Measured on the minimal theme's Fn + Ctrl frame: `esc` (idle roles) 54 at ±2, and a
/// `lock` drawn in `background` 0.
const LABEL_PIXELS: usize = 20;
const LABEL_TOLERANCE: u8 = 2;

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

/// The RGBA pixels of a 2170x60 frame; `None` while it is missing or partly written.
fn decode(path: &Path) -> Option<Vec<u8>> {
    let mut reader = png::Decoder::new(File::open(path).ok()?).read_info().ok()?;
    let mut buf = vec![0; reader.output_buffer_size()];
    let info = reader.next_frame(&mut buf).ok()?;
    assert_eq!((info.width, info.height), (2170, 60), "{}", path.display());
    Some(buf)
}

fn pixel(frame: &[u8], (x, y): (u32, u32)) -> [u8; 3] {
    let i = ((y * 2170 + x) * 4) as usize;
    [frame[i], frame[i + 1], frame[i + 2]]
}

fn png_pixel(path: &Path, at: (u32, u32)) -> [u8; 3] {
    pixel(&decode(path).unwrap(), at)
}

fn close(a: [u8; 3], b: [u8; 3], tolerance: u8) -> bool {
    a.iter().zip(b).all(|(x, y)| x.abs_diff(y) <= tolerance)
}

/// One `x86_64` `struct input_event` (24 bytes), as `touchbard_keys.rs` writes them.
fn record(kind: u16, code: u16, value: i32) -> Vec<u8> {
    let mut r = Vec::with_capacity(24);
    r.extend_from_slice(&1_762_675_200_i64.to_ne_bytes());
    r.extend_from_slice(&123_456_i64.to_ne_bytes());
    r.extend_from_slice(&kind.to_ne_bytes());
    r.extend_from_slice(&code.to_ne_bytes());
    r.extend_from_slice(&value.to_ne_bytes());
    r
}

/// A keyboard file holding one report: Fn and `LeftCtrl` down (scan code, key, then `SYN_REPORT`).
fn fn_ctrl_held(path: &Path) {
    let mut bytes = Vec::new();
    for code in [464, 29] {
        bytes.extend(record(4, 4, 0x7_00e0));
        bytes.extend(record(1, code, 1));
    }
    bytes.extend(record(0, 0, 0));
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, bytes).unwrap();
}

/// The shipped default with every override the minimal theme cannot resolve dropped: the lock's
/// `red` fill and the `background` text paired with it, so the lock is drawn in the idle roles.
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
    assert_eq!(lock.style.text.as_deref(), Some("background"));
    lock.style.fill = None;
    lock.style.text = None;
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

#[test]
fn the_simulated_fn_ctrl_layer_draws_the_first_run_lock_in_the_idle_roles_under_a_theme_without_red() {
    let home = home_with_active("sim_minimal_lock", MINIMAL);
    let (sim, keys) = (home.join("sim"), home.join("keys/keyboard.bin"));
    fs::create_dir_all(&sim).unwrap();
    fn_ctrl_held(&keys);
    let mut bar = Command::new(env!("CARGO_BIN_EXE_touchbard"))
        .args(["--sim", sim.to_str().unwrap(), "--keys", keys.to_str().unwrap()])
        .env("HOME", &home)
        .env_remove("XDG_STATE_HOME")
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    // Frame 1 is the Default layer at start; the one report holding Fn + Ctrl draws frame 2
    // (the end of the key file then returns the bar to the Default layer in frame 3).
    let path = sim.join("frame-0002.png");
    let deadline = Instant::now() + Duration::from_secs(10);
    let frame = loop {
        if let Some(frame) = decode(&path) {
            break frame;
        }
        if Instant::now() >= deadline {
            let _ = bar.kill();
            panic!("{} never appeared: {:?}", path.display(), bar.wait_with_output().unwrap());
        }
        thread::sleep(Duration::from_millis(5));
    };
    drop(bar.stdin.take());
    let out = bar.wait_with_output().unwrap();
    assert!(out.status.success(), "exit {:?}, stderr: {}", out.status, String::from_utf8_lossy(&out.stderr));

    let minimal = BarTheme::from_omarchy_colors(MINIMAL).unwrap();
    let defaults = Config::default();
    let layer = defaults.layout.layer(Chord::FnCtrl).unwrap();
    let scene = build_scene(layer, &minimal, BarSize::MACBOOK_PRO, &SpacingRules::STANDARD, None).unwrap();
    let rect = |id: &str| scene.buttons.iter().find(|b| b.id.as_str() == id).unwrap().rect;
    let (lock, esc) = (rect("lock"), rect("esc"));
    let mid = u32::from(lock.y) + u32::from(lock.height) / 2;
    let fill = pixel(&frame, (u32::from(lock.x) + 8, mid));
    let idle = pixel(&frame, (u32::from(esc.x) + 8, mid));
    assert!(close(fill, idle, 2), "the lock has the idle fill esc has: {fill:?} versus {idle:?}");
    assert!(!close(fill, [0x10, 0x14, 0x18], 2), "the lock is drawn over the background: {fill:?}");

    // The label room (inside the side label padding and the border) holds glyph pixels in the
    // button text colour, the foreground.
    let border = u32::from(scene.border_width);
    let columns = u32::from(lock.x + scene.label_padding)..u32::from(lock.right() - scene.label_padding);
    let rows = u32::from(lock.y) + border + 1..u32::from(lock.y + lock.height) - border - 1;
    let text = [0xd0, 0xd4, 0xd8];
    let light = rows
        .flat_map(|y| columns.clone().map(move |x| (x, y)))
        .filter(|&at| close(pixel(&frame, at), text, LABEL_TOLERANCE))
        .count();
    assert!(light >= LABEL_PIXELS, "the lock label shows {light} pixels near {text:?}");
}
