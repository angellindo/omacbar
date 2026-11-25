//! `touchbard`'s opt-in device run (`--drm`, `--touch`) through its entry path, `run_hardware`,
//! with the same `Daemon`, `DrmDisplay`, `EvdevTouches` and actions the binary uses, and fakes only
//! at the device boundary: the card opener hands out a `RecordingKms` as an opened node, the
//! digitizer opener hands out `EvdevTouches` over synthetic 24-byte `input_event` records with a
//! synthetic verified profile. Card and digitizer are chosen from a temp sysfs tree and a temp
//! `/proc/bus/input/devices` text. Every run gets a private HOME with its own config; `--emit`
//! writes key records to a file there, and the one command action is a harmless `grep` of the
//! command's own status. No test opens `/dev`, reads the host's `/sys` or input devices, or
//! changes the host.
//!
//! Stop signals go only to a child process this file starts: the test binary itself, re-run on
//! the one `#[ignore]`d test. The child is started with the stop signals blocked (GNU `env
//! --block-signal`), because libtest's own threads never call `block_stop_signals` and would
//! otherwise take a process signal by its default action. The child reports on its stdout; the
//! parent reads it with deadlines, so a broken run fails instead of hanging.
//!
//! Nothing here is evidence about the T1 panel or digitizer: a successful KMS call on the
//! recording card is not a lit panel, and a restore is not a physical return to the function keys
//! (TB-10, TB-45). IDs: the T1 Touch Bar `05ac:8600` (research 0001, 0010). Event codes:
//! `linux/input-event-codes.h`; errno: `asm-generic/errno-base.h`; signal numbers and `SigBlk`
//! bit order: `asm-generic/signal.h`, proc(5) (bit n−1 is signal n).
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::print_stdout)]
// The digitizer metadata literal ends in `..Default::default()` so that a field the implementer
// adds does not break this file; with exactly the pinned fields that update is needless.
#![allow(clippy::needless_update)]

use super::*;

use std::fs;
use std::io::{self, BufRead, BufReader, Cursor, Read};
use std::os::unix::fs::{DirBuilderExt, symlink};
use std::process::{Child, ChildStdout, Command, Stdio};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::mpsc::{Receiver, RecvTimeoutError};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use tb_core::config::{Config, to_toml};
use tb_core::{Action, Button, Chord, Group, Layer, Layout, SpacingRules, place};
use tb_daemon::fake::{FlatRenderer, KmsCall, KmsLog, KmsStep, PriorCrtc, RecordingKms};
use tb_daemon::{
    AbsAxis, DigitizerMeta, DigitizerProfile, DirtyClip, DumbBuffer, EvdevTouches, FbSpec, Kms, KmsMetadata,
    ModeInfo, NodeId, StopSignals, block_stop_signals, calibrate,
};

const DRM_MAJOR: u32 = 226;
const T1_NODE: NodeId = NodeId { major: DRM_MAJOR, minor: 1, char_device: true };
const T1_ID: &str = "Bus=0003 Vendor=05ac Product=8600";
/// `ABS_MT_SLOT`, `ABS_MT_POSITION_X/Y`, `ABS_MT_TRACKING_ID`, plus `ABS_X/Y`.
const MT_ABS: &str = "260800000000003";
const EIO: i32 = 5;
const EBUSY: i32 = 16;
const ENODEV: i32 = 19;
const KEY_F5: u16 = 63;
const EV_KEY: u16 = 1;
/// How long a child process may take to reach a point; generous, since a failing run must fail,
/// not hang, and a passing one takes well under a second.
const WAIT: Duration = Duration::from_secs(60);
/// How long a stop signal may take to end the run.
const STOP_LIMIT: Duration = Duration::from_secs(10);

