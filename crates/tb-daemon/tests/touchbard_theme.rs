//! Functional: `touchbard` draws the active Omarchy theme of `$HOME`, or the bundled Tokyo Night
//! naming the colours file it could not use. Each test runs the binary with `HOME` set to its own
//! home under cargo's temp dir, built from the theme fixtures (never the real `$HOME`). Needs a
//! Vulkan adapter (lavapipe is fine); pixels compare with ±2 per channel.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::fs::{self, File};
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};

/// A point on the bar's edge padding (left of ESC): the bar background.
const BACKGROUND_AT: (u32, u32) = (2, 30);

fn fresh_dir(test: &str, what: &str) -> PathBuf {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join("touchbard_theme").join(test).join(what);
    if dir.exists() {
        fs::remove_dir_all(&dir).unwrap();
    }
    fs::create_dir_all(&dir).unwrap();
    dir
}

fn fixture(theme: &str) -> String {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../fixtures/omarchy-themes")
        .join(theme)
        .join("colors.toml");
    fs::read_to_string(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()))
}

/// A home whose active theme's `colors.toml` holds `colors`; returns the home and that file.
fn home_with_colors(test: &str, colors: &str) -> (PathBuf, PathBuf) {
    let home = fresh_dir(test, "home");
    let dir = home.join(".local/state/omarchy/current/theme");
    fs::create_dir_all(&dir).unwrap();
    let file = dir.join("colors.toml");
    fs::write(&file, colors).unwrap();
    (home, file)
}

/// `touchbard --sim <sim>` with `HOME` = `home` and stdin closed at once.
fn run_touchbard(home: &Path, sim: &Path) -> Output {
    Command::new(env!("CARGO_BIN_EXE_touchbard"))
        .args(["--sim", sim.to_str().unwrap()])
        .env("HOME", home)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .unwrap()
}

fn frame_files(dir: &Path) -> Vec<String> {
    let mut names: Vec<String> = fs::read_dir(dir)
        .unwrap()
        .map(|e| e.unwrap().file_name().into_string().unwrap())
        .filter(|n| n.starts_with("frame-") && Path::new(n).extension().is_some_and(|e| e == "png"))
        .collect();
    names.sort();
    names
}

fn png_pixel(path: &Path, (x, y): (u32, u32)) -> [u8; 3] {
    let mut reader = png::Decoder::new(File::open(path).unwrap()).read_info().unwrap();
    let mut buf = vec![0; reader.output_buffer_size()];
    let info = reader.next_frame(&mut buf).unwrap();
    assert_eq!((info.width, info.height), (2170, 60), "{}", path.display());
    assert_eq!(info.color_type, png::ColorType::Rgba, "{}", path.display());
    let i = ((y * info.width + x) * 4) as usize;
    [buf[i], buf[i + 1], buf[i + 2]]
}

fn assert_colour(got: [u8; 3], want: [u8; 3], what: &str) {
    assert!(got.iter().zip(want).all(|(g, w)| g.abs_diff(w) <= 2), "{what}: got {got:?}, want {want:?} ± 2");
}

#[test]
fn touchbard_draws_the_active_omarchy_theme_of_its_home() {
    let (home, _) = home_with_colors("catppuccin_latte", &fixture("catppuccin-latte"));
    let sim = fresh_dir("catppuccin_latte", "sim");

    let out = run_touchbard(&home, &sim);

    assert!(out.status.success(), "exit {:?}, stderr: {}", out.status, String::from_utf8_lossy(&out.stderr));
    assert_eq!(frame_files(&sim), ["frame-0001.png"]);
    // catppuccin-latte `background` = #eff1f5.
    assert_colour(
        png_pixel(&sim.join("frame-0001.png"), BACKGROUND_AT),
        [0xef, 0xf1, 0xf5],
        "bar background",
    );
}

#[test]
fn touchbard_draws_tokyo_night_and_names_the_colours_file_when_the_active_theme_is_invalid() {
    let without_accent: String = fixture("catppuccin-latte")
        .lines()
        .filter(|l| !l.starts_with("accent"))
        .flat_map(|l| [l, "\n"])
        .collect();
    let (home, colors) = home_with_colors("missing_accent", &without_accent);
    let sim = fresh_dir("missing_accent", "sim");

    let out = run_touchbard(&home, &sim);

    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(out.status.success(), "exit {:?}, stderr: {stderr}", out.status);
    assert_eq!(frame_files(&sim), ["frame-0001.png"]);
    // The bundled Tokyo Night `background` = #1a1b26.
    assert_colour(
        png_pixel(&sim.join("frame-0001.png"), BACKGROUND_AT),
        [0x1a, 0x1b, 0x26],
        "bar background",
    );
    assert!(stderr.contains(colors.to_str().unwrap()), "stderr names {}: {stderr}", colors.display());
}
