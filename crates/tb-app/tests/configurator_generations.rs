//! The real `touchbar-customizer` keeps a generation before it writes the profile: before the
//! first default (the absence, and whether the directory existed) and before a save from the
//! editor (the exact hand-written bytes and permission bits it replaces, while the saved file
//! keeps those bits). A new process (this test) finds the generation complete; quitting and
//! reopening keeps nothing new. When the copy cannot be written (file size limit, EFBIG) the save
//! is abandoned and the hand-written file stays byte for byte; with room for the copy the same
//! save goes through. Every run has its own temp `HOME` and `XDG_STATE_HOME` under cargo's temp
//! dir, never the real ones. The binary runs need a GPU adapter (lavapipe is fine).
//!
//! The size-limit cases cannot cap the binary: a GPU driver may need large files of its own to
//! create a device (Mesa `anv` needs `RLIMIT_FSIZE` >= 1 GiB), so a capped binary stops before the
//! Save. They run the same configurator headless instead, through its production entry
//! (`spec().init` reads `HOME` and `XDG_STATE_HOME`), inside a child process of this test that is
//! capped from `exec` and ignores `SIGXFSZ`; no GPU, no timing. The child reports, and the parent
//! checks, that the cap was in force, that the Save was tapped and what the editor said about it.
//!
//! The store is read straight from disk, in the layout ADR-0014 fixes: one directory per
//! generation holding `manifest.toml` (`kind`, `reason`, `mode`, ...) and, for a kept file,
//! `config.toml`.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::ffi::OsStr;
use std::fs;
use std::io::Write;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::Command;

use rune::{Env, Runtime};
use tb_app::State;
use tb_config::default_path;
use tb_core::config::{Config, parse, to_toml};

const STORE: &str = "touchbar-customizer/backups";
/// The file size limit (soft and hard) of both size-limit runs, in bytes.
const LIMIT: usize = 64 * 1024;
/// Set only in the size-limited child: it makes the helper test drive the editor.
const CHILD: &str = "CONFIGURATOR_GENERATIONS_SIZE_LIMITED_CHILD";
/// The helper test the size-limit cases run in their child process.
const HELPER: &str = "size_limited_editor_saves_once";
/// Prefix of the child's report lines on stdout.
const REPORT: &str = "size-limited-editor:";
/// The parent's copy of the child's stdout, kept in the case's profile dir: the parent never
/// writes to its own stdout, so libtest's `test <name> ... ok|FAILED` lines stay whole.
const CHILD_STDOUT: &str = "size-limited-editor.stdout";

struct Profile {
    dir: PathBuf,
    home: PathBuf,
    state: PathBuf,
}

impl Profile {
    fn new(test: &str) -> Self {
        let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join("configurator_generations").join(test);
        if dir.exists() {
            fs::remove_dir_all(&dir).unwrap();
        }
        let (home, state) = (dir.join("home"), dir.join("state"));
        fs::create_dir_all(&home).unwrap();
        fs::create_dir_all(&state).unwrap();
        Self { dir, home, state }
    }

    fn config(&self) -> PathBuf {
        default_path(&self.home)
    }

    fn store(&self) -> PathBuf {
        self.state.join(STORE)
    }

    fn seed(&self, bytes: &[u8], mode: u32) {
        fs::create_dir_all(self.config().parent().unwrap()).unwrap();
        fs::write(self.config(), bytes).unwrap();
        fs::set_permissions(self.config(), fs::Permissions::from_mode(mode)).unwrap();
    }

    fn command(&self, program: &OsStr) -> Command {
        let mut command = Command::new(program);
        command.env("HOME", &self.home).env("XDG_STATE_HOME", &self.state).current_dir(&self.dir);
        command
    }

    /// `--snap <dir>/<tag>.png` plus `extra`.
    fn args(&self, tag: &str, extra: &[&OsStr]) -> Vec<std::ffi::OsString> {
        let mut args = vec!["--snap".into(), self.dir.join(format!("{tag}.png")).into_os_string()];
        args.extend(extra.iter().map(|a| a.to_os_string()));
        args
    }

    /// One configurator run; it must exit 0.
    fn run(&self, tag: &str, extra: &[&OsStr]) {
        let out = self
            .command(OsStr::new(env!("CARGO_BIN_EXE_touchbar-customizer")))
            .args(self.args(tag, extra))
            .output()
            .unwrap();
        assert!(
            out.status.success(),
            "exit {:?}, stderr: {}",
            out.status,
            String::from_utf8_lossy(&out.stderr)
        );
    }

