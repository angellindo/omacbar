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

mod log;

use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::sync::mpsc::{self, Sender};
use std::thread;

use tb_config::ActiveTheme;
use tb_core::{BarSize, BarTheme, presets};
use tb_daemon::{
    Daemon, EvdevModifiers, Event, GpuRenderer, LineTouches, SimDisplay, TouchError, TouchSource,
    find_fn_keyboard, forward_modifiers,
};

const USAGE: &str = "usage: touchbard [--sim DIR] [--keys auto|PATH]   (touches on stdin: `down <x>` | `up`)";
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

#[derive(Debug, PartialEq, Eq)]
struct Args {
    sim_dir: PathBuf,
    keys: Option<Keys>,
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

/// `[--sim DIR] [--keys auto|PATH]` in any order, each at most once; `None` for anything else.
fn parse_args(mut args: impl Iterator<Item = String>) -> Option<Args> {
    let (mut sim_dir, mut keys) = (None, None);
    while let Some(flag) = args.next() {
        let value = args.next()?;
        let slot_was_empty = match flag.as_str() {
            "--sim" => sim_dir.replace(PathBuf::from(value)).is_none(),
            "--keys" if value == "auto" => keys.replace(Keys::Auto).is_none(),
            "--keys" => keys.replace(Keys::Path(PathBuf::from(value))).is_none(),
            _ => return None,
        };
        if !slot_was_empty {
            return None;
        }
    }
    Some(Args { sim_dir: sim_dir.unwrap_or_else(|| PathBuf::from(DEFAULT_SIM_DIR)), keys })
}

/// What reaches the daemon loop. The theme thread never ends on its own, so the end of the
/// touches (stdin closed or failed) is a message rather than the channel closing.
enum Message {
    Event(Event),
    TouchesEnded,
}

impl From<Event> for Message {
    fn from(event: Event) -> Self {
        Message::Event(event)
    }
}

/// Startup state: the default preset (config file: a later ticket) and the active theme of `$HOME`,
/// followed while running.
fn run(args: Args) -> Result<(), String> {
    let renderer = GpuRenderer::new().map_err(|e| e.to_string())?;
    let display = SimDisplay::new(BarSize::MACBOOK_PRO, args.sim_dir).map_err(|e| e.to_string())?;
    let (tx, rx) = mpsc::channel();
    if let Some(keys) = args.keys {
        follow_keys(keys, tx.clone());
    }
    let theme = if let Some(home) = std::env::var_os("HOME") {
        follow_theme(ActiveTheme::under_home(Path::new(&home)), tx.clone())
    } else {
        log::error("HOME is not set: showing the bundled Tokyo Night, theme switches are not followed");
        BarTheme::omarchy_fallback()
    };
    let mut daemon = Daemon::new(presets::default_layout(), theme, renderer, display);
    let touches = thread::spawn(move || {
        let ended = forward_touches(&mut LineTouches::new(std::io::stdin().lock()), &tx);
        // A dropped receiver means the daemon already stopped: nobody is left to tell.
        let _ = tx.send(Message::TouchesEnded);
        ended
    });
    let events = rx.into_iter().map_while(|message| match message {
        Message::Event(event) => Some(event),
        Message::TouchesEnded => None,
    });
    daemon.run(events).map_err(|e| e.to_string())?;
    match touches.join() {
        Ok(result) => result.map_err(|e| e.to_string()),
        Err(_) => Err("the touch reader thread panicked".to_owned()),
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
fn follow_keys(keys: Keys, events: Sender<Message>) {
    thread::spawn(move || {
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
        let ended =
            EvdevModifiers::open(&path).and_then(|mut keyboard| forward_modifiers(&mut keyboard, &events));
        match ended {
            Ok(()) => log::info(format_args!(
                "the keyboard {} closed; the bar is on the default layer",
                path.display()
            )),
            Err(e) => log::error(format_args!("{e}; the bar is on the default layer")),
        }
    });
}

/// The active theme now, and a thread that sends `Event::Theme` after every theme switch. The
/// watch starts before the first load, so no switch is lost. When it cannot start or ends, the
/// reason is logged and the bar keeps its theme.
fn follow_theme(active: ActiveTheme, events: Sender<Message>) -> BarTheme {
    let watch = active.watch();
    let theme = active_theme(&active);
    match watch {
        Ok(mut watch) => {
            thread::spawn(move || {
                let ended = loop {
                    if let Err(e) = watch.next_change() {
                        break e;
                    }
                    if events.send(Message::Event(Event::Theme(active_theme(&active)))).is_err() {
                        return;
                    }
                };
                log::error(format_args!("{ended}; theme switches are no longer followed"));
            });
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

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(args: &[&str]) -> Option<Args> {
        parse_args(args.iter().map(|a| (*a).to_owned()))
    }

    fn sim(dir: &str, keys: Option<Keys>) -> Args {
        Args { sim_dir: PathBuf::from(dir), keys }
    }

    #[test]
    fn no_arguments_use_the_default_simulator_directory_and_read_no_keyboard() {
        assert_eq!(parse(&[]), Some(sim("out/touchbard", None)));
    }

    #[test]
    fn sim_takes_exactly_one_directory() {
        assert_eq!(parse(&["--sim", "x/y"]), Some(sim("x/y", None)));
        assert_eq!(parse(&["--sim"]), None);
        assert_eq!(parse(&["--sim", "x", "extra"]), None);
        assert_eq!(parse(&["--sim", "x", "--sim", "y"]), None);
        assert_eq!(parse(&["x"]), None);
    }

    #[test]
    fn keys_is_auto_or_a_path_in_either_order_with_sim() {
        assert_eq!(parse(&["--keys", "auto"]), Some(sim("out/touchbard", Some(Keys::Auto))));
        assert_eq!(
            parse(&["--keys", "k.bin", "--sim", "s"]),
            Some(sim("s", Some(Keys::Path(PathBuf::from("k.bin")))))
        );
        assert_eq!(parse(&["--sim", "s", "--keys", "auto"]), Some(sim("s", Some(Keys::Auto))));
        assert_eq!(parse(&["--keys", "auto", "--keys", "auto"]), None);
    }
}
