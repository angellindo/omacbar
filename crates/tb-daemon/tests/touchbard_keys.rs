//! Functional: `touchbard --keys PATH` follows the held Fn and modifier keys read from PATH as raw
//! `x86_64` `struct input_event` records (24 bytes) and shows the matching layer on the simulator.
//! PATH is always a file this test wrote under cargo's temp dir: no test passes `--keys auto` or
//! a `/dev` path. Each run gets an empty `HOME` (bundled Tokyo Night) and stdin held open until
//! the frames are checked. Needs a Vulkan adapter (lavapipe is fine). Which layer a frame shows is
//! read from sampled pixels: a button beside its label and the background in every gap between
//! buttons, from `build_scene` over `tb_core::place`, ±2 per channel.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::fs::{self, File};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, ExitStatus, Stdio};
use std::thread;
use std::time::{Duration, Instant};

use tb_core::{BarSize, BarTheme, Chord, Rgba, SpacingRules, presets};
use tb_render::{BarScene, build_scene};

/// How long to wait for a frame before calling it missing (loaded laptop).
const MISSING: Duration = Duration::from_secs(10);
/// How long to watch for frames that should not come.
const QUIET: Duration = Duration::from_millis(500);
const POLL: Duration = Duration::from_millis(5);

const EV_SYN: u16 = 0;
const EV_KEY: u16 = 1;
const EV_MSC: u16 = 4;
const KEY_FN: u16 = 464;
const KEY_LEFTCTRL: u16 = 29;

fn record(kind: u16, code: u16, value: i32) -> Vec<u8> {
    let mut r = Vec::with_capacity(24);
    r.extend_from_slice(&1_762_675_200_i64.to_ne_bytes());
    r.extend_from_slice(&123_456_i64.to_ne_bytes());
    r.extend_from_slice(&kind.to_ne_bytes());
    r.extend_from_slice(&code.to_ne_bytes());
    r.extend_from_slice(&value.to_ne_bytes());
    r
}

/// One keyboard report: each key's scan code and state, then `SYN_REPORT`.
fn report(keys: &[(u16, i32)]) -> Vec<u8> {
    let mut bytes = Vec::new();
    for &(code, value) in keys {
        bytes.extend(record(EV_MSC, 4, 0x7_00e0));
        bytes.extend(record(EV_KEY, code, value));
    }
    bytes.extend(record(EV_SYN, 0, 0));
    bytes
}

fn fresh_dir(test: &str, what: &str) -> PathBuf {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join("touchbard_keys").join(test).join(what);
    if dir.exists() {
        fs::remove_dir_all(&dir).unwrap();
    }
    fs::create_dir_all(&dir).unwrap();
    dir
}

/// A running `touchbard`, killed if a test fails before it exits. stderr goes to a file.
struct Touchbard {
    child: Child,
    sim: PathBuf,
    stderr: PathBuf,
}

impl Touchbard {
    fn start(test: &str, args: &[&str], sim: &Path) -> Self {
        let home = fresh_dir(test, "home");
        let stderr = fresh_dir(test, "log").join("stderr.txt");
        let child = Command::new(env!("CARGO_BIN_EXE_touchbard"))
            .args(args)
            .env("HOME", &home)
            .stdin(Stdio::piped())
            .stdout(Stdio::null())
            .stderr(File::create(&stderr).unwrap())
            .spawn()
            .unwrap();
        Self { child, sim: sim.to_path_buf(), stderr }
    }

    /// The pixels of frame `n` once it decodes completely; panics after `MISSING`.
    fn frame(&self, n: u32) -> Png {
        let path = self.sim.join(format!("frame-{n:04}.png"));
        let deadline = Instant::now() + MISSING;
        loop {
            if let Some(png) = Png::decode(&path) {
                return png;
            }
            assert!(
                Instant::now() < deadline,
                "{} never appeared; stderr: {}",
                path.display(),
                self.stderr_text()
            );
            thread::sleep(POLL);
        }
    }

    fn frames(&self) -> Vec<String> {
        let mut names: Vec<String> = fs::read_dir(&self.sim)
            .unwrap()
            .map(|e| e.unwrap().file_name().into_string().unwrap())
            .filter(|n| n.starts_with("frame-") && Path::new(n).extension().is_some_and(|e| e == "png"))
            .collect();
        names.sort();
        names
    }

    fn running(&mut self) -> bool {
        self.child.try_wait().unwrap().is_none()
    }

