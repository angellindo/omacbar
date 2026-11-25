//! A running `touchbard` draws a restored design: in one isolated profile (temp `HOME` and
//! `XDG_STATE_HOME`, store outside the config tree) the daemon draws a hand-written profile, a
//! protected save replaces it (keeping a generation of it), and restoring that generation brings
//! the exact file back; the daemon reloads it and presents the restored layout in a new frame.
//! The restore goes through the library `ConfigFile::{look, recover}` that `touchbar-recover`
//! wraps. No device is opened; a frame is observed once its PNG decodes; pixels use ±2 per channel.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::fs::{self, File, Permissions};
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::thread;
use std::time::{Duration, Instant};

use tb_config::{Backups, ConfigFile, GenerationKind, Loaded, Reason, default_path};
use tb_core::config::{Config, to_toml};
use tb_core::{Action, BarTheme, Button, Chord, Group, Layer, Layout};

const WAIT: Duration = Duration::from_secs(60);
const POLL: Duration = Duration::from_millis(10);
const RED: [u8; 3] = [0xe0, 0x30, 0x40];
const GREEN: [u8; 3] = [0x20, 0xc0, 0x50];

fn config(fill: &str, key: &str) -> Config {
    let mut button = Button::new("saved", "saved", Action::Key { keys: vec![key.to_owned()] });
    button.style.fill = Some(fill.to_owned());
    Config {
        layout: Layout {
            layers: vec![Layer {
                group_gap: None,
                chord: Chord::Default,
                groups: vec![Group { buttons: vec![button] }],
            }],
        },
        ..Config::default()
    }
}

struct Bar {
    child: Child,
    sim: PathBuf,
    log: PathBuf,
}

impl Bar {
    fn start(home: &Path, state: &Path, dir: &Path) -> Self {
        let (sim, log) = (dir.join("sim"), dir.join("stderr"));
        let child = Command::new(env!("CARGO_BIN_EXE_touchbard"))
            .arg("--sim")
            .arg(&sim)
            .arg("--emit")
            .arg(dir.join("keys.bin"))
            .env("HOME", home)
            .env("XDG_STATE_HOME", state)
            .stdin(Stdio::piped())
            .stdout(Stdio::null())
            .stderr(File::create(&log).unwrap())
            .spawn()
            .unwrap();
        Self { child, sim, log }
    }

    fn stderr(&self) -> String {
        fs::read_to_string(&self.log).unwrap_or_default()
    }

    fn fill(&self, n: u32, expected: [u8; 3]) {
        let path = self.sim.join(format!("frame-{n:04}.png"));
        let deadline = Instant::now() + WAIT;
        let pixels = loop {
            if let Some(pixels) = decode(&path) {
                break pixels;
            }
            assert!(Instant::now() < deadline, "{} missing; {}", path.display(), self.stderr());
            thread::sleep(POLL);
        };
        let i = (30 * 2170 + 100) * 4;
        let got = &pixels[i..i + 3];
        assert!(
            got.iter().zip(expected).all(|(a, b)| a.abs_diff(b) <= 2),
            "frame {n} pixel (100,30) = {got:?}; expected {expected:?} ±2"
        );
    }

    fn close(&mut self) {
        drop(self.child.stdin.take());
        let deadline = Instant::now() + WAIT;
        loop {
            if let Some(status) = self.child.try_wait().unwrap() {
                assert!(status.success(), "exit {status:?}: {}", self.stderr());
                return;
            }
            assert!(Instant::now() < deadline, "stdin close did not stop the daemon");
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
    assert_eq!((info.width, info.height), (2170, 60));
    Some(pixels)
}

#[test]
fn a_running_daemon_draws_the_restored_design() {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join("touchbard_recovery").join("restored");
    if dir.exists() {
        fs::remove_dir_all(&dir).unwrap();
    }
    let (home, state) = (dir.join("home"), dir.join("state"));
    fs::create_dir_all(&home).unwrap();
    fs::create_dir_all(&state).unwrap();
    let path = default_path(&home);
    let store = state.join("touchbar-customizer/backups");
    let theme = BarTheme::omarchy_fallback();

    let mine =
        format!("# my red bar, by hand\n\n{}\n# keep this\n", to_toml(&config("#e03040", "F9")).unwrap());
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(&path, &mine).unwrap();
    fs::set_permissions(&path, Permissions::from_mode(0o640)).unwrap();

    let mut bar = Bar::start(&home, &state, &dir);
    bar.fill(1, RED);

    let mut editor = ConfigFile::protected(&path, Backups::at(&store));
    assert_eq!(editor.load(&theme).unwrap(), Loaded::Read);
    editor.save(config("#20c050", "F10"), &theme).unwrap();
    bar.fill(2, GREEN);
    let kept = Backups::at(&store).list(&path).unwrap()[0].clone();
    assert_eq!(kept.kind, GenerationKind::File);

    let mut recovery = ConfigFile::protected(&path, Backups::at(&store));
    recovery.look().unwrap();
    recovery.recover(&kept.id).unwrap();
    assert_eq!(fs::read_to_string(&path).unwrap(), mine, "the hand-written file is back, byte for byte");
    assert_eq!(fs::metadata(&path).unwrap().permissions().mode() & 0o777, 0o640);
    bar.fill(3, RED);
    assert_eq!(Backups::at(&store).list(&path).unwrap()[0].reason, Reason::Recover);
    bar.close();
}