    /// The size-limited editor run: this test binary again, capped by `prlimit` at `LIMIT` (soft
    /// and hard) from `exec` with `SIGXFSZ` ignored, so an oversized write fails with EFBIG,
    /// running only `HELPER`. It must exit 0; its report lines are returned as `key=value` and
    /// its whole stdout is kept in the profile dir as `CHILD_STDOUT`.
    fn save_in_a_size_limited_editor(&self) -> Report {
        let me = std::env::current_exe().unwrap();
        let out = self
            .command(OsStr::new("sh"))
            .arg("-c")
            .arg(r#"trap "" XFSZ; exec prlimit --fsize="$0:$0" -- "$@""#)
            .arg(LIMIT.to_string())
            .arg(me)
            .args([HELPER, "--exact", "--ignored", "--nocapture", "--test-threads=1"])
            .env(CHILD, "1")
            .output()
            .unwrap();
        let stdout = String::from_utf8_lossy(&out.stdout).into_owned();
        let stderr = String::from_utf8_lossy(&out.stderr);
        assert!(out.status.success(), "the limited editor run: exit {:?}\n{stdout}\n{stderr}", out.status);
        let lines: Vec<(String, String)> = stdout
            .lines()
            .filter_map(|l| l.strip_prefix(REPORT))
            .filter_map(|l| l.trim().split_once('='))
            .map(|(k, v)| (k.to_owned(), v.to_owned()))
            .collect();
        assert!(!lines.is_empty(), "the helper reported nothing (did it run?)\n{stdout}\n{stderr}");
        fs::write(self.dir.join(CHILD_STDOUT), &stdout).unwrap();
        Report { lines, raw: stdout }
    }
}

/// What the size-limited editor saw, in the order it reported it.
struct Report {
    lines: Vec<(String, String)>,
    raw: String,
}

impl Report {
    fn get(&self, key: &str) -> &str {
        self.lines
            .iter()
            .find(|(k, _)| k == key)
            .map_or_else(|| panic!("no `{key}` in the report:\n{}", self.raw), |(_, v)| v.as_str())
    }

    /// The cap was in force in the process that saved, and an oversized write fails, not kills.
    fn assert_limited(&self) {
        assert_eq!(self.get("fsize-soft"), LIMIT.to_string(), "{}", self.raw);
        assert_eq!(self.get("fsize-hard"), LIMIT.to_string(), "{}", self.raw);
        assert_eq!(self.get("xfsz-ignored"), "true", "{}", self.raw);
    }

    /// The editor opened the profile, the drag changed the layout and the Save was tapped.
    fn assert_save_tapped(&self) {
        assert_eq!(self.get("opened-profile"), "true", "the editor shows the seeded file: {}", self.raw);
        assert_eq!(self.get("edited"), "true", "the drag changed the layout: {}", self.raw);
        assert!(self.get("dirty-before").contains("pending changes"), "{}", self.raw);
        assert_eq!(self.get("tapped"), "save", "{}", self.raw);
        assert_ne!(self.get("status-before"), self.get("status-after"), "the Save answered: {}", self.raw);
    }
}

fn save_script() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("scripts/drag-f3-before-f1.json")
}

/// `--script` arguments of the flow that drags F3 before F1 and saves.
fn script_args(script: &Path) -> Vec<&OsStr> {
    vec!["--script".as_ref(), script.as_os_str(), "--at".as_ref(), "mark:end".as_ref()]
}

fn mode(path: &Path) -> u32 {
    fs::symlink_metadata(path).unwrap().permissions().mode() & 0o7777
}

fn generations(store: &Path) -> Vec<PathBuf> {
    let Ok(entries) = fs::read_dir(store) else { return Vec::new() };
    let mut dirs: Vec<PathBuf> = entries
        .map(|e| e.unwrap().path())
        .filter(|p| !p.file_name().unwrap().to_string_lossy().starts_with('.'))
        .collect();
    dirs.sort();
    dirs
}

fn manifest_value(generation: &Path, key: &str) -> Option<String> {
    let text = fs::read_to_string(generation.join("manifest.toml")).ok()?;
    text.lines().find_map(|line| {
        let (k, v) = line.split_once('=')?;
        (k.trim().trim_matches('"') == key).then(|| v.trim().trim_matches(['"', '\'']).to_owned())
    })
}

/// A TOML integer as written in a manifest (`416` or `0o640`).
fn manifest_int(generation: &Path, key: &str) -> Option<u32> {
    let v = manifest_value(generation, key)?.replace('_', "");
    match v.strip_prefix("0o") {
        Some(octal) => u32::from_str_radix(octal, 8).ok(),
        None => v.parse().ok(),
    }
}

/// The default config as a person leaves it after editing by hand, padded with `padding` bytes of
/// comments: byte equality then proves the exact file was kept.
fn hand_written(padding: usize) -> Vec<u8> {
    let mut text = format!(
        "# my touch bar, edited by hand\n\n{}\n# keep this note\n",
        to_toml(&Config::default()).unwrap()
    );
    while text.len() < padding {
        text.push_str("# a long personal note that only a byte-exact copy keeps .........................\n");
    }
    text.into_bytes()
}

