//! `touchbard`: shows the default layout on the simulator and presses buttons from stdin touches.
//!
//! `printf 'down 906\nup\n' | touchbard --sim out/touchbard` writes `frame-0001..0003.png`
//! (start, F5 pressed, released). Touch lines are `down <x>` or `up`; it exits when stdin closes.
//! The bar is drawn in the active Omarchy theme of `$HOME` (ADR-0008), or in the bundled Tokyo
//! Night with the reason on stderr, and redrawn when the theme is switched.
//!
//! `--keys auto` follows the held Fn and modifier keys of the Fn keyboard (found by its `KEY_FN`
//! capability in `/proc/bus/input/devices`) and shows their layer; `--keys PATH` reads key events
//! from PATH instead (ADR-0009). Without `--keys` no keyboard is read. A keyboard that cannot be
//! opened or fails is logged and the bar stays on, or goes back to, the default layer.
//!
//! `--sim-window` also shows the simulator's screen live in a desktop window at the bar's
//! proportions, and a mouse press on it is the touch instead of stdin (ADR-0010); the run ends
//! when the window closes. Without a display it exits 1 with the reason.
//!
//! A tap runs its button's action (ADR-0011, ADR-0012). `--emit uinput` presses keys on a
//! virtual keyboard named `touchbard`; `--emit PATH` writes the same `input_event` records to
//! a regular file. If emission is unavailable, the reason is logged and each tapped key action
//! logs that nothing was pressed. Commands run through `sh -c`, detached. Their failures are
//! logged. No action failure stops the bar.
//!
//! `--drm auto|PATH` uses the validated Touch Bar card instead of the simulator. `--touch
//! auto|PATH` uses the validated digitizer instead of stdin. A hardware run restores the
//! display on EOF, failure, SIGHUP, SIGINT or SIGTERM (ADR-0014).
//!
//! The saved config under `$HOME/.config/touchbar-customizer/config.toml` supplies the layout
//! and theme source. Atomic saves are followed live; invalid saves retain the accepted config.

mod config;
mod devices;
mod log;
mod workers;

use std::io;
use std::os::fd::AsFd;
use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::sync::mpsc::{self, Sender};
use std::thread;

use std::sync::Arc;
use tb_config::ActiveTheme;
use tb_core::{BarSize, BarTheme};

use config::ConfigState;

use tb_daemon::{
    ActionError, Actions, Daemon, Display, EvdevModifiers, Event, GpuRenderer, KeyEmitter, LineTouches,
    Report, SceneRenderer, ShSpawner, SimDisplay, SimWindow, StopRead, StopSignals, TouchError, TouchSource,
    VirtualKeyboard, block_stop_signals, find_fn_keyboard, forward_modifiers, sim_window_app,
};

#[cfg(test)]
use devices::run_hardware;
use devices::{Devices, HardwareInput, Selection, run_hardware_with};
use workers::Workers;

const USAGE: &str = "usage: touchbard [--sim DIR | --drm auto|PATH] [--touch auto|PATH] [--keys auto|PATH] [--emit uinput|PATH] [--sim-window]   (without --touch or --sim-window, touches on stdin: `down <x>` | `up`)";
const DEFAULT_SIM_DIR: &str = "out/touchbard";
/// The kernel's list of input devices and their capabilities.
const DEVICES: &str = "/proc/bus/input/devices";

/// Where the held modifier keys are read from.
#[derive(Debug, PartialEq, Eq)]
enum Keys {
    /// The Fn keyboard, found in `/proc/bus/input/devices`.
    Auto,
    /// Key event records from this file or device.
    Path(PathBuf),
}

/// Where the keys of tapped buttons are pressed.
#[derive(Debug, PartialEq, Eq)]
enum Emit {
    /// A uinput virtual keyboard.
    Uinput,
    /// `input_event` records written to this file.
    Path(PathBuf),
}

#[derive(Debug, PartialEq, Eq)]
struct Args {
    sim_dir: PathBuf,
    keys: Option<Keys>,
    emit: Option<Emit>,
    drm: Option<Selection>,
    touch: Option<Selection>,
    /// Show the simulator window, whose presses are the touches (stdin is not read).
    window: bool,
}