    /// Closes stdin and waits for the exit.
    fn close_stdin(&mut self) -> ExitStatus {
        drop(self.child.stdin.take());
        let deadline = Instant::now() + MISSING;
        loop {
            if let Some(status) = self.child.try_wait().unwrap() {
                return status;
            }
            assert!(Instant::now() < deadline, "touchbard did not exit after stdin closed");
            thread::sleep(POLL);
        }
    }

    fn stderr_text(&self) -> String {
        fs::read_to_string(&self.stderr).unwrap_or_default()
    }
}

impl Drop for Touchbard {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

/// A decoded 2170x60 RGBA frame.
struct Png {
    rgba: Vec<u8>,
}

impl Png {
    /// `None` while the file is missing or partly written.
    fn decode(path: &Path) -> Option<Self> {
        let mut reader = png::Decoder::new(File::open(path).ok()?).read_info().ok()?;
        let mut buf = vec![0; reader.output_buffer_size()];
        let info = reader.next_frame(&mut buf).ok()?;
        assert_eq!((info.width, info.height), (2170, 60), "{}", path.display());
        assert_eq!(info.color_type, png::ColorType::Rgba, "{}", path.display());
        Some(Self { rgba: buf })
    }

    fn pixel(&self, x: u16, y: u16) -> [u8; 4] {
        let i = (usize::from(y) * 2170 + usize::from(x)) * 4;
        [self.rgba[i], self.rgba[i + 1], self.rgba[i + 2], self.rgba[i + 3]]
    }
}

fn bytes(c: Rgba) -> [u8; 4] {
    [c.r, c.g, c.b, c.a]
}

/// The `chord` layer of the default preset in the bundled Tokyo Night, nothing pressed.
fn scene(chord: Chord) -> BarScene {
    let layout = presets::default_layout();
    build_scene(
        layout.layer(chord).unwrap(),
        &BarTheme::omarchy_fallback(),
        BarSize::MACBOOK_PRO,
        &SpacingRules::STANDARD,
        None,
    )
    .unwrap()
}

/// What a sampled pixel must be.
enum Want {
    /// This colour, ±2 per channel.
    Colour([u8; 4]),
    /// A button drawn over the background: anything but the background colour. Idle fills are
    /// translucent and blended by the GPU, so their composited value is not a scene colour.
    NotBackground,
}

/// The sampled points where `png` does not look like `scene`: each button 8 px inside its left
/// edge (clear of the border and the centred label) shows its fill, exactly when it is opaque,
/// and the middle of every gap between neighbouring buttons and of both edge paddings shows the
/// background. Empty: `png` shows `scene`.
fn mismatches(png: &Png, scene: &BarScene) -> Vec<String> {
    let y = scene.buttons[0].rect.y + scene.buttons[0].rect.height / 2;
    let background = bytes(scene.background);
    let mut points: Vec<(u16, Want, String)> = Vec::new();
    for b in &scene.buttons {
        let want = if b.fill.a == 255 { Want::Colour(bytes(b.fill)) } else { Want::NotBackground };
        points.push((b.rect.x + 8, want, format!("{} fill", b.id.as_str())));
    }
    let mut edges = vec![0];
    for b in &scene.buttons {
        edges.extend([b.rect.x, b.rect.right()]);
    }
    edges.push(scene.size.width);
    for gap in edges.chunks(2) {
        points.push((gap[0].midpoint(gap[1]), Want::Colour(background), format!("background at {}", gap[0])));
    }
    let close = |a: [u8; 4], b: [u8; 4]| a.iter().zip(b).all(|(x, y)| x.abs_diff(y) <= 2);
    points
        .into_iter()
        .filter_map(|(x, want, what)| {
            let got = png.pixel(x, y);
            match want {
                Want::Colour(c) if !close(got, c) => Some(format!("{what} ({x}, {y}) = {got:?}, want {c:?}")),
                Want::NotBackground if close(got, background) => {
                    Some(format!("{what} ({x}, {y}) = {got:?} is the background"))
                }
                _ => None,
            }
        })
        .collect()
}

/// `png` shows the `chord` layer and none of the other layers of the default preset.
fn assert_shows(png: &Png, chord: Chord, what: &str) {
    let wrong = mismatches(png, &scene(chord));
    assert!(wrong.is_empty(), "{what} is not the {chord:?} layer: {wrong:#?}");
    for other in [Chord::Default, Chord::Fn, Chord::FnCtrl] {
        if other != chord {
            assert!(!mismatches(png, &scene(other)).is_empty(), "{what} also looks like the {other:?} layer");
        }
    }
}

fn keys_file(test: &str, reports: &[Vec<u8>]) -> PathBuf {
    let path = fresh_dir(test, "keys").join("keyboard.bin");
    fs::write(&path, reports.concat()).unwrap();
    path
}

#[test]
fn touchbard_shows_the_fn_and_fn_ctrl_layers_while_those_keys_are_held() {
    let keys = keys_file(
        "fn_ctrl",
        &[report(&[(KEY_FN, 1)]), report(&[(KEY_LEFTCTRL, 1)]), report(&[(KEY_FN, 0), (KEY_LEFTCTRL, 0)])],
    );
    let sim = fresh_dir("fn_ctrl", "sim");
    // Flags in any order: --keys before --sim here, after it in the other cases.
    let mut bar = Touchbard::start(
        "fn_ctrl",
        &["--keys", keys.to_str().unwrap(), "--sim", sim.to_str().unwrap()],
        &sim,
    );

    let expected = [Chord::Default, Chord::Fn, Chord::FnCtrl, Chord::Default];
    let frames: Vec<Png> = (1..=4).map(|n| bar.frame(n)).collect();
    thread::sleep(QUIET);
    assert_eq!(
        bar.frames(),
        ["frame-0001.png", "frame-0002.png", "frame-0003.png", "frame-0004.png"],
        "start, then one frame per modifier change"
    );
    for (n, (png, chord)) in frames.iter().zip(expected).enumerate() {
        assert_shows(png, chord, &format!("frame-{:04}", n + 1));
    }
    assert!(bar.running(), "the end of the keyboard does not end touchbard");

    let status = bar.close_stdin();
    assert!(status.success(), "exit {status:?}, stderr: {}", bar.stderr_text());
}

#[test]
fn touchbard_names_a_keyboard_it_cannot_open_and_stays_on_the_default_layer() {
    let missing = fresh_dir("missing", "keys").join("no-such-keyboard");
    let sim = fresh_dir("missing", "sim");
    let mut bar = Touchbard::start(
        "missing",
        &["--sim", sim.to_str().unwrap(), "--keys", missing.to_str().unwrap()],
        &sim,
    );

    let start = bar.frame(1);
    thread::sleep(QUIET);
    assert!(bar.running(), "a keyboard that cannot be opened does not end touchbard");
    assert_eq!(bar.frames(), ["frame-0001.png"]);
    assert_shows(&start, Chord::Default, "frame-0001");

    let status = bar.close_stdin();
    assert!(status.success(), "exit {status:?}, stderr: {}", bar.stderr_text());
    let stderr = bar.stderr_text();
    assert!(stderr.contains(missing.to_str().unwrap()), "stderr names the keyboard path: {stderr}");
}

#[test]
fn a_keyboard_that_fails_mid_stream_returns_the_bar_to_the_default_layer() {
    let mut cut = record(EV_KEY, KEY_LEFTCTRL, 1);
    cut.truncate(9);
    let keys = keys_file("fails", &[report(&[(KEY_FN, 1)]), cut]);
    let sim = fresh_dir("fails", "sim");
    let mut bar =
        Touchbard::start("fails", &["--sim", sim.to_str().unwrap(), "--keys", keys.to_str().unwrap()], &sim);

    let frames: Vec<Png> = (1..=3).map(|n| bar.frame(n)).collect();
    thread::sleep(QUIET);
    assert_eq!(bar.frames(), ["frame-0001.png", "frame-0002.png", "frame-0003.png"]);
    for (n, (png, chord)) in frames.iter().zip([Chord::Default, Chord::Fn, Chord::Default]).enumerate() {
        assert_shows(png, chord, &format!("frame-{:04}", n + 1));
    }
    assert!(bar.running(), "a keyboard read error does not end touchbard");

    let status = bar.close_stdin();
    assert!(status.success(), "exit {status:?}, stderr: {}", bar.stderr_text());
}

#[test]
fn a_keys_flag_without_a_path_or_given_twice_is_usage_naming_keys_and_exit_two() {
    let sim = fresh_dir("usage", "sim");
    let sim = sim.to_str().unwrap();
    for args in [
        vec!["--keys"],
        vec!["--sim", sim, "--keys"],
        vec!["--keys", "/nonexistent/a", "--keys", "/nonexistent/b", "--sim", sim],
    ] {
        let home = fresh_dir("usage", "home");
        let out = Command::new(env!("CARGO_BIN_EXE_touchbard"))
            .args(&args)
            .env("HOME", &home)
            .stdin(Stdio::null())
            .output()
            .unwrap();
        let stderr = String::from_utf8_lossy(&out.stderr).to_lowercase();
        assert_eq!(out.status.code(), Some(2), "{args:?}: stderr {stderr}");
        assert!(
            stderr.contains("usage") && stderr.contains("--keys"),
            "{args:?}: usage names --keys: {stderr}"
        );
    }
}