/// The one generation in the store that kept a file, with its exact bytes.
fn the_kept_file(store: &Path) -> (PathBuf, Vec<u8>) {
    let gens = generations(store);
    assert_eq!(gens.len(), 1, "one generation in {}: {gens:?}", store.display());
    let generation = gens[0].clone();
    assert_eq!(manifest_value(&generation, "kind").as_deref(), Some("file"));
    let data = generation.join("config.toml");
    assert!(fs::symlink_metadata(&data).unwrap().is_file());
    assert_eq!(mode(&data), 0o600, "the kept copy is private");
    let bytes = fs::read(&data).unwrap();
    (generation, bytes)
}

#[test]
fn the_first_run_keeps_an_absent_generation_before_writing_the_default() {
    let p = Profile::new("first_run");

    p.run("first", &[]);

    assert!(p.config().is_file());
    assert_eq!(mode(&p.config()), 0o600, "a new config is private");
    let gens = generations(&p.store());
    assert_eq!(gens.len(), 1, "{gens:?}");
    assert_eq!(manifest_value(&gens[0], "kind").as_deref(), Some("absent"));
    assert_eq!(manifest_value(&gens[0], "dir-existed").as_deref(), Some("false"));
    assert_eq!(mode(&p.store()), 0o700);
    assert!(!p.home.join(".local/state").join(STORE).exists(), "the store follows XDG_STATE_HOME");
}

#[test]
fn a_save_from_the_editor_keeps_the_exact_hand_written_file_and_reopening_keeps_nothing_new() {
    let p = Profile::new("save_then_reopen");
    let mine = hand_written(0);
    p.seed(&mine, 0o640);
    let script = save_script();

    p.run("save", &script_args(&script));

    let saved = fs::read(p.config()).unwrap();
    assert_ne!(saved, mine, "the flow saved a new layout");
    assert_eq!(mode(&p.config()), 0o640, "the saved file keeps the hand-written file's bits");
    let (generation, kept) = the_kept_file(&p.store());
    assert_eq!(kept, mine, "the generation holds the hand-written bytes, comments included");
    assert_eq!(manifest_int(&generation, "mode"), Some(0o640));
    assert_eq!(manifest_value(&generation, "reason").as_deref(), Some("save"));

    p.run("reopen", &[]);

    assert_eq!(fs::read(p.config()).unwrap(), saved, "reopening leaves the saved file");
    assert_eq!(generations(&p.store()), vec![generation], "reopening keeps nothing new");
}

#[test]
fn with_room_for_the_copy_a_size_limited_save_goes_through() {
    let p = Profile::new("limit_room");
    let mine = hand_written(0);
    assert!(mine.len() < LIMIT / 4, "the control file fits easily");
    p.seed(&mine, 0o600);

    let report = p.save_in_a_size_limited_editor();

    report.assert_limited();
    report.assert_save_tapped();
    let shown = p.config().display().to_string();
    let status = report.get("status-after");
    assert!(status.starts_with(&format!("saved config {shown}")), "the save went through: {}", report.raw);
    assert!(report.get("dirty-after").contains("no pending changes"), "{}", report.raw);
    let saved = fs::read(p.config()).unwrap();
    assert_ne!(saved, mine, "the new layout is on disk");
    assert_eq!(
        report.get("disk-is-editor-layout"),
        "true",
        "the file holds the editor's layout: {}",
        report.raw
    );
    assert_eq!(mode(&p.config()), 0o600);
    let (generation, kept) = the_kept_file(&p.store());
    assert_eq!(kept, mine, "the generation holds the exact replaced bytes");
    assert_eq!(manifest_value(&generation, "reason").as_deref(), Some("save"));
    assert_eq!(manifest_int(&generation, "mode"), Some(0o600));
}

#[test]
fn when_the_copy_cannot_be_written_the_save_is_abandoned_and_the_file_stays() {
    let p = Profile::new("limit_no_room");
    let mine = hand_written(4 * LIMIT);
    assert!(mine.len() > LIMIT, "only a copy of this file is too big for the limit");
    p.seed(&mine, 0o640);

    let report = p.save_in_a_size_limited_editor();

    report.assert_limited();
    report.assert_save_tapped();
    let layout: usize = report.get("layout-bytes").parse().unwrap();
    assert!(layout < LIMIT / 4, "the new layout itself fits ({layout} bytes): {}", report.raw);
    let shown = p.config().display().to_string();
    let status = report.get("status-after");
    assert!(status.starts_with(&format!("not saved {shown}")), "the save was refused: {}", report.raw);
    assert!(status.contains("os error 27"), "the reason is EFBIG, a write over the limit: {}", report.raw);
    assert!(report.get("dirty-after").contains("pending changes"), "the draft is kept: {}", report.raw);
    assert!(!report.get("dirty-after").contains("no pending"), "{}", report.raw);
    assert_eq!(fs::read(p.config()).unwrap(), mine, "the hand-written file is untouched");
    assert_eq!(mode(&p.config()), 0o640);
    for generation in generations(&p.store()) {
        assert_ne!(
            manifest_value(&generation, "kind").as_deref(),
            Some("file"),
            "no partial copy listed: {generation:?}"
        );
    }
}

