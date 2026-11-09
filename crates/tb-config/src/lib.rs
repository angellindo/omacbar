//! The config file on disk: load (default when missing), validate, save atomically, watch for
//! changes. File I/O lives here so `tb-core` stays pure.

mod file;
mod watch;

pub use file::{ConfigFile, ConfigFileError, Loaded, default_path};
pub use watch::ConfigWatch;
