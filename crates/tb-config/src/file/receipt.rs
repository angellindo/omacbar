//! Save receipts are auxiliary outputs. They never write to the generation store.

use std::io;
use std::os::fd::{AsFd, OwnedFd};
use std::path::Path;

use rustix::fs::{CWD, Mode, OFlags, Stat, fstat, openat};
use sha2::{Digest, Sha256};

use super::disk::write_new_atomically;
use super::{ConfigFile, NEW_FILE_MODE, dir_of};

impl ConfigFile {
    /// Publishes a receipt immediately after a successful explicit save. The recorded version
    /// is the buffer passed to the swap, so another writer cannot change the reported digest.
    pub fn write_save_receipt(&self, path: &Path) -> io::Result<()> {
        let bytes = self.seen.0.as_deref().ok_or_else(|| io::Error::other("no saved config bytes"))?;
        let config = self.path.to_str().ok_or_else(|| io::Error::other("config path is not UTF-8"))?;
        let name = path.file_name().ok_or_else(|| invalid("save receipt needs a file name"))?;
        let dir = open_directory(CWD, dir_of(path))?;
        if Some(name) == self.path.file_name()
            && same_directory(&fstat(&dir)?, &fstat(open_directory(CWD, dir_of(&self.path))?)?)
        {
            return Err(invalid("save receipt would replace the config"));
        }
        if let Some(backups) = self.backups() {
            match open_directory(CWD, backups.dir()) {
                Ok(store) if is_inside(&dir, &fstat(&store)?)? => {
                    return Err(invalid("save receipt would write inside the generation store"));
                }
                Ok(_) => {}
                Err(e) if e.kind() == io::ErrorKind::NotFound => {}
                Err(e) => return Err(e),
            }
        }
        let receipt = serde_json::json!({"config": config, "sha256": format!("{:x}", Sha256::digest(bytes))});
        // Use the checked directory descriptor, so swapping a symlink after the check
        // cannot redirect the temporary file or the published receipt into the store.
        write_new_atomically(dir, Path::new(name), receipt.to_string().as_bytes(), NEW_FILE_MODE)
    }
}

fn is_inside(dir: &OwnedFd, store: &Stat) -> io::Result<bool> {
    let mut dir = dir.try_clone()?;
    let mut stat = fstat(&dir)?;
    loop {
        if same_directory(&stat, store) {
            return Ok(true);
        }
        dir = open_directory(&dir, Path::new(".."))?;
        let parent = fstat(&dir)?;
        if same_directory(&stat, &parent) {
            return Ok(false);
        }
        stat = parent;
    }
}

fn same_directory(left: &Stat, right: &Stat) -> bool {
    left.st_dev == right.st_dev && left.st_ino == right.st_ino
}

fn open_directory(dir: impl AsFd, path: &Path) -> io::Result<OwnedFd> {
    let flags = OFlags::PATH | OFlags::DIRECTORY | OFlags::CLOEXEC;
    openat(dir, path, flags, Mode::empty()).map_err(Into::into)
}

fn invalid(message: &str) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidInput, message)
}