/// Not a case: the editor half of the size-limit cases, run only in their capped child (`CHILD`
/// set; elsewhere it does nothing). It opens the real configurator headless through its production
/// entry, drags F3 before F1 on the default layer as `scripts/drag-f3-before-f1.json` does, taps
/// Save and prints what it saw. It writes no file of its own.
#[test]
#[ignore = "run by the size-limit cases inside their size-limited child"]
fn size_limited_editor_saves_once() {
    if std::env::var_os(CHILD).is_none() {
        return;
    }
    // libtest has already printed `test <name> ... ` without a newline: end that line first, so
    // every report line starts with `REPORT`.
    let out = std::io::stdout();
    writeln!(out.lock()).unwrap();
    let say =
        |key: &str, value: &str| writeln!(out.lock(), "{REPORT} {key}={}", value.replace('\n', " ")).unwrap();
    let limits = fs::read_to_string("/proc/self/limits").unwrap();
    let fsize = limits.lines().find(|l| l.starts_with("Max file size")).unwrap();
    let fields: Vec<&str> = fsize.split_whitespace().collect();
    say("fsize-soft", fields[3]);
    say("fsize-hard", fields[4]);
    let status = fs::read_to_string("/proc/self/status").unwrap();
    let ignored = status.lines().find_map(|l| l.strip_prefix("SigIgn:")).unwrap().trim();
    let xfsz = u64::from_str_radix(ignored, 16).unwrap() & (1 << (25 - 1)) != 0;
    say("xfsz-ignored", &xfsz.to_string());

    let spec = tb_app::spec();
    let mut rt = Runtime::new((spec.init)(), spec.view, spec.size, Env::new(true, 1.0));
    rt.on_key = spec.on_key;
    settle(&mut rt);
    let opened = rt.state.config.clone();
    let config = default_path(Path::new(&std::env::var_os("HOME").unwrap()));
    let on_disk = || parse(&fs::read_to_string(&config).unwrap()).ok();
    say("opened-profile", &(on_disk().as_ref() == Some(&opened)).to_string());
    say("status-before", &text_of(&rt, "status"));

    let (x, y) = rt.find("preview-f3").expect("the preview shows F3");
    rt.pointer_move(x, y);
    rt.pointer_down(x, y);
    rt.frame(1.0 / 60.0);
    for n in 1..=24 {
        rt.pointer_move(x - 190.0 * n as f32 / 24.0, y);
        rt.frame(1.0 / 60.0);
    }
    rt.pointer_up(x - 190.0, y);
    settle(&mut rt);
    say("edited", &(rt.state.config != opened).to_string());
    say("dirty-before", &text_of(&rt, "dirty-summary"));

    let (x, y) = rt.find("save").expect("the Save control is shown");
    rt.pointer_move(x, y);
    rt.pointer_down(x, y);
    rt.frame(1.0 / 60.0);
    rt.pointer_up(x, y);
    say("tapped", "save");
    settle(&mut rt);
    say("status-after", &text_of(&rt, "status"));
    say("dirty-after", &text_of(&rt, "dirty-summary"));
    say("layout-bytes", &to_toml(&rt.state.config).unwrap().len().to_string());
    say("disk-is-editor-layout", &(on_disk().as_ref() == Some(&rt.state.config)).to_string());
}

fn settle(rt: &mut Runtime<State>) {
    rt.frame(0.0);
    for _ in 0..30 {
        rt.frame(1.0 / 60.0);
    }
    rt.settle_all();
    rt.frame(0.0);
}

/// What the element keyed `key` shows: its own text, else the texts inside it.
fn text_of(rt: &Runtime<State>, key: &str) -> String {
    let elements = rt.elements();
    let e = elements
        .iter()
        .find(|e| e.key.as_deref() == Some(key) && !e.exiting)
        .unwrap_or_else(|| panic!("no element keyed {key}"));
    if let Some(t) = &e.text {
        return t.clone();
    }
    let inside = format!("{}/", e.path);
    elements
        .iter()
        .filter(|d| d.path.starts_with(&inside) && !d.exiting)
        .filter_map(|d| d.text.as_deref())
        .collect::<Vec<_>>()
        .join(" ")
}
