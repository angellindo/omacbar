//! `touchbard`: shows the default layout on the simulator and presses buttons from stdin touches.
//!
//! `printf 'down 906\nup\n' | touchbard --sim out/touchbard` writes `frame-0001..0003.png`
//! (start, F5 pressed, released). Touch lines are `down <x>` or `up`; it exits when stdin closes.
//! The bar is drawn in the active Omarchy theme of `$HOME` (ADR-0008), or in the bundled Tokyo
//! Night with the reason on stderr.

mod log;

use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::sync::mpsc;
use std::thread;

use tb_config::ActiveTheme;
use tb_core::{BarSize, BarTheme, presets};
use tb_daemon::{Daemon, GpuRenderer, LineTouches, SimDisplay, forward_touches};

const USAGE: &str = "usage: touchbard [--sim DIR]   (touches on stdin: `down <x>` | `up`)";
const DEFAULT_SIM_DIR: &str = "out/touchbard";

fn main() -> ExitCode {
    let Some(sim_dir) = parse_args(std::env::args().skip(1)) else {
        log::error(USAGE);
        return ExitCode::from(2);
    };
    match run(sim_dir) {
        Ok(()) => ExitCode::SUCCESS,
        Err(message) => {
            log::error(message);
            ExitCode::FAILURE
        }
    }
}

/// The simulator directory from `[--sim DIR]`; `None` for anything else.
fn parse_args(mut args: impl Iterator<Item = String>) -> Option<PathBuf> {
    match (args.next().as_deref(), args.next(), args.next()) {
        (None, _, _) => Some(PathBuf::from(DEFAULT_SIM_DIR)),
        (Some("--sim"), Some(dir), None) => Some(PathBuf::from(dir)),
        _ => None,
    }
}

/// Startup state: the default preset (config file: a later ticket) and the active theme of `$HOME`.
fn run(sim_dir: PathBuf) -> Result<(), String> {
    let display = SimDisplay::new(BarSize::MACBOOK_PRO, sim_dir).map_err(|e| e.to_string())?;
    let theme = match std::env::var_os("HOME") {
        Some(home) => active_theme(&ActiveTheme::under_home(Path::new(&home))),
        None => {
            log::error("HOME is not set: showing the bundled Tokyo Night");
            BarTheme::omarchy_fallback()
        }
    };
    let mut daemon = Daemon::new(presets::default_layout(), theme, GpuRenderer::new(), display);
    let (tx, rx) = mpsc::channel();
    let touches = thread::spawn(move || forward_touches(&mut LineTouches::new(std::io::stdin().lock()), &tx));
    daemon.run(rx).map_err(|e| e.to_string())?;
    match touches.join() {
        Ok(result) => result.map_err(|e| e.to_string()),
        Err(_) => Err("the touch reader thread panicked".to_owned()),
    }
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

    fn parse(args: &[&str]) -> Option<PathBuf> {
        parse_args(args.iter().map(|a| (*a).to_owned()))
    }

    #[test]
    fn no_arguments_use_the_default_simulator_directory() {
        assert_eq!(parse(&[]), Some(PathBuf::from("out/touchbard")));
    }

    #[test]
    fn sim_takes_exactly_one_directory() {
        assert_eq!(parse(&["--sim", "x/y"]), Some(PathBuf::from("x/y")));
        assert_eq!(parse(&["--sim"]), None);
        assert_eq!(parse(&["--sim", "x", "extra"]), None);
        assert_eq!(parse(&["x"]), None);
    }
}