/// A new directory under the temp dir that only this user can read.
fn private_dir(name: &str) -> PathBuf {
    static RUNS: AtomicUsize = AtomicUsize::new(0);
    let n = RUNS.fetch_add(1, Ordering::Relaxed);
    let dir = std::env::temp_dir().join(format!("touchbard-{name}-{}-{n}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::DirBuilder::new().recursive(true).mode(0o700).create(&dir).unwrap();
    dir
}

// ---------- a private bench: HOME, sysfs tree, input devices text, key sink ----------

struct Bench {
    root: PathBuf,
    /// Remove `root` when dropped (not for a child's dir, which its parent owns).
    owned: bool,
    home: PathBuf,
    sim: PathBuf,
    sysfs: PathBuf,
    devices: PathBuf,
    keys: PathBuf,
    mask: PathBuf,
}

impl Bench {
    /// A laptop: an Intel GPU as card0, the T1 bar as card1, and one T1 digitizer at event7
    /// next to the T1 keyboard (same ids, no multitouch axes) at event15.
    fn new(name: &str) -> Self {
        let mut bench = Self::bare(&private_dir(name));
        bench.owned = true;
        bench.laptop();
        bench
    }

    /// Only HOME and its config: no sysfs tree and no input devices text exist.
    fn bare(root: &Path) -> Self {
        let b = Self {
            root: root.to_owned(),
            owned: false,
            home: root.join("home"),
            sim: root.join("sim"),
            sysfs: root.join("sys"),
            devices: root.join("input-devices"),
            keys: root.join("keys.bin"),
            mask: root.join("mask"),
        };
        let path = tb_config::default_path(&b.home);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, to_toml(&Config { layout: layout(&b.mask), ..Config::default() }).unwrap()).unwrap();
        b
    }

    fn laptop(&self) {
        fs::create_dir_all(self.sysfs.join("class/drm")).unwrap();
        self.pci_card("card0", "0000:00:02.0", "i915");
        self.usb_card("card1", "1-3", "appletbdrm", "8600");
        self.input_devices(&[keyboard("event15"), digitizer("event7")]);
    }

    fn input_devices(&self, blocks: &[String]) {
        fs::write(&self.devices, blocks.concat()).unwrap();
    }

    fn write(&self, rel: &str, text: &str) {
        let p = self.sysfs.join(rel);
        fs::create_dir_all(p.parent().unwrap()).unwrap();
        fs::write(p, text).unwrap();
    }

    fn link(&self, rel: &str, target: &str) {
        let p = self.sysfs.join(rel);
        fs::create_dir_all(p.parent().unwrap()).unwrap();
        let _ = fs::remove_file(&p);
        symlink(target, p).unwrap();
    }

    /// A DRM card under an Apple USB interface bound to `driver`.
    fn usb_card(&self, card: &str, usb: &str, driver: &str, product: &str) {
        let iface = format!("devices/usb1/{usb}/{usb}:1.2");
        self.write(&format!("devices/usb1/{usb}/idVendor"), "05ac\n");
        self.write(&format!("devices/usb1/{usb}/idProduct"), &format!("{product}\n"));
        fs::create_dir_all(self.sysfs.join(format!("bus/usb/drivers/{driver}"))).unwrap();
        self.link(&format!("{iface}/driver"), &format!("../../../../bus/usb/drivers/{driver}"));
        self.card_dir(card, &iface, &format!("../../../{usb}:1.2"));
    }

    /// A DRM card under a PCI GPU bound to `driver`.
    fn pci_card(&self, card: &str, slot: &str, driver: &str) {
        let dev = format!("devices/pci0000:00/{slot}");
        fs::create_dir_all(self.sysfs.join(format!("bus/pci/drivers/{driver}"))).unwrap();
        self.link(&format!("{dev}/driver"), &format!("../../../bus/pci/drivers/{driver}"));
        self.card_dir(card, &dev, &format!("../../../{slot}"));
    }

    fn card_dir(&self, card: &str, parent: &str, device_link: &str) {
        let minor: u32 = card.trim_start_matches("card").parse().unwrap();
        let dir = format!("{parent}/drm/{card}");
        self.write(&format!("{dir}/dev"), &format!("{DRM_MAJOR}:{minor}\n"));
        self.link(&format!("{dir}/device"), device_link);
        self.link(&format!("class/drm/{card}"), &format!("../../{dir}"));
    }

    /// How often F5 was pressed on the `--emit` key sink.
    fn f5_presses(&self) -> usize {
        let bytes = fs::read(&self.keys).unwrap_or_default();
        bytes
            .as_chunks::<24>()
            .0
            .iter()
            .filter(|r| {
                u16::from_ne_bytes([r[16], r[17]]) == EV_KEY
                    && u16::from_ne_bytes([r[18], r[19]]) == KEY_F5
                    && i32::from_ne_bytes([r[20], r[21], r[22], r[23]]) == 1
            })
            .count()
    }

    fn frames(&self) -> usize {
        fs::read_dir(&self.sim).map_or(0, |d| d.filter_map(Result::ok).count())
    }
}

impl Drop for Bench {
    fn drop(&mut self) {
        if self.owned {
            let _ = fs::remove_dir_all(&self.root);
        }
    }
}

/// F5 presses a key; `mask` runs a command that writes its own blocked-signal mask to `mask`.
fn layout(mask: &Path) -> Layout {
    let key = Button::new("f5", "F5", Action::Key { keys: vec!["F5".to_owned()] });
    let run = format!("exec grep SigBlk /proc/self/status > '{}'", mask.display());
    let command = Button::new("mask", "mask", Action::Command { run });
    Layout {
        layers: vec![Layer {
            group_gap: None,
            chord: Chord::Default,
            groups: vec![Group { buttons: vec![key, command] }],
        }],
    }
}

fn x_of(id: &str) -> u16 {
    let layout = layout(Path::new("/nonexistent"));
    let placed = place(&layout.layers[0], BarSize::MACBOOK_PRO, &SpacingRules::STANDARD).unwrap();
    let rect = placed.iter().find(|p| p.id.as_str() == id).unwrap().rect;
    rect.x + rect.width / 2
}

fn device_block(id: &str, name: &str, handlers: &str, abs: Option<&str>) -> String {
    let caps = match abs {
        Some(abs) => format!("B: EV=1b\nB: KEY=400 0 0 0 0 0\nB: ABS={abs}\nB: MSC=20\n"),
        None => "B: EV=120013\nB: KEY=1006300001000 3800000000 10e00000180001f f800000000000002\n".to_owned(),
    };
    format!("I: {id} Version=0110\nN: Name=\"{name}\"\nH: Handlers={handlers} \nB: PROP=0\n{caps}\n")
}

fn digitizer(event: &str) -> String {
    device_block(T1_ID, DIGITIZER, event, Some(MT_ABS))
}

fn keyboard(event: &str) -> String {
    device_block(T1_ID, "Synthetic keyboard", &format!("sysrq kbd leds {event}"), None)
}

// ---------- synthetic digitizer reports (MT protocol B) ----------

/// A synthetic name and range: no real device reports them. One raw unit per bar column.
const DIGITIZER: &str = "Synthetic digitizer";
const EV_SYN: u16 = 0;
const EV_ABS: u16 = 3;
const SYN_REPORT: u16 = 0;
const SYN_DROPPED: u16 = 3;
const ABS_MT_SLOT: u16 = 0x2f;
const ABS_MT_POSITION_X: u16 = 0x35;
const ABS_MT_POSITION_Y: u16 = 0x36;
const ABS_MT_TRACKING_ID: u16 = 0x39;
const X_AXIS: AbsAxis = AbsAxis { code: ABS_MT_POSITION_X, min: 0, max: 2169 };

/// The calibration a verified profile for the synthetic digitizer gives.
fn calibration() -> tb_daemon::Calibration {
    let meta = DigitizerMeta {
        bus: 0x0003,
        vendor: 0x05ac,
        product: 0x8600,
        name: DIGITIZER.to_owned(),
        axes: vec![
            AbsAxis { code: ABS_MT_SLOT, min: 0, max: 9 },
            AbsAxis { code: ABS_MT_TRACKING_ID, min: 0, max: 65535 },
            X_AXIS,
            AbsAxis { code: ABS_MT_POSITION_Y, min: 0, max: 59 },
        ],
        ..Default::default()
    };
    let profile = DigitizerProfile {
        bus: 0x0003,
        vendor: 0x05ac,
        product: 0x8600,
        name: DIGITIZER,
        axis: X_AXIS,
        reversed: false,
    };
    calibrate(&meta, &[profile], BarSize::MACBOOK_PRO.width)
        .unwrap_or_else(|e| panic!("precondition: the synthetic profile matches: {e}"))
}

/// One `x86_64` `struct input_event`: 16 bytes of time, type, code, value.
fn record(kind: u16, code: u16, value: i32) -> Vec<u8> {
    let mut r = vec![0u8; 16];
    r.extend(kind.to_ne_bytes());
    r.extend(code.to_ne_bytes());
    r.extend(value.to_ne_bytes());
    r
}

fn report() -> Vec<u8> {
    record(EV_SYN, SYN_REPORT, 0)
}

/// A contact lands in slot 0 at bar column `x`, in one report.
fn land(x: u16) -> Vec<u8> {
    [
        record(EV_ABS, ABS_MT_SLOT, 0),
        record(EV_ABS, ABS_MT_TRACKING_ID, 1),
        record(EV_ABS, ABS_MT_POSITION_X, i32::from(x)),
        report(),
    ]
    .concat()
}

fn lift() -> Vec<u8> {
    [record(EV_ABS, ABS_MT_TRACKING_ID, -1), report()].concat()
}

fn tap(id: &str) -> Vec<u8> {
    [land(x_of(id)), lift()].concat()
}

/// What a read returns once the records are used up.
enum End {
    /// End of file: the digitizer closed.
    Closed,
    /// This errno on every later read.
    Fails(i32),
    /// Blocks until the sender is dropped, then ends: a reader nobody can wake.
    Blocks(Receiver<()>),
}

/// The byte stream a synthetic digitizer node gives.
struct Feed {
    records: Cursor<Vec<u8>>,
    end: End,
}

impl Read for Feed {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        match self.records.read(buf)? {
            0 => match &self.end {
                End::Closed => Ok(0),
                End::Fails(errno) => Err(io::Error::from_raw_os_error(*errno)),
                End::Blocks(held) => {
                    let _ = held.recv();
                    Ok(0)
                }
            },
            n => Ok(n),
        }
    }
}

