//! The guarded write shared by saves, first writes, and restores.

use std::fs;
use std::io;
use std::os::fd::AsFd;
use std::path::{Path, PathBuf};

use rustix::fs::{AtFlags, CWD, Mode, OFlags, openat, unlinkat};

use super::directories::{CreatedDirs, still_named};
use super::disk::{self, DirLock, Found, Replacement, remove_owned, write_backup};
use super::{ConfigFile, ConfigFileError, Content, DiskVersion, NEW_FILE_MODE, WriteKind, dir_of, refused};
use crate::backup::{Backups, GenerationId, Kept, Locked, Reason};
use crate::durable::{create_dir_all_durably, directory_prefix};

impl ConfigFile {
    pub(super) fn swap_as(
        &mut self,
        expected: &DiskVersion,
        content: Content<'_>,
        kind: WriteKind,
        reason: Reason,
    ) -> Result<(), ConfigFileError> {
        let (backup, generation) = if let Some(backups) = &self.protection {
            check_version(self.probe()?.into_version(), expected)?;
            let locked = backups.lock(&self.path)?;
            let protected = Protected { file: self, backups, locked: &locked };
            let (backup, id) = protected.write(expected, content, kind, reason)?;
            (backup, Some(id))
        } else {
            (self.write_unprotected(expected, content, kind)?, None)
        };
        self.seen = DiskVersion(content.bytes().map(<[u8]>::to_vec));
        if kind == WriteKind::Overwrite {
            self.backup = backup;
        }
        self.last_generation = generation;
        Ok(())
    }

    fn write_unprotected(
        &self,
        expected: &DiskVersion,
        content: Content<'_>,
        kind: WriteKind,
    ) -> Result<Option<PathBuf>, ConfigFileError> {
        check_version(self.version_now()?, expected)?;
        let dir = dir_of(&self.path);
        create_dir_all_durably(dir, 0o777).map_err(|e| io_at(dir, e))?;
        let lock = DirLock::take(dir).map_err(|e| io_at(dir, e))?;
        check_version(self.version_now()?, expected)?;
        if kind == WriteKind::Overwrite
            && fs::symlink_metadata(&self.path).is_ok_and(|m| m.file_type().is_symlink())
        {
            return Err(ConfigFileError::Symlink { path: self.path.clone() });
        }
        let backup = LegacyBackup::keep(CWD, &self.path, expected, kind).map_err(|e| io_at(dir, e))?;
        let staged = stage(CWD, &self.path, content, None).map_err(|e| io_at(dir, e))?;
        check_version(self.version_now()?, expected)?;
        put(CWD, &self.path, staged).map_err(|e| self.io(e))?;
        let _ = lock.sync();
        Ok(backup.and_then(LegacyBackup::commit))
    }

    pub(super) fn probe(&self) -> Result<Found, ConfigFileError> {
        disk::probe(&self.path).map_err(|why| refused(&self.path, why))
    }
}

struct Protected<'a> {
    file: &'a ConfigFile,
    backups: &'a Backups,
    locked: &'a Locked,
}

