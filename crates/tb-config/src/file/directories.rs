//! Config directories created by one protected write, retained only after it succeeds.

use std::ffi::{OsStr, OsString};
use std::io;
use std::os::fd::{AsFd, OwnedFd};
use std::path::Path;

use rustix::fs::{AtFlags, Mode, OFlags, RenameFlags, Stat, fstat, fsync, mkdirat, openat, statat, unlinkat};
use rustix::io::Errno;
use rustix::rand::{GetRandomFlags, getrandom};

use crate::durable::mounts::same_directory;
use crate::durable::{directory_prefix, same_inode};

#[derive(Default)]
pub(super) struct CreatedDirs(Vec<Created>);

struct Created {
    parent: OwnedFd,
    name: OsString,
    inode: Stat,
    pin: Option<OwnedFd>,
}

impl CreatedDirs {
    /// The caller checks the pinned existing ancestor before any new directory is made.
    pub(super) fn create(&mut self, mut parent: OwnedFd, missing: Vec<OsString>) -> io::Result<OwnedFd> {
        for name in missing.into_iter().rev() {
            let mut staged = Created::stage(parent)?;
            parent = staged.publish(name)?;
            let synced = sync(&staged.parent);
            self.0.push(staged);
            synced?;
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
            entry.remove();
        }
    }
}

impl Created {
    fn stage(parent: OwnedFd) -> io::Result<Self> {
        // Use the same parent so the new directory inherits its normal mode, group and ACL.
        // A concurrent mkdir at the final name cannot cause us to pin or remove that directory.
        loop {
            let mut bytes = [0; 16];
            if getrandom(&mut bytes[..], GetRandomFlags::empty())? != bytes.len() {
                return Err(io::Error::other("cannot get a private staging name"));
            }
            let name = OsString::from(format!(".tb-config-{:032x}", u128::from_ne_bytes(bytes)));
            match mkdirat(&parent, &name, Mode::from_raw_mode(0o777)) {
                Ok(()) => {
                    // Record identity without another descriptor: openat can still fail with EMFILE.
                    let inode = statat(&parent, &name, AtFlags::SYMLINK_NOFOLLOW)?;
                    return Ok(Self { parent, name, inode, pin: None });
                }
                Err(Errno::EXIST) => {}
                Err(e) => return Err(e.into()),
            }
        }
    }

    fn publish(&mut self, name: OsString) -> io::Result<OwnedFd> {
        match self.pin_and_publish(&name) {
            Ok(dir) => {
                self.name = name;
                if !self.still_at_name() {
                    return Err(io::Error::other("the published config directory changed"));
                }
                Ok(dir)
            }
            Err(e) => {
                self.remove();
                Err(e)
            }
        }
    }

    fn pin_and_publish(&mut self, name: &OsStr) -> io::Result<OwnedFd> {
        let path_flags = OFlags::PATH | OFlags::DIRECTORY | OFlags::NOFOLLOW | OFlags::CLOEXEC;
        #[cfg(test)]
        crate::recovery_regressions::write_identity::config_directory_fd(
            &self.parent,
            name,
            &self.name,
            false,
        )?;
        let dir = openat(&self.parent, &self.name, path_flags, Mode::empty())?;
        if !same_inode(&fstat(&dir)?, &self.inode) {
            return Err(io::Error::other("the staged config directory changed"));
        }
        let pinned = self.pin.insert(dir);
        #[cfg(test)]
        crate::recovery_regressions::write_identity::config_directory_fd(
            &self.parent,
            name,
            &self.name,
            true,
        )?;
        let child = pinned.try_clone()?;
        if !self.still_at_name() {
            return Err(io::Error::other("the staged config directory changed"));
        }
        rustix::fs::renameat_with(&self.parent, &self.name, &self.parent, name, RenameFlags::NOREPLACE)?;
        Ok(child)
    }

    fn remove(&self) {
        if self.still_at_name() && unlinkat(&self.parent, &self.name, AtFlags::REMOVEDIR).is_ok() {
            let _ = sync(&self.parent);
        }
    }

    fn still_at_name(&self) -> bool {
        let Ok(named) = statat(&self.parent, &self.name, AtFlags::SYMLINK_NOFOLLOW) else {
            return false;
        };
        match &self.pin {
            Some(dir) => fstat(dir).is_ok_and(|pinned| same_inode(&named, &pinned)),
            None => same_inode(&named, &self.inode),
        }
    }
}

const DIRECTORY: OFlags = OFlags::RDONLY.union(OFlags::DIRECTORY).union(OFlags::CLOEXEC);

fn sync(dir: &OwnedFd) -> io::Result<()> {
    fsync(openat(dir, ".", DIRECTORY, Mode::empty())?).map_err(Into::into)
}

pub(super) fn still_named(path: &Path, dir: impl AsFd) -> io::Result<bool> {
    let (named, missing) = directory_prefix(path)?;
    Ok(missing.is_empty() && same_directory(named, dir)?)
}