type Touches = EvdevTouches<Feed>;

/// What the digitizer opener hands out: always opened here; refusals are written as `Err` inline.
#[allow(clippy::unnecessary_wraps)]
fn feed(records: Vec<u8>, end: End) -> Result<Touches, String> {
    Ok(EvdevTouches::new(Feed { records: Cursor::new(records), end }, calibration()))
}

fn closed(records: Vec<u8>) -> Result<Touches, String> {
    feed(records, End::Closed)
}

// ---------- running the entry path ----------

struct Outcome {
    result: Result<(), String>,
    /// Paths the card opener was asked for, in order.
    cards: Vec<PathBuf>,
    /// Paths the digitizer opener was asked for, in order.
    digitizers: Vec<PathBuf>,
    calls: Vec<KmsCall>,
    /// The recording card's whole buffer when the run ended.
    pixels: Vec<u8>,
}

impl Outcome {
    fn changes(&self) -> Vec<&'static str> {
        changes(&self.calls)
    }

    /// Restores of a shown frame: the CRTC disabled again (fb 0, no connector, no mode).
    fn restores(&self) -> usize {
        let disable = KmsCall::SetCrtc { crtc: RecordingKms::CRTC, fb: 0, connectors: vec![], mode: None };
        self.calls.iter().filter(|c| **c == disable).count()
    }

    fn modesets(&self) -> usize {
        self.calls.iter().filter(|c| matches!(c, KmsCall::SetCrtc { fb, .. } if *fb != 0)).count()
    }

    fn error(&self) -> String {
        self.result.clone().expect_err("the run must fail")
    }
}

/// The calls that change the card or its ownership, as short names; reads are left out.
fn changes(calls: &[KmsCall]) -> Vec<&'static str> {
    calls
        .iter()
        .filter_map(|c| match c {
            KmsCall::SetMaster => Some("set_master"),
            KmsCall::DropMaster => Some("drop_master"),
            KmsCall::CreateDumb { .. } => Some("create_dumb"),
            KmsCall::MapDumb => Some("map_dumb"),
            KmsCall::Unmap => Some("unmap"),
            KmsCall::DestroyDumb => Some("destroy_dumb"),
            KmsCall::AddFb2 { .. } => Some("add_fb2"),
            KmsCall::SetCrtc { .. } => Some("set_crtc"),
            KmsCall::Dirty { .. } => Some("dirty"),
            KmsCall::RmFb { .. } => Some("rm_fb"),
            KmsCall::Close => Some("close"),
            _ => None,
        })
        .collect()
}

