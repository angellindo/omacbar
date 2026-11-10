//! Saved config reaches the real daemon through startup and directory-watch notifications.
//! Every input, theme, recorded key and PNG lives under a temporary HOME; no device is opened.
//! A frame is observed only once its PNG decodes; pixels use ±2 per channel.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::fs::{self, File};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::thread;
use std::time::{Duration, Instant};

use tb_config::default_path;
use tb_core::config::{Config, ThemeSource, parse, to_toml};
use tb_core::{Action, Button, Chord, Group, Layer, Layout};

const WAIT: Duration = Duration::from_secs(15);
const POLL: Duration = Duration::from_millis(10);
const QUIET: Duration = Duration::from_millis(350);
const RED: [u8; 3] = [0xe0, 0x30, 0x40];
const GREEN: [u8; 3] = [0x20, 0xc0, 0x50];
const BLUE: [u8; 3] = [0x30, 0x60, 0xe0];
const TOKYO: [u8; 3] = [0x1a, 0x1b, 0x26];
const LATTE: [u8; 3] = [0xef, 0xf1, 0xf5];

fn home(name: &str) -> PathBuf {
    static NEXT: AtomicUsize = AtomicUsize::new(0);
    let path = Path::new(env!("CARGO_TARGET_TMPDIR")).join("touchbard_config").join(format!(
        "{name}-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    fs::create_dir_all(&path).unwrap();
    path
}

fn config(fill: &str, key: &str) -> Config {
    let mut button = Button::new("saved", "saved", Action::Key { keys: vec![key.to_owned()] });
    button.style.fill = Some(fill.to_owned());
    Config {
        layout: Layout {
            layers: vec![Layer { chord: Chord::Default, groups: vec![Group { buttons: vec![button] }] }],
        },
        ..Config::default()
    }
}

fn save(home: &Path, config: &Config) {
    let path = default_path(home);
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    // Same-directory replacement is the configurator's save mechanism. Repeated saves replace
    // the inode and expose an incorrect watch on the file instead of its parent directory.
    let staged = path.with_extension("next");
    fs::write(&staged, to_toml(config).unwrap()).unwrap();
    fs::rename(staged, path).unwrap();
}

fn theme(home: &Path, name: &str) {
    let current = home.join(".local/state/omarchy/current");
    fs::create_dir_all(current.join("theme")).unwrap();
    let fixture = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../fixtures/omarchy-themes")
        .join(name)
        .join("colors.toml");
    fs::copy(fixture, current.join("theme/colors.toml")).unwrap();
    fs::write(current.join("theme.name"), name).unwrap();
}

struct Bar {
    child: Child,
    sim: PathBuf,
    log: PathBuf,
    keys: PathBuf,
}

impl Bar {
    fn start(home: &Path) -> Self {
        let (sim, log, keys) = (home.join("sim"), home.join("stderr"), home.join("keys.bin"));
        let child = Command::new(env!("CARGO_BIN_EXE_touchbard"))
            .arg("--sim")
            .arg(&sim)
            .arg("--emit")
            .arg(&keys)
            .env("HOME", home)
            .stdin(Stdio::piped())
            .stdout(Stdio::null())
            .stderr(File::create(&log).unwrap())
            .spawn()
            .unwrap();
        Self { child, sim, log, keys }
    }

    fn stderr(&self) -> String {
        fs::read_to_string(&self.log).unwrap_or_default()
    }

    fn frame(&self, n: u32) -> Vec<u8> {
        let path = self.sim.join(format!("frame-{n:04}.png"));
        let deadline = Instant::now() + WAIT;
        loop {
            if let Some(pixels) = decode(&path) {
                return pixels;
            }
            assert!(Instant::now() < deadline, "{} missing; {}", path.display(), self.stderr());
            thread::sleep(POLL);
        }
    }

    fn fill(&self, n: u32, expected: [u8; 3]) {
        colour(&self.frame(n), 20, expected);
    }

    fn logged(&self, text: &str) {
        let deadline = Instant::now() + WAIT;
        while !self.stderr().contains(text) {
            assert!(Instant::now() < deadline, "missing diagnostic {text:?}: {}", self.stderr());
            thread::sleep(POLL);
        }
    }

    fn quiet_after(&mut self, n: u32) {
        thread::sleep(QUIET);
        assert!(self.child.try_wait().unwrap().is_none(), "daemon stopped: {}", self.stderr());
        assert!(
            !self.sim.join(format!("frame-{:04}.png", n + 1)).exists(),
            "rejection or unchanged source must not present another frame"
        );
    }

    fn tap(&mut self, count: usize, code: u16) {
        self.child.stdin.as_mut().unwrap().write_all(b"down 40\nup\n").unwrap();
        let deadline = Instant::now() + WAIT;
        loop {
            let bytes = fs::read(&self.keys).unwrap_or_default();
            if bytes.len() >= count * 96 {
                let last = &bytes[(count - 1) * 96..count * 96];
                let records: Vec<(u16, u16, i32)> = last
                    .as_chunks::<24>()
                    .0
                    .iter()
                    .map(|r| {
                        (
                            u16::from_ne_bytes([r[16], r[17]]),
                            u16::from_ne_bytes([r[18], r[19]]),
                            i32::from_ne_bytes([r[20], r[21], r[22], r[23]]),
                        )
                    })
                    .collect();
                assert_eq!(records, [(1, code, 1), (0, 0, 0), (1, code, 0), (0, 0, 0)]);
                return;
            }
            assert!(Instant::now() < deadline, "tap {count} missing; {}", self.stderr());
            thread::sleep(POLL);
        }
    }

    fn close(&mut self) {
        drop(self.child.stdin.take());
        let deadline = Instant::now() + WAIT;
        loop {
            if let Some(status) = self.child.try_wait().unwrap() {
                assert!(status.success(), "exit {status:?}: {}", self.stderr());
                return;
            }
            assert!(Instant::now() < deadline, "stdin close did not stop daemon");
            thread::sleep(POLL);
        }
    }
}

impl Drop for Bar {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

fn decode(path: &Path) -> Option<Vec<u8>> {
    let mut reader = png::Decoder::new(File::open(path).ok()?).read_info().ok()?;
    let mut pixels = vec![0; reader.output_buffer_size()];
    let info = reader.next_frame(&mut pixels).ok()?;
    assert_eq!((info.width, info.height, info.color_type), (2170, 60, png::ColorType::Rgba));
    Some(pixels)
}

fn colour(pixels: &[u8], x: usize, expected: [u8; 3]) {
    let i = (30 * 2170 + x) * 4;
    let got = &pixels[i..i + 3];
    assert!(
        got.iter().zip(expected).all(|(a, b)| a.abs_diff(b) <= 2),
        "pixel ({x},30) = {got:?}; expected {expected:?} ±2"
    );
}

#[test]
fn missing_config_is_created_as_the_existing_default() {
    let home = home("missing");
    let mut bar = Bar::start(&home);
    colour(&bar.frame(1), 2, TOKYO);
    bar.close();
    let path = default_path(&home);
    assert!(path.is_file(), "startup must create {}", path.display());
    assert_eq!(parse(&fs::read_to_string(path).unwrap()).unwrap(), Config::default());
}

#[test]
fn saved_layout_and_key_action_control_the_initial_bar() {
    let home = home("startup");
    save(&home, &config("#e03040", "F9"));
    let mut bar = Bar::start(&home);
    bar.fill(1, RED);
    bar.tap(1, 67);
    bar.fill(3, RED);
    bar.close();
}

#[test]
fn invalid_startup_reports_every_issue_without_overwriting_the_file() {
    let home = home("invalid-startup");
    let mut invalid = config("#e03040", "F9");
    let button = &mut invalid.layout.layers[0].groups[0].buttons[0];
    button.label.clear();
    button.action = Action::Key { keys: vec!["NeverAKey".to_owned()] };
    save(&home, &invalid);
    let before = fs::read(default_path(&home)).unwrap();
    let mut bar = Bar::start(&home);
    colour(&bar.frame(1), 2, TOKYO);
    bar.close();
    assert_eq!(fs::read(default_path(&home)).unwrap(), before);
    let log = bar.stderr();
    assert!(log.contains("config.toml"), "diagnostic must name config: {log}");
    assert!(log.contains("empty label"), "all validation issues must be reported: {log}");
    assert!(log.contains("NeverAKey"), "all validation issues must be reported: {log}");
}

#[test]
fn malformed_startup_config_is_reported_preserved_and_can_be_replaced() {
    let home = home("malformed-startup");
    save(&home, &Config::default());
    let path = default_path(&home);
    let malformed = b"version = [not valid TOML";
    fs::write(&path, malformed).unwrap();
    let mut bar = Bar::start(&home);
    colour(&bar.frame(1), 2, TOKYO);
    bar.logged("config.toml");
    assert_eq!(fs::read(&path).unwrap(), malformed);
    save(&home, &config("#e03040", "F9"));
    bar.fill(2, RED);
    bar.tap(1, 67);
    bar.fill(4, RED);
    bar.close();
}

#[test]
fn atomic_and_in_place_saves_reload_and_invalid_saves_keep_the_last_action() {
    let home = home("reload");
    save(&home, &config("#e03040", "F9"));
    let mut bar = Bar::start(&home);
    bar.fill(1, RED);
    save(&home, &config("#20c050", "F10"));
    bar.fill(2, GREEN);
    save(&home, &config("#3060e0", "F11"));
    bar.fill(3, BLUE);
    fs::write(default_path(&home), to_toml(&config("#e03040", "F12")).unwrap()).unwrap();
    bar.fill(4, RED);
    fs::write(default_path(&home), "not = [valid TOML").unwrap();
    bar.logged("config.toml");
    bar.quiet_after(4);
    bar.tap(1, 88);
    bar.fill(6, RED);
    save(&home, &config("#20c050", "F10"));
    bar.fill(7, GREEN);
    bar.tap(2, 68);
    bar.fill(9, GREEN);
    bar.close();
}

#[test]
fn a_config_read_failure_keeps_the_layout_and_later_replacement_recovers() {
    let home = home("read-failure");
    save(&home, &config("#e03040", "F9"));
    let mut bar = Bar::start(&home);
    bar.fill(1, RED);
    let path = default_path(&home);
    fs::rename(&path, path.with_extension("old")).unwrap();
    let staged = path.with_extension("directory");
    fs::create_dir(&staged).unwrap();
    fs::rename(staged, &path).unwrap();
    bar.logged("config.toml");
    bar.quiet_after(1);
    bar.tap(1, 67);
    bar.fill(3, RED);
    fs::remove_dir(&path).unwrap();
    save(&home, &config("#20c050", "F10"));
    bar.fill(4, GREEN);
    bar.tap(2, 68);
    bar.fill(6, GREEN);
    bar.close();
}

#[test]
fn file_theme_resolves_before_validation_and_source_changes_are_atomic() {
    let home = home("sources");
    theme(&home, "tokyo-night");
    let colors = home.join("file-colors.toml");
    fs::write(
        &colors,
        "background = '#101820'\nforeground = '#ffffff'\naccent = '#abcdef'\nonly_file = '#e03040'\n",
    )
    .unwrap();
    let mut saved = config("only_file", "F9");
    saved.theme = ThemeSource::File { path: colors.to_string_lossy().into_owned() };
    save(&home, &saved);
    let mut bar = Bar::start(&home);
    bar.fill(1, RED);
    colour(&bar.frame(1), 2, [0x10, 0x18, 0x20]);
    theme(&home, "catppuccin-latte");
    bar.quiet_after(1);

    save(&home, &config("#20c050", "F10"));
    let switched = bar.frame(2);
    colour(&switched, 20, GREEN);
    colour(&switched, 2, LATTE);
    bar.quiet_after(2);

    let missing = home.join("missing-colors.toml");
    let mut rejected = config("#3060e0", "F11");
    rejected.theme = ThemeSource::File { path: missing.to_string_lossy().into_owned() };
    save(&home, &rejected);
    bar.logged("missing-colors.toml");
    bar.quiet_after(2);
    bar.tap(1, 68);
    let retained = bar.frame(4);
    colour(&retained, 20, GREEN);
    colour(&retained, 2, LATTE);

    save(&home, &saved);
    let restored = bar.frame(5);
    colour(&restored, 20, RED);
    colour(&restored, 2, [0x10, 0x18, 0x20]);
    bar.quiet_after(5);
    bar.close();
}
