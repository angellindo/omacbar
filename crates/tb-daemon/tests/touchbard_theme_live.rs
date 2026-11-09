//! Functional: a running `touchbard` redraws the bar in the new theme within 1 s of a theme
//! switch or of the shipped `theme-set.d` hook. Each test runs the binary with `HOME` set to its
//! own home under cargo's temp dir, built from the theme fixtures (never the real `$HOME`), and
//! stdin held open. The simulator writes each PNG in place, so a frame counts from its first
//! complete decode. Needs a Vulkan adapter (lavapipe is fine); pixels compare with ±2 per channel.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::fs::{self, File};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, ExitStatus, Stdio};
use std::thread;
use std::time::{Duration, Instant};

/// How fast a theme change must reach the bar (ticket Acceptance).
const WITHIN: Duration = Duration::from_secs(1);
/// How long to wait for a frame before calling it missing (loaded laptop).
const MISSING: Duration = Duration::from_secs(10);
const QUIET: Duration = Duration::from_millis(500);
const POLL: Duration = Duration::from_millis(5);
/// A point on the bar's edge padding (left of ESC): the bar background.
const BACKGROUND_AT: (u32, u32) = (2, 30);
const TOKYO_NIGHT_BACKGROUND: [u8; 3] = [0x1a, 0x1b, 0x26];
const CATPPUCCIN_LATTE_BACKGROUND: [u8; 3] = [0xef, 0xf1, 0xf5];

fn fresh_dir(test: &str, what: &str) -> PathBuf {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join("touchbard_theme_live").join(test).join(what);
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

fn hook() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../contrib/omarchy/hooks/theme-set.d/touchbar-customizer")
}

fn current(home: &Path) -> PathBuf {
    home.join(".local/state/omarchy/current")
}

/// A home where `omarchy theme set <theme>` has run.
fn omarchy_home(test: &str, theme: &str) -> PathBuf {
    let home = fresh_dir(test, "home");
    let current = current(&home);
    fs::create_dir_all(current.join("theme")).unwrap();
    fs::write(current.join("theme/colors.toml"), fixture(theme)).unwrap();
    fs::write(current.join("theme.generation"), "1-1\n").unwrap();
    fs::write(current.join("theme.name"), format!("{theme}\n")).unwrap();
    home
}

/// A running `touchbard --sim`, killed if a test fails before it exits.
struct Touchbard {
    child: Child,
    sim: PathBuf,
}

impl Touchbard {
    fn start(home: &Path, sim: PathBuf) -> Self {
        let child = Command::new(env!("CARGO_BIN_EXE_touchbard"))
            .args(["--sim", sim.to_str().unwrap()])
            .env("HOME", home)
            .stdin(Stdio::piped())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .unwrap();
        Self { child, sim }
    }

    /// The background of frame `n` and when it first decoded; panics after `MISSING`.
    fn frame(&self, n: u32) -> ([u8; 3], Instant) {
        let path = self.sim.join(format!("frame-{n:04}.png"));
        let deadline = Instant::now() + MISSING;
        loop {
            if let Some(px) = decoded_background(&path) {
                return (px, Instant::now());
            }
            assert!(Instant::now() < deadline, "{} never appeared", path.display());
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
}

impl Drop for Touchbard {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

/// The bar background of a complete PNG; `None` while the file is missing or partly written.
fn decoded_background(path: &Path) -> Option<[u8; 3]> {
    let mut reader = png::Decoder::new(File::open(path).ok()?).read_info().ok()?;
    let mut buf = vec![0; reader.output_buffer_size()];
    let info = reader.next_frame(&mut buf).ok()?;
    assert_eq!((info.width, info.height), (2170, 60), "{}", path.display());
    let (x, y) = BACKGROUND_AT;
    let i = ((y * info.width + x) * 4) as usize;
    Some([buf[i], buf[i + 1], buf[i + 2]])
}

fn assert_colour(got: [u8; 3], want: [u8; 3], what: &str) {
    assert!(got.iter().zip(want).all(|(g, w)| g.abs_diff(w) <= 2), "{what}: got {got:?}, want {want:?} ± 2");
}

#[test]
fn touchbard_redraws_in_the_new_theme_within_a_second_of_a_theme_switch() {
    let home = omarchy_home("switch", "tokyo-night");
    let mut bar = Touchbard::start(&home, fresh_dir("switch", "sim"));
    let (start, _) = bar.frame(1);
    assert_colour(start, TOKYO_NIGHT_BACKGROUND, "frame-0001 background");

    // `omarchy theme set catppuccin-latte`: stage, remove, rename into place, generation, name.
    let current = current(&home);
    fs::create_dir_all(current.join("next-theme")).unwrap();
    fs::write(current.join("next-theme/colors.toml"), fixture("catppuccin-latte")).unwrap();
    fs::remove_dir_all(current.join("theme")).unwrap();
    let switched = Instant::now();
    fs::rename(current.join("next-theme"), current.join("theme")).unwrap();
    fs::write(current.join("theme.generation"), "2-2\n").unwrap();
    fs::write(current.join("theme.name"), "catppuccin-latte\n").unwrap();

    let (after, shown) = bar.frame(2);
    let took = shown - switched;
    assert!(took <= WITHIN, "frame-0002 came {took:?} after the switch, want ≤ {WITHIN:?}");
    assert_colour(after, CATPPUCCIN_LATTE_BACKGROUND, "frame-0002 background");

    let status = bar.close_stdin();
    assert!(status.success(), "exit {status:?}");
    assert_eq!(bar.frames(), ["frame-0001.png", "frame-0002.png"], "one switch is one new frame");
}

#[test]
fn touchbard_redraws_within_a_second_of_the_theme_set_hook() {
    let home = omarchy_home("hook", "tokyo-night");
    let mut bar = Touchbard::start(&home, fresh_dir("hook", "sim"));
    let (start, _) = bar.frame(1);
    assert_colour(start, TOKYO_NIGHT_BACKGROUND, "frame-0001 background");

    // An edit inside the active theme is not a switch: nothing is drawn until the hook runs.
    fs::write(current(&home).join("theme/colors.toml"), fixture("catppuccin-latte")).unwrap();
    thread::sleep(QUIET);
    assert_eq!(bar.frames(), ["frame-0001.png"], "an in-place edit draws nothing");

    let hooked = Instant::now();
    let run = Command::new("bash").arg(hook()).arg("catppuccin-latte").env("HOME", &home).output().unwrap();
    assert!(
        run.status.success(),
        "hook exit {:?}, stderr: {}",
        run.status,
        String::from_utf8_lossy(&run.stderr)
    );

    let (after, shown) = bar.frame(2);
    let took = shown - hooked;
    assert!(took <= WITHIN, "frame-0002 came {took:?} after the hook, want ≤ {WITHIN:?}");
    assert_colour(after, CATPPUCCIN_LATTE_BACKGROUND, "frame-0002 background");

    let status = bar.close_stdin();
    assert!(status.success(), "exit {status:?}");
    assert_eq!(bar.frames(), ["frame-0001.png", "frame-0002.png"], "one hook call is one new frame");
}