fn main() -> ExitCode {
    let Some(args) = parse_args(std::env::args().skip(1)) else {
        log::error(USAGE);
        return ExitCode::from(2);
    };
    match run(args) {
        Ok(()) => ExitCode::SUCCESS,
        Err(message) => {
            log::error(message);
            ExitCode::FAILURE
        }
    }
}

/// Each flag is accepted at most once, in any order, subject to the display/input conflicts.
fn parse_args(mut args: impl Iterator<Item = String>) -> Option<Args> {
    let mut sim_dir = None;
    let mut parsed =
        Args { sim_dir: PathBuf::new(), keys: None, emit: None, drm: None, touch: None, window: false };
    while let Some(flag) = args.next() {
        if flag == "--sim-window" {
            if std::mem::replace(&mut parsed.window, true) {
                return None;
            }
            continue;
        }
        let value = args.next()?;
        if matches!(flag.as_str(), "--drm" | "--touch") && value.starts_with("--") {
            return None;
        }
        let slot_was_empty = match flag.as_str() {
            "--sim" => sim_dir.replace(PathBuf::from(value)).is_none(),
            "--keys" if value == "auto" => parsed.keys.replace(Keys::Auto).is_none(),
            "--keys" => parsed.keys.replace(Keys::Path(PathBuf::from(value))).is_none(),
            "--emit" if value == "uinput" => parsed.emit.replace(Emit::Uinput).is_none(),
            "--emit" => parsed.emit.replace(Emit::Path(PathBuf::from(value))).is_none(),
            "--drm" => parsed.drm.replace(value.into()).is_none(),
            "--touch" => parsed.touch.replace(value.into()).is_none(),
            _ => return None,
        };
        if !slot_was_empty {
            return None;
        }
    }
    if parsed.drm.is_some() && (sim_dir.is_some() || parsed.window) || parsed.touch.is_some() && parsed.window
    {
        return None;
    }
    parsed.sim_dir = sim_dir.unwrap_or_else(|| PathBuf::from(DEFAULT_SIM_DIR));
    Some(parsed)
}

/// What reaches the daemon loop. The theme thread never ends on its own, so the end of the
/// touches (stdin closed or failed) is a message rather than the channel closing.
enum Message {
    Event(Event),
    TouchesEnded,
    Stop(Result<(), String>),
    ConfigChanged,
}

impl From<Event> for Message {
    fn from(event: Event) -> Self {
        Message::Event(event)
    }
}

/// Startup config and theme under HOME, followed while running; both modes share the coordinator.
fn run(args: Args) -> Result<(), String> {
    let home = std::env::var_os("HOME").map(PathBuf::from);
    if args.drm.is_some() || args.touch.is_some() {
        let stop = block_stop_signals().map_err(|e| format!("cannot block stop signals: {e}"))?;
        let workers = Workers::new().map_err(|e| format!("cannot create stop pipe: {e}"))?;
        let devices = Devices::real(workers.stop_reader());
        return run_hardware_with(
            args,
            home.as_deref(),
            || GpuRenderer::new().map_err(|e| e.to_string()),
            devices,
            Some(stop),
            HardwareInput { stdin: io::stdin(), workers, join_digitizer: true },
        );
    }
    if args.window {
        return run_with_window(args, home.as_deref());
    }
    let renderer = GpuRenderer::new().map_err(|e| e.to_string())?;
    let display = SimDisplay::new(BarSize::MACBOOK_PRO, args.sim_dir).map_err(|e| e.to_string())?;
    let (tx, rx) = mpsc::channel();
    let mut workers = Workers::default();
    let theme = follow_keys_and_theme(args.keys, home.as_deref(), &tx, &mut workers);
    let config = ConfigState::start(home.as_deref(), theme, &tx, &mut workers);
    let actions = actions(args.emit, None);
    let touches =
        thread::spawn(move || forward_until_ended(&mut LineTouches::new(std::io::stdin().lock()), &tx));
    run_daemon(renderer, display, config, actions, rx)?;
    join(touches, "touch reader")
}

