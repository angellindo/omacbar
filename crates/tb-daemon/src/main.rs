//! `touchbard`: shows the default layout on the simulator and presses buttons from stdin touches.
//!
//! `printf 'down 906\nup\n' | touchbard --sim out/touchbard` writes `frame-0001..0003.png`
//! (start, F5 pressed, released). Touch lines are `down <x>` or `up`; it exits when stdin closes.

mod log;

use std::path::PathBuf;
use std::process::ExitCode;
use std::sync::mpsc;
use std::thread;

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

/// Startup state: the default preset and the fallback theme (config file TB-13, live theme TB-14).
fn run(sim_dir: PathBuf) -> Result<(), String> {
    let display = SimDisplay::new(BarSize::MACBOOK_PRO, sim_dir).map_err(|e| e.to_string())?;
    let mut daemon =
        Daemon::new(presets::default_layout(), BarTheme::omarchy_fallback(), GpuRenderer::new(), display);
    let (tx, rx) = mpsc::channel();
    let touches = thread::spawn(move || forward_touches(&mut LineTouches::new(std::io::stdin().lock()), &tx));
    daemon.run(rx).map_err(|e| e.to_string())?;
    match touches.join() {
        Ok(result) => result.map_err(|e| e.to_string()),
        Err(_) => Err("the touch reader thread panicked".to_owned()),
    }
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