const STARTED: [&str; 4] = ["set_master", "create_dumb", "map_dumb", "add_fb2"];
const RELEASED: [&str; 5] = ["rm_fb", "unmap", "destroy_dumb", "drop_master", "close"];
const RESTORED: [&str; 6] = ["set_crtc", "rm_fb", "unmap", "destroy_dumb", "drop_master", "close"];

fn started_then_released() -> Vec<&'static str> {
    [&STARTED[..], &RELEASED[..]].concat()
}

fn tail<'a>(changes: &'a [&'static str], n: usize) -> &'a [&'static str] {
    &changes[changes.len().saturating_sub(n)..]
}

fn run(
    bench: &Bench,
    flags: &[&str],
    kms: RecordingKms,
    node: NodeId,
    touches: Result<Touches, String>,
) -> Outcome {
    let log = kms.log();
    run_with(bench, flags, kms, &log, node, touches, None)
}

/// `run_hardware` with `flags` plus `--emit <key sink>`, and openers that record what they were
/// asked for and hand out `kms` (as the node `node`) and `touches`.
fn run_with<K: Kms + 'static>(
    bench: &Bench,
    flags: &[&str],
    kms: K,
    log: &KmsLog,
    node: NodeId,
    touches: Result<Touches, String>,
    stop: Option<StopSignals>,
) -> Outcome {
    let (cards, digitizers) = (Arc::new(Mutex::new(Vec::new())), Arc::new(Mutex::new(Vec::new())));
    let (card_log, digitizer_log) = (Arc::clone(&cards), Arc::clone(&digitizers));
    let devices = Devices {
        sysfs: bench.sysfs.clone(),
        input_devices: bench.devices.clone(),
        open_card: Box::new(move |path: &Path| {
            card_log.lock().unwrap().push(path.to_owned());
            Ok((kms, node))
        }),
        open_touch: Box::new(move |path: &Path| {
            digitizer_log.lock().unwrap().push(path.to_owned());
            touches
        }),
    };
    let keys = bench.keys.to_str().unwrap();
    let all = [flags, &["--emit", keys]].concat();
    let args =
        parse_args(all.iter().map(|a| (*a).to_owned())).unwrap_or_else(|| panic!("{all:?} must parse"));
    let result = run_hardware(args, Some(&bench.home), FlatRenderer, devices, stop);
    let taken = |v: &Arc<Mutex<Vec<PathBuf>>>| v.lock().unwrap().clone();
    Outcome {
        result,
        cards: taken(&cards),
        digitizers: taken(&digitizers),
        calls: log.calls(),
        pixels: log.pixels(),
    }
}

/// Changes the bench before a run.
type Arrange = fn(&Bench);

const BOTH: [&str; 4] = ["--drm", "/dev/dri/card1", "--touch", "/dev/input/event7"];

// ---------- a path that is no device node: refused before anything is read ----------

#[test]
fn a_path_that_is_not_a_device_node_is_refused_before_sysfs_or_the_input_devices_are_read() {
    let root = private_dir("lexical");
    let outside = root.join("dri/card1");
    let outside_event = root.join("input/event7");
    let rows: [(&str, Vec<&str>, &Path); 4] = [
        (
            "a card outside /dev/dri",
            vec!["--drm", outside.to_str().unwrap(), "--touch", "/dev/input/event7"],
            outside.as_path(),
        ),
        (
            "a card path through ..",
            vec!["--drm", "/dev/dri/../dri/card1", "--touch", "/dev/input/event7"],
            Path::new("/dev/dri/../dri/card1"),
        ),
        (
            "an event node outside /dev/input",
            vec!["--touch", outside_event.to_str().unwrap()],
            outside_event.as_path(),
        ),
        (
            "an event path through ..",
            vec!["--touch", "/dev/input/../input/event7"],
            Path::new("/dev/input/../input/event7"),
        ),
    ];
    for (i, (label, mut flags, path)) in rows.into_iter().enumerate() {
        // Neither the sysfs tree nor the input devices text exists: reading either would fail
        // with an error that names it instead of the refused path.
        let bench = Bench::bare(&root.join(format!("bench-{i}")));
        let sim = bench.sim.to_str().unwrap().to_owned();
        if flags[0] == "--touch" {
            flags.extend(["--sim", &sim]);
        }
        let out = run(&bench, &flags, RecordingKms::touch_bar(), T1_NODE, closed(tap("f5")));
        let error = out.error();
        assert!(error.contains(path.to_str().unwrap()), "{label}: names the path: {error}");
        assert!(!error.contains(bench.sysfs.to_str().unwrap()), "{label}: no sysfs read: {error}");
        assert!(!error.contains(bench.devices.to_str().unwrap()), "{label}: no input devices read: {error}");
        assert_eq!(out.cards, Vec::<PathBuf>::new(), "{label}: no card opened");
        assert_eq!(out.digitizers, Vec::<PathBuf>::new(), "{label}: no digitizer opened");
        assert_eq!(bench.frames(), 0, "{label}: nothing shown");
        assert_eq!(bench.f5_presses(), 0, "{label}: no action");
        let _ = fs::remove_dir_all(&bench.root);
    }
    let _ = fs::remove_dir_all(&root);
}

// ---------- identity before opening, opened identity before taking master ----------

