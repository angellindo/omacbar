//! Change notification through inotify on directories: the config file's directory, and the
//! Omarchy and touchbar-customizer state directories for a theme switch.
//!
//! Directories are watched, not files: an editor or `save` that renames a new file into place
//! replaces the inode, and a watch on the file would go quiet after the first such save. The same
//! holds for `omarchy theme set`, which renames a new `theme` directory into place.

use std::ffi::OsString;
use std::io;
use std::mem::MaybeUninit;
use std::os::fd::OwnedFd;
use std::os::unix::ffi::OsStrExt;
use std::path::{Path, PathBuf};

use rustix::fs::inotify::{self, CreateFlags, ReadFlags, Reader, WatchFlags};

use crate::file::{ConfigFile, ConfigFileError, dir_of};

/// Room for many events per read; changes read together count once.
const EVENT_BUFFER: usize = 4096;

/// A watch on one config file. `Send`, so it can block on its own thread.
#[derive(Debug)]
pub struct ConfigWatch {
    dirs: DirWatch,
}

impl ConfigFile {
    /// Starts watching the file's directory (it must exist: `load` and `save` create it). The
    /// watch is registered before this returns, so no later change is missed.
    pub fn watch(&self) -> Result<ConfigWatch, ConfigFileError> {
        let dir = dir_of(self.path());
        let name = self.path().file_name().unwrap_or_default().to_owned();
        let io_at = |source: io::Error| ConfigFileError::Io { path: dir.to_path_buf(), source };
        let mut dirs = DirWatch::new().map_err(io_at)?;
        dirs.add(dir, vec![name]).map_err(io_at)?;
        Ok(ConfigWatch { dirs })
    }
}

impl ConfigWatch {
    /// Blocks until the config file changed: written and closed in place, or renamed into place.
    /// Changes read together count once. Other files in the directory and deletions are not
    /// changes. The directory going away ends the watch (`WatchEnded`).
    pub fn next_change(&mut self) -> Result<(), ConfigFileError> {
        self.dirs.next_change().map_err(|e| match e {
            WatchFailure::Io { dir, source } => ConfigFileError::Io { path: dir, source },
            WatchFailure::Ended { dir } => ConfigFileError::WatchEnded { dir },
        })
    }
}

/// Why a [`DirWatch`] stopped; each owner maps it to its own error type.
#[derive(Debug)]
pub(crate) enum WatchFailure {
    Io {
        dir: PathBuf,
        source: io::Error,
    },
    /// The watched directory `dir` was removed (or its filesystem unmounted).
    Ended {
        dir: PathBuf,
    },
}

/// One inotify instance watching names in some directories.
#[derive(Debug)]
pub(crate) struct DirWatch {
    inotify: OwnedFd,
    dirs: Vec<Watched>,
}

#[derive(Debug)]
struct Watched {
    wd: i32,
    dir: PathBuf,
    names: Vec<OsString>,
}

impl DirWatch {
    pub(crate) fn new() -> io::Result<Self> {
        let inotify = inotify::init(CreateFlags::CLOEXEC)?;
        Ok(Self { inotify, dirs: Vec::new() })
    }

    /// Watches `names` in `dir` (which must exist) from now on: a name written and closed in
    /// place (`CLOSE_WRITE`) or renamed into place (`MOVED_TO`, files and directories alike).
    pub(crate) fn add(&mut self, dir: &Path, names: Vec<OsString>) -> io::Result<()> {
        let wd = inotify::add_watch(
            &self.inotify,
            dir,
            WatchFlags::CLOSE_WRITE | WatchFlags::MOVED_TO | WatchFlags::ONLYDIR,
        )?;
        self.dirs.push(Watched { wd, dir: dir.to_path_buf(), names });
        Ok(())
    }

    /// Blocks until one of the watched names changed; changes read together count once. A queue
    /// overflow may have dropped such a change, so it counts as one. A watched directory going
    /// away ends the watch, naming it.
    pub(crate) fn next_change(&mut self) -> Result<(), WatchFailure> {
        let mut buf = [MaybeUninit::<u8>::uninit(); EVENT_BUFFER];
        let mut reader = Reader::new(&self.inotify, &mut buf);
        let mut changed = false;
        loop {
            let event = reader.next().map_err(|e| WatchFailure::Io {
                dir: self.dirs.first().map(|w| w.dir.clone()).unwrap_or_default(),
                source: io::Error::from(e),
            })?;
            let flags = event.events();
            let watched = self.dirs.iter().find(|w| w.wd == event.wd());
            if flags.contains(ReadFlags::IGNORED) {
                if let Some(w) = watched {
                    return Err(WatchFailure::Ended { dir: w.dir.clone() });
                }
                continue;
            }
            changed |= flags.contains(ReadFlags::QUEUE_OVERFLOW)
                || watched
                    .zip(event.file_name())
                    .is_some_and(|(w, name)| w.names.iter().any(|n| n.as_bytes() == name.to_bytes()));
            if changed && reader.is_buffer_empty() {
                return Ok(());
            }
        }
    }
}
