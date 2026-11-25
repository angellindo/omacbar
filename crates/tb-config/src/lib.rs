//! The files the bar follows on disk: the config file (load, default when missing, validate, save
//! atomically, watch for changes) and the active Omarchy theme (load, watch for a theme switch).
//! File I/O lives here so `tb-core` stays pure.

mod backup;
mod durable;
mod file;
mod theme;
mod watch;

#[cfg(test)]
mod recovery_regressions;

pub use backup::{BackupError, Backups, Generation, GenerationId, GenerationKind, Health, Reason};
pub use file::{ConfigFile, ConfigFileError, DiskVersion, Loaded, default_path};
pub use theme::{ActiveTheme, LoadedTheme, ThemeFileError, ThemeWatch};
pub use watch::ConfigWatch;