#[test]
fn the_display_card_is_identified_in_sysfs_before_any_node_is_opened() {
    let rows: [(&str, &str, Arrange); 4] = [
        ("an explicit desktop GPU", "/dev/dri/card0", |_| {}),
        ("an explicit T2 bar", "/dev/dri/card2", |b| b.usb_card("card2", "1-4", "appletbdrm", "8302")),
        ("auto with no Touch Bar card", "auto", |b| {
            fs::remove_file(b.sysfs.join("class/drm/card1")).unwrap();
        }),
        ("auto with two Touch Bar cards", "auto", |b| b.usb_card("card2", "1-4", "appletbdrm", "8600")),
    ];
    for (label, card, arrange) in rows {
        let bench = Bench::new("identify");
        arrange(&bench);
        let flags = ["--drm", card, "--touch", "/dev/input/event7"];
        let out = run(&bench, &flags, RecordingKms::touch_bar(), T1_NODE, closed(tap("f5")));
        assert!(!out.error().is_empty(), "{label}: a reason");
        assert_eq!(out.cards, Vec::<PathBuf>::new(), "{label}: no card node is opened");
        assert_eq!(out.digitizers, Vec::<PathBuf>::new(), "{label}: no digitizer is opened");
        assert_eq!(bench.f5_presses(), 0, "{label}: no action");
    }

    // The positive control: the identified card is the one opened, auto or explicit.
    for card in ["auto", "/dev/dri/card1"] {
        let bench = Bench::new("identify-ok");
        let flags = ["--drm", card, "--touch", "/dev/input/event7"];
        let out = run(&bench, &flags, RecordingKms::touch_bar(), T1_NODE, closed(Vec::new()));
        assert_eq!(out.result, Ok(()), "--drm {card}");
        assert_eq!(out.cards, [PathBuf::from("/dev/dri/card1")], "--drm {card}: the Touch Bar card");
    }
}

#[test]
fn an_opened_node_that_is_not_the_identified_card_is_closed_before_master_is_taken() {
    let rows = [
        ("another minor", NodeId { minor: 0, ..T1_NODE }),
        ("another major", NodeId { major: 13, ..T1_NODE }),
        ("not a character device", NodeId { char_device: false, ..T1_NODE }),
    ];
    for (label, node) in rows {
        let bench = Bench::new("node");
        let out = run(&bench, &BOTH, RecordingKms::touch_bar(), node, closed(tap("f5")));
        assert!(!out.error().is_empty(), "{label}: a reason");
        assert_eq!(out.cards, [PathBuf::from("/dev/dri/card1")], "{label}: opened once");
        assert_eq!(out.changes(), ["close"], "{label}: closed with nothing taken or changed");
        assert_eq!(out.digitizers, Vec::<PathBuf>::new(), "{label}: no digitizer is opened");
    }
}

// ---------- startup failures change nothing visible ----------

#[test]
fn a_refused_display_start_shows_nothing_and_opens_no_digitizer() {
    let foreign = PriorCrtc { fb: 41, mode: Some((60, 2170)) };
    let rows: [(&str, RecordingKms, &[&str]); 3] = [
        (
            "master busy",
            RecordingKms::touch_bar().fail_at(KmsStep::SetMaster, EBUSY),
            &["set_master", "close"],
        ),
        (
            "a foreign frame shown",
            RecordingKms::touch_bar().with_prior_crtc(foreign),
            &["set_master", "drop_master", "close"],
        ),
        (
            "another orientation",
            RecordingKms::touch_bar().with_orientation(Some("Normal")),
            &["set_master", "drop_master", "close"],
        ),
    ];
    for (label, kms, expected) in rows {
        let bench = Bench::new("refused");
        let out = run(&bench, &BOTH, kms, T1_NODE, closed(tap("f5")));
        assert!(!out.error().is_empty(), "{label}: a reason");
        assert_eq!(out.changes(), expected, "{label}: no SETCRTC, nothing shown");
        assert_eq!(out.digitizers, Vec::<PathBuf>::new(), "{label}: the digitizer is not opened");
        assert_eq!(bench.f5_presses(), 0, "{label}: no action");
    }
}

#[test]
fn a_digitizer_that_cannot_be_chosen_or_opened_leaves_the_card_unshown_and_released() {
    let rows: [(&str, &str, Arrange, bool); 4] = [
        ("the opener refuses", "/dev/input/event7", |_| {}, true),
        ("an explicit node that is no candidate", "/dev/input/event9", |_| {}, false),
        ("an explicit keyboard node with the same ids", "/dev/input/event15", |_| {}, false),
        (
            "auto with two candidates",
            "auto",
            |b| b.input_devices(&[digitizer("event7"), digitizer("event8")]),
            false,
        ),
    ];
    for (label, node, arrange, opened) in rows {
        let bench = Bench::new("digitizer");
        arrange(&bench);
        let refusal = Err("synthetic: no verified digitizer".to_owned());
        let flags = ["--drm", "/dev/dri/card1", "--touch", node];
        let out = run(&bench, &flags, RecordingKms::touch_bar(), T1_NODE, refusal);
        let error = out.error();
        // The card may be started before the digitizer is chosen or after; it is never shown.
        let expected = if out.cards.is_empty() { vec!["close"] } else { started_then_released() };
        assert_eq!(out.changes(), expected, "{label}: released in reverse, no SETCRTC");
        assert_eq!(out.modesets(), 0, "{label}: nothing shown");
        let asked: Vec<PathBuf> = if opened { vec![PathBuf::from(node)] } else { vec![] };
        assert_eq!(out.digitizers, asked, "{label}: only a chosen candidate is opened");
        if opened {
            assert!(error.contains("synthetic: no verified digitizer"), "{label}: names the reason: {error}");
        }
    }

    // The positive control: auto opens the one candidate's node.
    let bench = Bench::new("digitizer-ok");
    let flags = ["--drm", "/dev/dri/card1", "--touch", "auto"];
    let out = run(&bench, &flags, RecordingKms::touch_bar(), T1_NODE, closed(Vec::new()));
    assert_eq!(out.result, Ok(()));
    assert_eq!(out.digitizers, [PathBuf::from("/dev/input/event7")]);
}

