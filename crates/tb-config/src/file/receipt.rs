//! Save receipts are auxiliary outputs. They never write to the generation store.

use std::io;
use std::path::Path;

use rustix::fs::{CWD, fstat};

use crate::durable::mounts::Mounts;
use crate::durable::{directory_prefix, is_inside, open_directory, same_inode};
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
            && same_inode(&fstat(&dir)?, &fstat(open_directory(CWD, dir_of(&self.path))?)?)
        {
            return Err(invalid("save receipt would replace the config"));
        }
        if let Some(backups) = self.backups() {
            let (store, missing) = directory_prefix(backups.dir())?;
            let mounts = Mounts::read()?;
            let inside = missing.is_empty() && is_inside(&dir, &fstat(&store)?)?;
            let store = mounts.tree(&store, &missing)?;
            if inside || store.contains(&mounts.tree(&dir, &[])?) {
                return Err(invalid("save receipt would write inside the generation store"));
            }
            if mounts.tree(&dir, &[name.to_os_string()])?.contains(&store) {
                return Err(invalid("save receipt would occupy the generation store's path"));
            }
        }
        let receipt = serde_json::json!({"config": config, "sha256": format!("{:x}", Sha256::digest(bytes))});
        // Use the checked directory descriptor, so swapping a symlink after the check
        // cannot redirect the temporary file or the published receipt into the store.
        write_new_atomically(dir, Path::new(name), receipt.to_string().as_bytes(), NEW_FILE_MODE)
    }
}

fn invalid(message: &str) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidInput, message)
}