impl Protected<'_> {
    fn write(
        &self,
        expected: &DiskVersion,
        content: Content<'_>,
        kind: WriteKind,
        reason: Reason,
    ) -> Result<(Option<PathBuf>, GenerationId), ConfigFileError> {
        let path = self.file.path();
        let dir = dir_of(path);
        let (ancestor, missing) = directory_prefix(dir).map_err(|e| io_at(dir, e))?;
        self.backups.check_config_directory(dir, &ancestor, &missing, self.locked)?;
        let mut created = CreatedDirs::default();
        let (lock, kept_absence) = if missing.is_empty() {
            let fd =
                openat(&ancestor, ".", OFlags::RDONLY | OFlags::DIRECTORY | OFlags::CLOEXEC, Mode::empty())
                    .map_err(|e| io_at(dir, e.into()))?;
            (DirLock::from_dir(fd).map_err(|e| io_at(dir, e))?, None)
        } else {
            let recheck = || {
                self.backups.check_location(path)?;
                check_version(self.file.probe()?.into_version(), expected)
            };
            recheck()?;
            let id = self.backups.keep(
                self.locked,
                path,
                &Kept::Absent { dir_existed: false },
                reason,
                recheck,
            )?;
            let open = || {
                self.backups.check_config_directory(dir, &ancestor, &missing, self.locked)?;
                let fd = created.create(ancestor, missing).map_err(|e| io_at(dir, e))?;
                self.backups.check_config_directory(dir, &fd, &[], self.locked)?;
                DirLock::from_dir(fd).map_err(|e| io_at(dir, e))
            };
            (open().map_err(|e| after_keep(e, Some(id)))?, Some(id))
        };
        let outcome = self.replace(&lock, expected, content, kind, reason, kept_absence);
        if outcome.is_ok() {
            created.keep();
            let _ = lock.sync();
        }
        drop(created);
        outcome.map(|(backup, id)| (backup.map(|name| dir.join(name)), id))
    }

    fn replace(
        &self,
        lock: &DirLock,
        expected: &DiskVersion,
        content: Content<'_>,
        kind: WriteKind,
        reason: Reason,
        kept_absence: Option<GenerationId>,
    ) -> Result<(Option<PathBuf>, GenerationId), ConfigFileError> {
        let fd = lock.fd();
        let path = self.file.path();
        let name = Path::new(
            path.file_name().ok_or_else(|| self.file.io(io::Error::from(io::ErrorKind::InvalidInput)))?,
        );
        let dir = dir_of(path);
        let probe = || self.probe_locked(lock, name);
        let found = probe().map_err(|e| after_keep(e, kept_absence))?;
        if found.bytes() != expected.0.as_deref() {
            return Err(after_keep(ConfigFileError::Changed(found.into_version()), kept_absence));
        }
        let mode = match &found {
            Found::Absent => NEW_FILE_MODE,
            Found::File { mode, .. } => mode & 0o777,
        };
        let staged =
            stage(fd, name, content, Some(mode)).map_err(|e| after_keep(io_at(dir, e), kept_absence))?;
        let backup = LegacyBackup::keep(fd, name, expected, kind).map_err(|e| io_at(dir, e))?;
        let recheck = || {
            if let Some(replacement) = &staged {
                replacement.check_name().map_err(|e| self.file.io(e))?;
            }
            let now = probe()?;
            if now == found { Ok(()) } else { Err(ConfigFileError::Changed(now.into_version())) }
        };
        let id = if let Some(id) = kept_absence {
            id
        } else {
            let kept = match &found {
                Found::Absent => Kept::Absent { dir_existed: true },
                Found::File { bytes, mode } => Kept::File { bytes, mode: *mode },
            };
            self.backups.keep(self.locked, self.file.path(), &kept, reason, recheck)?
        };
        #[cfg(test)]
        crate::recovery_regressions::before_config_replace();
        recheck().map_err(|e| after_keep(e, Some(id)))?;
        put(fd, name, staged).map_err(|e| after_keep(self.file.io(e), Some(id)))?;
        Ok((backup.and_then(LegacyBackup::commit), id))
    }

    fn probe_locked(&self, lock: &DirLock, name: &Path) -> Result<Found, ConfigFileError> {
        let path = self.file.path();
        let dir = dir_of(path);
        let fd = lock.fd();
        self.backups.check_config_directory(dir, fd, &[], self.locked)?;
        if !still_named(dir, fd).map_err(|e| io_at(dir, e))? {
            return Err(io_at(dir, io::Error::other("the config directory changed during the write")));
        }
        disk::probe_at(fd, name).map_err(|why| refused(path, why))
    }
}

fn after_keep(err: ConfigFileError, id: Option<GenerationId>) -> ConfigFileError {
    match (err, id) {
        (ConfigFileError::Changed(current), Some(generation)) => {
            ConfigFileError::ChangedAfterKeep { current, generation }
        }
        (source, Some(generation)) => {
            ConfigFileError::RefusedAfterKeep { generation, source: Box::new(source) }
        }
        (source, None) => source,
    }
}

fn stage<'a>(
    dir: rustix::fd::BorrowedFd<'a>,
    path: &Path,
    content: Content<'_>,
    mode: Option<u32>,
) -> io::Result<Option<Replacement<'a>>> {
    match content {
        Content::Text(text) => Replacement::stage(dir, path, text.as_bytes(), mode).map(Some),
        Content::Exact { bytes, mode } => Replacement::stage(dir, path, bytes, Some(mode)).map(Some),
        Content::Remove => Ok(None),
    }
}

fn put(dir: impl AsFd, path: &Path, staged: Option<Replacement<'_>>) -> io::Result<()> {
    match staged {
        Some(staged) => staged.put(path),
        None => unlinkat(dir, path, AtFlags::empty()).map_err(Into::into),
    }
}

struct LegacyBackup<'a> {
    dir: rustix::fd::BorrowedFd<'a>,
    entry: Option<(PathBuf, fs::File)>,
}

impl<'a> LegacyBackup<'a> {
    fn keep(
        dir: rustix::fd::BorrowedFd<'a>,
        path: &Path,
        expected: &DiskVersion,
        kind: WriteKind,
    ) -> io::Result<Option<Self>> {
        match (&expected.0, kind) {
            (Some(bytes), WriteKind::Overwrite) => {
                Ok(Some(Self { dir, entry: Some(write_backup(dir, path, bytes)?) }))
            }
            _ => Ok(None),
        }
    }

    fn commit(mut self) -> Option<PathBuf> {
        self.entry.take().map(|(path, _)| path)
    }
}

impl Drop for LegacyBackup<'_> {
    fn drop(&mut self) {
        if let Some((path, file)) = &self.entry {
            remove_owned(self.dir, path, file);
        }
    }
}

fn check_version(current: DiskVersion, expected: &DiskVersion) -> Result<(), ConfigFileError> {
    if current == *expected { Ok(()) } else { Err(ConfigFileError::Changed(current)) }
}

fn io_at(path: &Path, source: io::Error) -> ConfigFileError {
    ConfigFileError::Io { path: path.to_path_buf(), source }
}