// ---------- every exit restores once ----------

#[test]
fn the_end_of_the_touches_runs_each_tap_once_and_restores_once() {
    let bench = Bench::new("ended");
    let out = run(&bench, &BOTH, RecordingKms::touch_bar(), T1_NODE, closed(tap("f5")));
    assert_eq!(out.result, Ok(()));
    assert_eq!(bench.f5_presses(), 1, "the tap pressed F5 once");
    let changes = out.changes();
    assert_eq!(changes[..5], ["set_master", "create_dumb", "map_dumb", "add_fb2", "set_crtc"]);
    assert_eq!(tail(&changes, 6), RESTORED);
    assert_eq!(out.modesets(), 1, "one modeset");
    assert_eq!(out.restores(), 1, "one restore");
    assert!(changes.contains(&"dirty"), "press and release were presented as damage");
}

#[test]
fn a_cancelled_touch_runs_no_action_leaves_nothing_pressed_and_restores_once() {
    let idle = run(&Bench::new("idle"), &BOTH, RecordingKms::touch_bar(), T1_NODE, closed(Vec::new()));
    assert_eq!(idle.result, Ok(()));
    let x = x_of("f5");
    let rows = [
        (
            "lost sync, then the stale contact lifts",
            [land(x), record(EV_SYN, SYN_DROPPED, 0), report(), lift()].concat(),
        ),
        (
            "a position outside the verified range",
            [land(x), record(EV_ABS, ABS_MT_POSITION_X, X_AXIS.max + 1), report(), lift()].concat(),
        ),
    ];
    for (label, records) in rows {
        let bench = Bench::new("cancelled");
        let out = run(&bench, &BOTH, RecordingKms::touch_bar(), T1_NODE, closed(records));
        assert_eq!(out.result, Ok(()), "{label}");
        assert_eq!(bench.f5_presses(), 0, "{label}: no action");
        assert!(out.changes().contains(&"dirty"), "{label}: the press was shown");
        assert!(out.pixels == idle.pixels, "{label}: the bar ends as it started, nothing pressed");
        assert_eq!(out.restores(), 1, "{label}: one restore");
        assert_eq!(tail(&out.changes(), 6), RESTORED, "{label}");
    }
}

#[test]
fn a_touch_read_failure_or_a_daemon_error_restores_once_and_is_reported() {
    let rows = [
        ("a touch read error", RecordingKms::touch_bar(), feed(land(x_of("f5")), End::Fails(EIO))),
        ("a present that fails", RecordingKms::touch_bar().fail_at(KmsStep::Dirty, EIO), closed(tap("f5"))),
    ];
    for (label, kms, touches) in rows {
        let bench = Bench::new("failed");
        let out = run(&bench, &BOTH, kms, T1_NODE, touches);
        assert!(!out.error().is_empty(), "{label}: reported");
        assert_eq!(out.restores(), 1, "{label}: restored once");
        assert_eq!(tail(&out.changes(), 6), RESTORED, "{label}: released after the restore");
        assert_eq!(bench.f5_presses(), 0, "{label}: no action");
    }
}

#[test]
fn an_unplugged_bar_runs_no_action_restores_nothing_on_the_gone_card_and_releases_the_rest() {
    let bench = Bench::new("unplugged");
    let touches = feed(land(x_of("f5")), End::Fails(ENODEV));
    let kms = RecordingKms::touch_bar().fail_at(KmsStep::Dirty, ENODEV);
    let out = run(&bench, &BOTH, kms, T1_NODE, touches);
    let error = out.error();
    assert!(error.to_lowercase().contains("gone"), "reports the card as gone: {error}");
    assert_eq!(out.restores(), 0, "nothing to restore on a gone card");
    assert_eq!(out.modesets(), 1, "the first modeset only");
    assert_eq!(tail(&out.changes(), 5), RELEASED, "buffer, mapping, master and the card released");
    assert_eq!(bench.f5_presses(), 0, "no action");
}

#[test]
fn a_failed_restore_still_attempts_every_release_step_and_is_reported() {
    let kms = RecordingKms::touch_bar().fail_at(KmsStep::RmFb, EIO);
    let out = run(&Bench::new("restore"), &BOTH, kms, T1_NODE, closed(Vec::new()));
    let error = out.error();
    assert!(error.to_lowercase().contains("restor"), "names the restore: {error}");
    assert_eq!(out.restores(), 1);
    assert_eq!(tail(&out.changes(), 6), RESTORED);
}

#[test]
fn touch_without_drm_drives_the_simulator_and_never_opens_a_card() {
    let bench = Bench::new("touch-only");
    let sim = bench.sim.to_str().unwrap().to_owned();
    let flags = ["--touch", "/dev/input/event7", "--sim", &sim];
    let out = run(&bench, &flags, RecordingKms::touch_bar(), T1_NODE, closed(tap("f5")));
    assert_eq!(out.result, Ok(()));
    assert_eq!(out.cards, Vec::<PathBuf>::new(), "no card is opened");
    assert_eq!(out.calls, [KmsCall::Close], "the unused card fake is only dropped");
    assert_eq!(out.digitizers, [PathBuf::from("/dev/input/event7")]);
    assert_eq!(bench.f5_presses(), 1);
    for n in 1..=3 {
        assert!(bench.sim.join(format!("frame-{n:04}.png")).is_file(), "frame {n}: start, press, release");
    }
}

// ---------- stop signals, only in a child process this test owns ----------

