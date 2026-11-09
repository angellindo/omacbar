//! The files the bar follows on disk: the config file (load, default when missing, validate, save
//! atomically, watch for changes) and the active Omarchy theme (load, watch for a theme switch).
//! File I/O lives here so `tb-core` stays pure.

mod file;
mod theme;
mod watch;

pub use file::{ConfigFile, ConfigFileError, Loaded, default_path};
pub use theme::{ActiveTheme, LoadedTheme, ThemeFileError};
pub use watch::ConfigWatch;
