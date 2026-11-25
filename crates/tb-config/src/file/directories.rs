//! Config directories created by one protected write, retained only after it succeeds.

use std::ffi::OsString;
use std::io;
use std::os::fd::{AsFd, OwnedFd};
use std::path::Path;

use rustix::fs::{AtFlags, Mode, OFlags, fstat, fsync, mkdirat, openat, statat, unlinkat};
use rustix::io::Errno;

use crate::durable::{directory_prefix, same_inode};

#[derive(Default)]
pub(super) struct CreatedDirs(Vec<Created>);

struct Created {
    parent: OwnedFd,
    name: OsString,
    dir: OwnedFd,
}

impl CreatedDirs {
    /// The caller checks the pinned existing ancestor before any new directory is made.
    pub(super) fn create(&mut self, mut parent: OwnedFd, missing: Vec<OsString>) -> io::Result<OwnedFd> {
        let path_flags = OFlags::PATH | OFlags::DIRECTORY | OFlags::NOFOLLOW | OFlags::CLOEXEC;
        for name in missing.into_iter().rev() {
            let fresh = match mkdirat(&parent, &name, Mode::from_raw_mode(0o777)) {
                Ok(()) => true,
                Err(Errno::EXIST) => false,
                Err(e) => return Err(e.into()),
            };
            let dir = openat(&parent, &name, path_flags, Mode::empty())?;
            if fresh {
                self.0.push(Created { parent: parent.try_clone()?, name, dir: dir.try_clone()? });
                sync(&parent)?;
            }
            parent = dir;
        }
        openat(parent, ".", DIRECTORY, Mode::empty()).map_err(Into::into)
    }

    pub(super) fn keep(&mut self) {
        self.0.clear();
    }
}

impl Drop for CreatedDirs {
    fn drop(&mut self) {
        // If a created directory was moved, leave its subtree alone. For a directory still
        // in place, rmdir fails if anyone added contents, including another directory.
        let attached = self.0.iter().take_while(|entry| entry.still_at_name()).count();
        for entry in self.0[..attached].iter().rev() {
            if entry.still_at_name() && unlinkat(&entry.parent, &entry.name, AtFlags::REMOVEDIR).is_ok() {
                let _ = sync(&entry.parent);
            }
        }
    }
}

impl Created {
    fn still_at_name(&self) -> bool {
        let at_name = statat(&self.parent, &self.name, AtFlags::SYMLINK_NOFOLLOW);
        match (at_name, fstat(&self.dir)) {
            (Ok(named), Ok(pinned)) => same_inode(&named, &pinned),
            _ => false,
        }
    }
}

const DIRECTORY: OFlags = OFlags::RDONLY.union(OFlags::DIRECTORY).union(OFlags::CLOEXEC);

fn sync(dir: &OwnedFd) -> io::Result<()> {
    fsync(openat(dir, ".", DIRECTORY, Mode::empty())?).map_err(Into::into)
}

pub(super) fn still_named(path: &Path, dir: impl AsFd) -> io::Result<bool> {
    let (named, missing) = directory_prefix(path)?;
    Ok(missing.is_empty() && same_inode(&fstat(named)?, &fstat(dir)?))
}