/// `run` with the simulator window as the touch source. winit needs the main thread for the
/// window and `GpuRenderer` is not `Send`, so the daemon is built and run on another thread.
fn run_with_window(args: Args, home: Option<&Path>) -> Result<(), String> {
    let (window, mut touches) = SimWindow::new();
    let display = SimDisplay::new(BarSize::MACBOOK_PRO, args.sim_dir)
        .map_err(|e| e.to_string())?
        .on_present(window.show_screen());
    let (tx, rx) = mpsc::channel();
    let mut workers = Workers::default();
    let theme = follow_keys_and_theme(args.keys, home, &tx, &mut workers);
    let config = ConfigState::start(home, theme, &tx, &mut workers);
    let actions = actions(args.emit, None);
    let app = sim_window_app(window);
    let handle = app.handle();
    let forward_tx = tx.clone();
    let touches = thread::spawn(move || forward_until_ended(&mut touches, &forward_tx));
    let daemon = thread::spawn(move || {
        let ran = GpuRenderer::new()
            .map_err(|e| e.to_string())
            .and_then(|renderer| run_daemon(renderer, display, config, actions, rx));
        if ran.is_err() {
            // Nothing new will be shown: close the window rather than leave it frozen.
            handle.quit();
        }
        ran
    });
    // The window closed (or never opened), and with it its touches: stop the daemon.
    let shown = app.try_run();
    let _ = tx.send(Message::TouchesEnded);
    let ran = join(daemon, "daemon");
    if let Err(e) = shown {
        if let Err(daemon_error) = ran {
            log::error(daemon_error);
        }
        return Err(format!("cannot open the simulator window: {e}"));
    }
    ran?;
    join(touches, "touch reader")
}

/// Starts following the keys (when given) and the active theme of `$HOME`; the theme now.
fn follow_keys_and_theme(
    keys: Option<Keys>,
    home: Option<&Path>,
    tx: &Sender<Message>,
    workers: &mut Workers,
) -> BarTheme {
    if let Some(keys) = keys {
        follow_keys(keys, tx.clone(), workers);
    }
    if let Some(home) = home {
        follow_theme(ActiveTheme::under_home(home), tx.clone(), workers)
    } else {
        log::error("HOME is not set: showing the bundled Tokyo Night, theme switches are not followed");
        BarTheme::omarchy_fallback()
    }
}

/// The actions taps run: keys on the virtual keyboard `emit` asks for, commands through `sh -c`,
/// every failure logged. Without a keyboard the reason is logged once and key actions do nothing.
fn actions(emit: Option<Emit>, stop: Option<&StopSignals>) -> Actions {
    let made = match emit {
        None => Err("no --emit given".to_owned()),
        Some(Emit::Uinput) => VirtualKeyboard::create().map(boxed).map_err(|e| e.to_string()),
        Some(Emit::Path(path)) => VirtualKeyboard::record_to(&path).map(boxed).map_err(|e| e.to_string()),
    };
    let keys = made.map_err(|reason| log::error(format_args!("{reason}; key actions do nothing"))).ok();
    let report: Report = Arc::new(|e: ActionError| log::error(e));
    Actions::new(keys, Box::new(ShSpawner::new(Arc::clone(&report)).with_stop_signals(stop)), report)
}

fn boxed(keyboard: VirtualKeyboard<std::fs::File>) -> Box<dyn KeyEmitter + Send> {
    Box::new(keyboard)
}

/// Runs the daemon on `messages` until the touches end.
fn run_daemon(
    renderer: impl SceneRenderer,
    display: impl Display,
    mut config: ConfigState,
    actions: Actions,
    messages: mpsc::Receiver<Message>,
) -> Result<(), String> {
    let mut daemon = Daemon::new(config.layout(), config.theme(), renderer, display).with_actions(actions);
    daemon.run([]).map_err(|e| e.to_string())?;
    for message in messages {
        let event = match message {
            Message::TouchesEnded => break,
            Message::Stop(result) => return result,
            Message::ConfigChanged => config.reload(),
            Message::Event(Event::Theme(theme)) => config.active_theme(theme),
            Message::Event(event) => Some(event),
        };
        if let Some(event) = event {
            daemon.handle(event).map_err(|e| e.to_string())?;
        }
    }
    Ok(())
}

/// Forwards `source`'s touches, then tells the daemon they ended.
fn forward_until_ended(source: &mut impl TouchSource, tx: &Sender<Message>) -> Result<(), TouchError> {
    let ended = forward_touches(source, tx);
    // A dropped receiver means the daemon already stopped: nobody is left to tell.
    let _ = tx.send(Message::TouchesEnded);
    ended
}

/// The result of the thread `what`, or that it panicked.
fn join<E: ToString>(thread: thread::JoinHandle<Result<(), E>>, what: &str) -> Result<(), String> {
    match thread.join() {
        Ok(result) => result.map_err(|e| e.to_string()),
        Err(_) => Err(format!("the {what} thread panicked")),
    }
}