const CHILD: &str = "TOUCHBARD_HARDWARE_CHILD";
const CHILD_TEST: &str = "tests::hardware::hardware_run_in_a_child_process";
const READY: &str = "touchbard-child: ready";
const DONE: &str = "touchbard-child: done";
const MASKS: &str = "touchbard-child: masks";
const SIGHUP_BIT: u64 = 1;
const SIGINT_BIT: u64 = 1 << 1;
const SIGUSR1_BIT: u64 = 1 << 9;
const SIGTERM_BIT: u64 = 1 << 14;
const STOP_SIGNALS: [&str; 3] = ["HUP", "INT", "TERM"];

/// The blocked-signal mask in a `/proc/.../status` text.
fn sig_blk(status: &str) -> u64 {
    let line = status.lines().find_map(|l| l.strip_prefix("SigBlk:")).expect("a SigBlk line");
    u64::from_str_radix(line.trim(), 16).unwrap()
}

fn thread_mask() -> u64 {
    sig_blk(&fs::read_to_string("/proc/thread-self/status").unwrap())
}

/// A child process this test owns: the test binary on the child test only, with `signals`
/// blocked from its start. Its stdout lines arrive on `lines`; the channel closes when it exits.
struct Owned {
    child: Child,
    lines: Receiver<String>,
    seen: Vec<String>,
    dir: PathBuf,
}

fn spawn_child(mode: &str, dir: &Path, signals: &[&str]) -> Owned {
    let mut command = Command::new("env");
    for signal in signals {
        command.arg(format!("--block-signal={signal}"));
    }
    let mut child = command
        .arg(std::env::current_exe().unwrap())
        .args([CHILD_TEST, "--exact", "--include-ignored", "--nocapture", "--test-threads=1"])
        .env(CHILD, format!("{mode}:{}", dir.display()))
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(fs::File::create(dir.join("stderr")).unwrap())
        .spawn()
        .unwrap();
    let stdout: ChildStdout = child.stdout.take().unwrap();
    let (tx, lines) = mpsc::channel();
    thread::spawn(move || {
        for line in BufReader::new(stdout).lines() {
            let Ok(line) = line else { break };
            if tx.send(line).is_err() {
                break;
            }
        }
    });
    Owned { child, lines, seen: Vec::new(), dir: dir.to_owned() }
}

impl Owned {
    /// Reads lines until one contains `marker` (libtest may print the test name before it on the
    /// same line); `false` when the child closed its stdout first.
    /// Kills the child (it is ours) and fails after `limit`.
    fn wait_for(&mut self, marker: &str, limit: Duration) -> bool {
        let deadline = Instant::now() + limit;
        loop {
            match self.lines.recv_timeout(deadline.saturating_duration_since(Instant::now())) {
                Ok(line) => {
                    let found = line.contains(marker);
                    self.seen.push(line);
                    if found {
                        return true;
                    }
                }
                Err(RecvTimeoutError::Disconnected) => return false,
                Err(RecvTimeoutError::Timeout) => self.fail(&format!("no `{marker}` within {limit:?}")),
            }
        }
    }

    /// Waits for the child to end within `limit` and to pass; its stdout lines.
    fn finish(mut self, limit: Duration) -> Vec<String> {
        let deadline = Instant::now() + limit;
        loop {
            match self.lines.recv_timeout(deadline.saturating_duration_since(Instant::now())) {
                Ok(line) => self.seen.push(line),
                Err(RecvTimeoutError::Disconnected) => break,
                Err(RecvTimeoutError::Timeout) => {
                    self.fail(&format!("the child did not end within {limit:?}"))
                }
            }
        }
        let status = self.child.wait().unwrap();
        let report = self.report();
        assert!(status.success(), "child {status:?}: {report}");
        assert!(self.seen.iter().any(|l| l.contains(DONE)), "the child test ran to its end: {report}");
        let _ = fs::remove_dir_all(&self.dir);
        self.seen
    }

    fn fail(&mut self, why: &str) -> ! {
        let _ = self.child.kill();
        let _ = self.child.wait();
        panic!("{why}: {}", self.report());
    }

    fn report(&self) -> String {
        let stderr = fs::read_to_string(self.dir.join("stderr")).unwrap_or_default();
        format!("stdout:\n{}\nstderr:\n{stderr}", self.seen.join("\n"))
    }

    /// Sends `signal` to this child's pid only. The child is not yet waited for, so its pid
    /// cannot belong to another process.
    fn signal(&mut self, signal: &str) {
        let pid = self.child.id().to_string();
        let sent =
            Command::new("sh").args(["-c", "kill -s \"$1\" \"$2\"", "sh", signal, &pid]).status().unwrap();
        if !sent.success() {
            self.fail(&format!("SIG{signal} could not be sent"));
        }
    }
}

#[test]
fn sighup_sigint_and_sigterm_stop_a_hardware_run_with_a_blocked_reader_and_restore_once() {
    for signal in STOP_SIGNALS {
        let dir = private_dir("stop");
        let mut child = spawn_child("stop", &dir, &STOP_SIGNALS);
        if !child.wait_for(READY, WAIT) {
            child.fail(&format!("SIG{signal}: the child ended before its first frame"));
        }
        child.signal(signal);
        // The child checks one restore and the release itself and says `done`.
        child.finish(STOP_LIMIT);
    }
}

