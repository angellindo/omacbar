//! Restoring a chosen generation (ADR-0015): one more protected compare-and-swap. It replaces
//! only the version this config file last saw, first keeps a `recover` generation of what it
//! replaces, and writes the kept bytes with the kept permission bits.

use super::{ConfigFile, ConfigFileError, Content, WriteKind};
use crate::backup::{BackupError, GenerationId, Stored};

impl ConfigFile {
    /// Records what is on disk now as the version seen, without parsing it: a missing or invalid
    /// file is what a restore may replace too. A symlink is followed, as reads do (a later write
    /// still refuses it).
    pub fn look(&mut self) -> Result<(), ConfigFileError> {
        self.seen = self.version_now()?;
        Ok(())
    }

    /// Restores generation `id` of this file: exact bytes and permission bits, creating a missing
    /// file or directory. Refused before anything changes when the generation does not verify,
    /// belongs to another config, records that no file existed (see [`Self::recover_removing`]),
    /// or the file is not the version last seen (`Changed`).
    pub fn recover(&mut self, id: &GenerationId) -> Result<(), ConfigFileError> {
        self.restore(*id, false)
    }

    /// Like `recover`, and a generation that records no file is restored by removing the config
    /// file (after keeping it); its directory is never removed.
    pub fn recover_removing(&mut self, id: &GenerationId) -> Result<(), ConfigFileError> {
        self.restore(*id, true)
    }

    fn restore(&mut self, id: GenerationId, remove: bool) -> Result<(), ConfigFileError> {
        let backups = self
            .protection
            .clone()
            .ok_or_else(|| ConfigFileError::Unprotected { path: self.path.clone() })?;
        let (_, stored) = backups.stored(id, &self.path)?;
        let seen = self.seen.clone();
        match stored {
            Stored::File { bytes, mode } => {
                self.swap(&seen, Content::Exact { bytes: &bytes, mode }, WriteKind::Recover)
            }
            Stored::Absent if !remove => Err(BackupError::Absence { id }.into()),
            // Already no file: nothing to replace, so nothing to keep, unless one appeared.
            Stored::Absent if seen.is_absent() => {
                let now = self.version_now()?;
                if now.is_absent() { Ok(()) } else { Err(ConfigFileError::Changed(now)) }
            }
            Stored::Absent => self.swap(&seen, Content::Remove, WriteKind::Recover),
        }
    }
}