/// Sends `Event::Touch` for each touch until the source closes (Ok) or fails (that error).
fn forward_touches(source: &mut impl TouchSource, events: &Sender<Message>) -> Result<(), TouchError> {
    while let Some(touch) = source.next_touch()? {
        if events.send(Message::Event(Event::Touch(touch))).is_err() {
            break;
        }
    }
    Ok(())
}

/// A thread that sends `Event::Modifiers` for every change of the held keys on `keys`. When the
/// keyboard is not found, cannot be opened or fails, the reason is logged and the bar stays on,
/// or goes back to, the default layer; touches and theme switches go on.
fn follow_keys(keys: Keys, events: Sender<Message>, workers: &mut Workers) {
    let stop = workers.stop_reader();
    if let Err(error) = workers.spawn("modifier reader", true, move || {
        let path = match keys {
            Keys::Path(path) => path,
            Keys::Auto => match std::fs::read_to_string(DEVICES).map(|text| find_fn_keyboard(&text)) {
                Ok(Some(keyboard)) => {
                    log::info(format_args!("layers follow {} ({})", keyboard.name, keyboard.path.display()));
                    keyboard.path
                }
                Ok(None) => {
                    log::error(format_args!(
                        "no keyboard reports Fn in {DEVICES}; the bar stays on the default layer"
                    ));
                    return;
                }
                Err(e) => {
                    log::error(format_args!(
                        "cannot read {DEVICES}: {e}; the bar stays on the default layer"
                    ));
                    return;
                }
            },
        };
        let ended = if let Some(stop) = stop {
            use rustix::fs::{Mode, OFlags, open};
            open(&path, OFlags::RDONLY | OFlags::NONBLOCK | OFlags::CLOEXEC, Mode::empty())
                .map_err(|source| tb_daemon::KeyError::Open { path: path.clone(), source: source.into() })
                .and_then(|file| {
                    let mut keyboard = EvdevModifiers::new(StopRead(file, Some(stop)));
                    forward_modifiers(&mut keyboard, &events)
                })
        } else {
            EvdevModifiers::open(&path).and_then(|mut keyboard| forward_modifiers(&mut keyboard, &events))
        };
        match ended {
            Ok(()) => log::info(format_args!(
                "the keyboard {} closed; the bar is on the default layer",
                path.display()
            )),
            Err(e) => log::error(format_args!("{e}; the bar is on the default layer")),
        }
    }) {
        log::error(format_args!("cannot start modifier reader: {error}"));
    }
}

/// The active theme now, and a thread that sends `Event::Theme` after every theme switch. The
/// watch starts before the first load, so no switch is lost. When it cannot start or ends, the
/// reason is logged and the bar keeps its theme.
fn follow_theme(active: ActiveTheme, events: Sender<Message>, workers: &mut Workers) -> BarTheme {
    let watch = active.watch();
    let theme = active_theme(&active);
    match watch {
        Ok(mut watch) => {
            let stop = workers.stop_reader();
            if let Err(error) = workers.spawn("theme watch", true, move || {
                let ended = loop {
                    match watch.next_change_until(stop.as_deref().map(AsFd::as_fd)) {
                        Ok(true) => {}
                        Ok(false) => return,
                        Err(error) => break error,
                    }
                    if events.send(Message::Event(Event::Theme(active_theme(&active)))).is_err() {
                        return;
                    }
                };
                log::error(format_args!("{ended}; theme switches are no longer followed"));
            }) {
                log::error(format_args!("cannot start theme watch: {error}"));
            }
        }
        Err(e) => log::error(format_args!("{e}; theme switches are not followed")),
    }
    theme
}

/// The active theme, or the bundled Tokyo Night with the reason logged.
fn active_theme(active: &ActiveTheme) -> BarTheme {
    let loaded = active.load();
    if let Some(reason) = loaded.fallback {
        log::error(format_args!("{reason}; showing the bundled Tokyo Night"));
    }
    loaded.theme
}

// The binary root's tests sit beside it as `main/tests.rs`, like `module/tests.rs` elsewhere.
#[cfg(test)]
#[path = "main/tests.rs"]
mod tests;

#[cfg(test)]
#[path = "main/devices_tests.rs"]
mod devices_tests;