#[test]
fn a_command_tapped_in_a_hardware_run_gets_the_callers_signal_mask() {
    let rows: [(&str, &[&str], u64); 3] = [
        ("nothing blocked", &[], 0),
        ("SIGUSR1 blocked", &["USR1"], SIGUSR1_BIT),
        ("the stop signals already blocked", &STOP_SIGNALS, SIGHUP_BIT | SIGINT_BIT | SIGTERM_BIT),
    ];
    for (label, blocked, expected) in rows {
        let dir = private_dir("mask");
        let lines = spawn_child("mask", &dir, blocked).finish(WAIT);
        let masks = lines
            .iter()
            .find_map(|l| l.split_once(MASKS).map(|(_, masks)| masks))
            .expect("the child reports the masks");
        let [caller, command]: [u64; 2] = masks
            .split_whitespace()
            .map(|m| u64::from_str_radix(m, 16).unwrap())
            .collect::<Vec<_>>()
            .try_into()
            .unwrap();
        assert_eq!(caller, expected, "{label}: the caller's mask reached the child");
        assert_eq!(
            command, caller,
            "{label}: the command runs with the caller's mask, {command:x} vs {caller:x}"
        );
    }
}

/// The child side of the two tests above; does nothing when run any other way.
#[test]
#[ignore = "runs only as the child process of the stop-signal and mask tests"]
fn hardware_run_in_a_child_process() {
    let Some(spec) = std::env::var_os(CHILD) else { return };
    let spec = spec.into_string().unwrap();
    let (mode, dir) = spec.split_once(':').unwrap();
    let dir = Path::new(dir);
    match mode {
        "stop" => stopped_by_a_signal(dir),
        "mask" => command_mask(dir),
        _ => panic!("unknown child mode {mode}"),
    }
    println!("{DONE}");
}

/// Starts a run whose digitizer never sends anything and whose reader stays blocked, says
/// `ready` once the first frame is shown, and checks that the stop signal ended it as an ordinary
/// end with one restore and the card released, without waiting for the reader.
fn stopped_by_a_signal(dir: &Path) {
    let stop = block_stop_signals().unwrap();
    let bench = Bench::bare(dir);
    bench.laptop();
    let kms = RecordingKms::touch_bar();
    let log = kms.log();
    let (unblock, held) = mpsc::channel::<()>();
    let touches = feed(Vec::new(), End::Blocks(held));
    let out = run_with(&bench, &BOTH, Announcing(kms), &log, T1_NODE, touches, Some(stop));
    drop(unblock);
    assert_eq!(out.result, Ok(()), "a stop is an ordinary end");
    assert_eq!(out.modesets(), 1);
    assert_eq!(out.restores(), 1, "restored once");
    assert_eq!(tail(&out.changes(), 6), RESTORED);
}

/// Blocks the stop signals, taps the mask command, and reports the mask this process started
/// with and the one the command ran with. The command writes into a FIFO, so the read waits for
/// it without polling.
fn command_mask(dir: &Path) {
    let caller = thread_mask();
    let stop = block_stop_signals().unwrap();
    let blocked = thread_mask();
    for (name, bit) in [("SIGHUP", SIGHUP_BIT), ("SIGINT", SIGINT_BIT), ("SIGTERM", SIGTERM_BIT)] {
        assert_eq!(blocked & bit, bit, "{name} is blocked on the calling thread: {blocked:016x}");
    }
    let bench = Bench::bare(dir);
    bench.laptop();
    assert!(Command::new("mkfifo").arg(&bench.mask).status().unwrap().success(), "a FIFO for the mask");
    let (tx, written) = mpsc::channel();
    let fifo = bench.mask.clone();
    thread::spawn(move || {
        let _ = tx.send(fs::read_to_string(fifo));
    });
    let kms = RecordingKms::touch_bar();
    let log = kms.log();
    let out = run_with(&bench, &BOTH, kms, &log, T1_NODE, closed(tap("mask")), Some(stop));
    assert_eq!(out.result, Ok(()));
    let status = written.recv_timeout(WAIT).expect("the mask command ran").unwrap();
    println!("{MASKS} {caller:x} {:x}", sig_blk(&status));
}

/// The recording card, saying `ready` on stdout after its first modeset.
struct Announcing(RecordingKms);

impl Kms for Announcing {
    fn driver_name(&mut self) -> io::Result<String> {
        self.0.driver_name()
    }

    fn set_master(&mut self) -> io::Result<()> {
        self.0.set_master()
    }

    fn drop_master(&mut self) -> io::Result<()> {
        self.0.drop_master()
    }

    fn metadata(&mut self) -> io::Result<KmsMetadata> {
        self.0.metadata()
    }

    fn create_dumb(&mut self, width: u32, height: u32, bpp: u32) -> io::Result<DumbBuffer> {
        self.0.create_dumb(width, height, bpp)
    }

    fn map_dumb(&mut self, handle: u32, len: usize) -> io::Result<()> {
        self.0.map_dumb(handle, len)
    }

    fn with_pixels(&mut self, write: &mut dyn FnMut(&mut [u8])) {
        self.0.with_pixels(write);
    }

    fn unmap(&mut self) -> io::Result<()> {
        self.0.unmap()
    }

    fn destroy_dumb(&mut self, handle: u32) -> io::Result<()> {
        self.0.destroy_dumb(handle)
    }

    fn add_fb2(&mut self, fb: &FbSpec) -> io::Result<u32> {
        self.0.add_fb2(fb)
    }

    fn set_crtc(
        &mut self,
        crtc: u32,
        fb: u32,
        connectors: &[u32],
        mode: Option<&ModeInfo>,
    ) -> io::Result<()> {
        self.0.set_crtc(crtc, fb, connectors, mode)?;
        if fb != 0 {
            println!("{READY}");
        }
        Ok(())
    }

    fn dirty(&mut self, fb: u32, clips: &[DirtyClip]) -> io::Result<()> {
        self.0.dirty(fb, clips)
    }

    fn rm_fb(&mut self, fb: u32) -> io::Result<()> {
        self.0.rm_fb(fb)
    }
}
