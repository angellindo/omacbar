//! The guarded write shared by saves, first writes, and restores.

use std::fs;
use std::io;
use std::path::Path;

use super::disk::{self, DirLock, Found, write_atomically, write_backup};
use super::{ConfigFile, ConfigFileError, Content, DiskVersion, NEW_FILE_MODE, WriteKind, dir_of, refused};
use crate::backup::{Backups, GenerationId, Kept, Locked, Reason};
use crate::durable::create_dir_all_durably;

impl ConfigFile {
    /// Keep what is being replaced and recheck it after staging the replacement.
    pub(super) fn swap_as(
        &mut self,
        expected: &DiskVersion,
        content: Content<'_>,
        kind: WriteKind,
        reason: Reason,
    ) -> Result<(), ConfigFileError> {
        // Protected writes never follow the final symlink, even at the initial, unlocked check.
        let store = if let Some(backups) = &self.protection {
            check_version(self.probe()?.into_version(), expected)?;
            Some(backups.lock(&self.path)?)
        } else {
            check_version(self.version_now()?, expected)?;
            None
        };
        let dir = dir_of(&self.path);
        let (lock, found, generation) = if let (Some(backups), Some(locked)) = (&self.protection, &store) {
            let (lock, found, generation) = self.prepare_protected(backups, locked, expected, reason)?;
            (lock, Some(found), Some(generation))
        } else {
            let lock = create_and_lock(dir)?;
            check_version(self.version_now()?, expected)?;
            if kind == WriteKind::Overwrite
                && fs::symlink_metadata(&self.path).is_ok_and(|m| m.file_type().is_symlink())
            {
                return Err(ConfigFileError::Symlink { path: self.path.clone() });
            }
            (lock, None, None)
        };
        let kept = match (&expected.0, kind) {
            (Some(bytes), WriteKind::Overwrite) => {
                Some(write_backup(&self.path, bytes).map_err(|e| io_at(dir, e))?)
            }
            _ => None,
        };
        let written = self.put(expected, content, found.as_ref());
        if written.is_err()
            && let Some(kept) = &kept
        {
            // The copy is only a backup if the write replaced the file.
            let _ = fs::remove_file(kept);
        }
        written?;
        // The rename already happened, so reporting this as a refused save would be wrong.
        let _ = lock.sync();
        self.seen = DiskVersion(content.bytes().map(<[u8]>::to_vec));
        if kind == WriteKind::Overwrite {
            self.backup = kept;
        }
        if generation.is_some() {
            self.last_generation = generation;
        }
        Ok(())
    }

    /// The store lock serializes protected first writes while the config directory is absent.
    /// Keep the absence before creating any config directories, so a failed generation leaves
    /// an originally absent hierarchy untouched.
    fn prepare_protected(
        &self,
        backups: &Backups,
        locked: &Locked,
        expected: &DiskVersion,
        reason: Reason,
    ) -> Result<(DirLock, Found, GenerationId), ConfigFileError> {
        let dir = dir_of(&self.path);
        let mut lock = match DirLock::take(dir) {
            Ok(lock) => Some(lock),
            Err(e) if e.kind() == io::ErrorKind::NotFound => None,
            Err(e) => return Err(io_at(dir, e)),
        };
        let mut found = self.probe()?;
        // An external writer can create the directory between the first lock attempt and probe.
        if lock.is_none() && matches!(found, Found::File { .. }) {
            lock = Some(DirLock::take(dir).map_err(|e| io_at(dir, e))?);
            found = self.probe()?;
        }
        if found.bytes() != expected.0.as_deref() {
            return Err(ConfigFileError::Changed(found.into_version()));
        }
        let kept = match &found {
            Found::Absent => Kept::Absent { dir_existed: lock.is_some() },
            Found::File { bytes, mode } => Kept::File { bytes, mode: *mode },
        };
        let id = backups.keep(locked, &self.path, &kept, reason)?;
        let lock = match lock {
            Some(lock) => lock,
            None => create_and_lock(dir)?,
        };
        Ok((lock, found, id))
    }

    pub(super) fn probe(&self) -> Result<Found, ConfigFileError> {
        disk::probe(&self.path).map_err(|why| refused(&self.path, why))
    }

    /// Recheck the backed-up bytes and mode, as well as whether the path is safe to replace,
    /// immediately before the rename (or removal). Unprotected saves still compare bytes.
    fn put(
        &self,
        expected: &DiskVersion,
        content: Content<'_>,
        kept: Option<&Found>,
    ) -> Result<(), ConfigFileError> {
        let mode = kept.map(|found| match found {
            Found::Absent => NEW_FILE_MODE,
            Found::File { mode, .. } => mode & 0o777,
        });
        let recheck = || {
            #[cfg(test)]
            crate::recovery_regressions::before_config_replace();
            if let Some(kept) = kept {
                let now = self.probe()?;
                return if now == *kept { Ok(()) } else { Err(ConfigFileError::Changed(now.into_version())) };
            }
            check_version(self.version_now()?, expected)
        };
        match content {
            Content::Text(text) => write_atomically(&self.path, text.as_bytes(), mode, io_at, recheck),
            Content::Exact { bytes, mode } => write_atomically(&self.path, bytes, Some(mode), io_at, recheck),
            Content::Remove => {
                recheck().and_then(|()| fs::remove_file(&self.path).map_err(|e| io_at(&self.path, e)))
            }
        }
    }
}

fn check_version(current: DiskVersion, expected: &DiskVersion) -> Result<(), ConfigFileError> {
    if current == *expected { Ok(()) } else { Err(ConfigFileError::Changed(current)) }
}

fn create_and_lock(dir: &Path) -> Result<DirLock, ConfigFileError> {
    create_dir_all_durably(dir, 0o777).map_err(|e| io_at(dir, e))?;
    DirLock::take(dir).map_err(|e| io_at(dir, e))
}

fn io_at(path: &Path, source: io::Error) -> ConfigFileError {
    ConfigFileError::Io { path: path.to_path_buf(), source }
}
