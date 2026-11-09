//! Change notification for the config file through inotify on its directory.
//!
//! The directory is watched, not the file: an editor or `save` that renames a new file into place
//! replaces the inode, and a watch on the file would go quiet after the first such save.

use std::ffi::OsString;
use std::io;
use std::mem::MaybeUninit;
use std::os::fd::OwnedFd;
use std::os::unix::ffi::OsStrExt;
use std::path::PathBuf;

use rustix::fs::inotify::{self, CreateFlags, ReadFlags, Reader, WatchFlags};

use crate::file::{ConfigFile, ConfigFileError, dir_of};

/// Room for many events per read; changes read together count once.
const EVENT_BUFFER: usize = 4096;

/// A watch on one config file. `Send`, so it can block on its own thread.
#[derive(Debug)]
pub struct ConfigWatch {
    inotify: OwnedFd,
    dir: PathBuf,
    name: OsString,
}

impl ConfigFile {
    /// Starts watching the file's directory (it must exist: `load` and `save` create it). The
    /// watch is registered before this returns, so no later change is missed.
    pub fn watch(&self) -> Result<ConfigWatch, ConfigFileError> {
        let dir = dir_of(self.path()).to_path_buf();
        let name = self.path().file_name().unwrap_or_default().to_owned();
        let io_at =
            |source: rustix::io::Errno| ConfigFileError::Io { path: dir.clone(), source: source.into() };
        let inotify = inotify::init(CreateFlags::CLOEXEC).map_err(io_at)?;
        // CLOSE_WRITE: written in place and complete. MOVED_TO: renamed into place.
        inotify::add_watch(
            &inotify,
            &dir,
            WatchFlags::CLOSE_WRITE | WatchFlags::MOVED_TO | WatchFlags::ONLYDIR,
        )
        .map_err(io_at)?;
        Ok(ConfigWatch { inotify, dir, name })
    }
}

impl ConfigWatch {
    /// Blocks until the config file changed: written and closed in place, or renamed into place.
    /// Changes read together count once. Other files in the directory and deletions are not
    /// changes. The directory going away ends the watch (`WatchEnded`).
    pub fn next_change(&mut self) -> Result<(), ConfigFileError> {
        let mut buf = [MaybeUninit::<u8>::uninit(); EVENT_BUFFER];
        let mut reader = Reader::new(&self.inotify, &mut buf);
        let mut changed = false;
        loop {
            let event = reader
                .next()
                .map_err(|e| ConfigFileError::Io { path: self.dir.clone(), source: io::Error::from(e) })?;
            let flags = event.events();
            if flags.contains(ReadFlags::IGNORED) {
                return Err(ConfigFileError::WatchEnded { dir: self.dir.clone() });
            }
            // A queue overflow may have dropped an event for the config: report it as a change.
            changed |= flags.contains(ReadFlags::QUEUE_OVERFLOW)
                || event.file_name().is_some_and(|n| n.to_bytes() == self.name.as_bytes());
            if changed && reader.is_buffer_empty() {
                return Ok(());
            }
        }
    }
}
