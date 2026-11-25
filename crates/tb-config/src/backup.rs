//! Generations (ADR-0015): before a protected write replaces the config file, a private,
//! verified, durable copy of what it replaces (the exact bytes and permission bits, or the fact
//! that no file was there) is kept in the user's state directory. Generations are never deleted
//! by this library; an unchanged save reuses the newest verified one instead of adding a copy.

mod manifest;
mod store;

use std::ffi::{OsStr, OsString};
use std::fmt;
use std::fs::File;
use std::io;
use std::os::fd::AsFd;
use std::path::{Path, PathBuf};

use rustix::fs::fstat;

use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::durable::mounts::{Mounts, same_directory};
use crate::durable::{directory_prefix, is_inside, parent_of, resolved};
use manifest::{FORMAT, Kind, Manifest};
use store::{Inspected, StoreDir};

/// The generation store of one user: the `backups` directory itself.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Backups {
    dir: PathBuf,
}

/// A generation's name in the store: a positive number, allocated in increasing order.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct GenerationId(u64);

impl GenerationId {
    /// The id a store directory name (or a user's argument) stands for: a positive number in
    /// digits only, no sign, space or leading zero. Anything else names no generation.
    pub fn parse(name: &str) -> Option<Self> {
        let n: u64 = name.parse().ok()?;
        (n > 0 && n.to_string() == name).then_some(Self(n))
    }
}

impl fmt::Display for GenerationId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// Which write kept a generation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Reason {
    /// The first write of a missing config file (the default written on open).
    FirstWrite,
    /// A save, or a save resolved against its theme source.
    Save,
    /// An overwrite the user acknowledged after a conflict.
    Overwrite,
    /// A restore of a chosen generation (`ConfigFile::recover`).
    Recover,
    /// A save of the factory layout the configurator loaded as a draft (a factory reset).
    FactoryReset,
}

/// What a generation holds.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GenerationKind {
    /// The exact bytes and permission bits of the replaced file.
    File,
    /// No file was at the path; `dir_existed` tells whether its directory was.
    Absent { dir_existed: bool },
}

/// Whether a generation still holds what its manifest records.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Health {
    Verified,
    /// Why it cannot be trusted (edited, truncated, a symlink, ...). It is kept, never reused.
    Damaged(String),
}

/// One generation of one config file, as listed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Generation {
    pub id: GenerationId,
    pub reason: Reason,
    pub kind: GenerationKind,
    /// Permission bits of the kept file (0 for an absence).
    pub mode: u32,
    /// Length of the kept bytes (0 for an absence).
    pub len: u64,
    pub health: Health,
}

#[derive(Debug, Error)]
pub enum BackupError {
    #[error("generation store `{}`: {source}", path.display())]
    Io { path: PathBuf, source: io::Error },
    /// The store cannot be trusted with private copies; nothing was written.
    #[error("generation store `{}` is refused: {why}; {fix}", path.display())]
    Unsafe { path: PathBuf, why: String, fix: String },
    #[error("generation {id} in `{}` is damaged: {why}", store.display())]
    Damaged { store: PathBuf, id: GenerationId, why: String },
    #[error("there is no generation {id} in `{}`", store.display())]
    NotFound { store: PathBuf, id: GenerationId },
    /// The generation is sound but was kept for another config file.
    #[error(
        "generation {id} belongs to `{}`, not to `{}`; it is never restored here",
        belongs_to.display(),
        config.display()
    )]
    Foreign { id: GenerationId, belongs_to: PathBuf, config: PathBuf },
    /// The generation records that no config file existed; restoring it removes the file, which
    /// only an explicit removal does.
    #[error("generation {id} records that there was no config file; restoring it removes the file")]
    Absence { id: GenerationId },
}

/// What a verified generation of one config holds, ready to be restored.
pub(crate) enum Stored {
    File { bytes: Vec<u8>, mode: u32 },
    Absent,
}

/// The store opened for writing and exclusively locked, held across one whole protected write.
/// Every protected write takes it before looking at or creating the config directory (and before
/// the directory lock), so two first writes of one profile never interleave: the one that keeps
/// the absence records whether the directory existed before either of them created it.
pub(crate) struct Locked {
    store: StoreDir,
    _lock: File,
}

/// What a protected write is about to replace.
pub(crate) enum Kept<'a> {
    File { bytes: &'a [u8], mode: u32 },
    Absent { dir_existed: bool },
}

impl Backups {
    /// The store at `dir` (the `backups` directory). Touches nothing on disk.
    pub fn at(dir: impl Into<PathBuf>) -> Self {
        Self { dir: dir.into() }
    }

    pub fn dir(&self) -> &Path {
        &self.dir
    }

    /// `<state>/touchbar-customizer/backups`, where `<state>` is `xdg_state_home` when it is
    /// absolute and otherwise `<home>/.local/state` (the XDG rule: a relative or empty value is
    /// ignored). `None` when neither gives a directory. Pure: callers pass what the binary read
    /// at startup.
    pub fn locate(home: Option<&Path>, xdg_state_home: Option<&OsStr>) -> Option<PathBuf> {
        let state = xdg_state_home
            .map(Path::new)
            .filter(|p| p.is_absolute())
            .map(Path::to_path_buf)
            .or_else(|| home.map(|h| h.join(".local").join("state")))?;
        Some(state.join("touchbar-customizer").join("backups"))
    }

    /// Every generation of `config`, newest first, each checked against its manifest. A store
    /// that does not exist yet lists nothing. Generations with unreadable directories or
    /// manifests stay in the store and are omitted from this list. See
    /// [`Self::list_with_unreadable`] for their diagnostics.
    pub fn list(&self, config: &Path) -> Result<Vec<Generation>, BackupError> {
        self.list_with_unreadable(config, |_, _| ())
    }

    /// Also reports store entries whose manifest cannot tell which config they represent.
    /// These ids are diagnostic only and do not belong to the requested config's history.
    pub fn list_with_unreadable(
        &self,
        config: &Path,
        mut unreadable: impl FnMut(GenerationId, String),
    ) -> Result<Vec<Generation>, BackupError> {
        let Some(store) = StoreDir::open(&self.dir)? else { return Ok(Vec::new()) };
        let config = absolute(config);
        let mut out = Vec::new();
        for id in store.ids()?.into_iter().rev() {
            match store.inspect(id) {
                Inspected::Read { manifest, kind, health, .. } if manifest.config == config => {
                    out.push(generation(id, &manifest, kind, health));
                }
                Inspected::Unreadable(why) => unreadable(id, why),
                _ => {}
            }
        }
        Ok(out)
    }

    /// Temp directories an interrupted write left in the store: incomplete, never a generation,
    /// kept for the user to inspect. None when there is no store.
    pub fn incomplete(&self) -> Result<Vec<String>, BackupError> {
        StoreDir::open(&self.dir)?.map_or_else(|| Ok(Vec::new()), |store| store.temps())
    }

    /// Checks one generation; `Damaged` says why it cannot be trusted.
    pub fn verify(&self, id: &GenerationId) -> Result<Generation, BackupError> {
        self.read(*id).map(|(generation, ..)| generation)
    }

    /// Checks that generation `id` verifies and was kept for `config`: whether
    /// `ConfigFile::recover` can restore it there. Reads only; never creates the store.
    pub fn verify_for(&self, id: &GenerationId, config: &Path) -> Result<Generation, BackupError> {
        self.stored(*id, config).map(|(generation, _)| generation)
    }

    /// The verified generation `id` of `config` and what it holds.
    pub(crate) fn stored(
        &self,
        id: GenerationId,
        config: &Path,
    ) -> Result<(Generation, Stored), BackupError> {
        let (generation, manifest, data) = self.read(id)?;
        let config = absolute(config);
        if manifest.config != config {
            return Err(BackupError::Foreign { id, belongs_to: manifest.config, config });
        }
        let stored = match (generation.kind, data) {
            (GenerationKind::Absent { .. }, _) => Stored::Absent,
            (GenerationKind::File, Some(bytes)) => Stored::File { bytes, mode: manifest.mode },
            (GenerationKind::File, None) => {
                return Err(self.damaged(id, format!("it holds no {}", store::DATA)));
            }
        };
        Ok((generation, stored))
    }

    /// Generation `id` read back and verified: as listed, its manifest and its kept bytes.
    fn read(&self, id: GenerationId) -> Result<(Generation, Manifest, Option<Vec<u8>>), BackupError> {
        let not_found = || BackupError::NotFound { store: self.dir.clone(), id };
        let store = StoreDir::open(&self.dir)?.ok_or_else(not_found)?;
        match store.inspect(id) {
            Inspected::Missing => Err(not_found()),
            Inspected::Unreadable(why) | Inspected::Read { health: Health::Damaged(why), .. } => {
                Err(self.damaged(id, why))
            }
            Inspected::Read { manifest, kind, health, data } => {
                Ok((generation(id, &manifest, kind, health), manifest, data))
            }
        }
    }

    fn damaged(&self, id: GenerationId, why: String) -> BackupError {
        BackupError::Damaged { store: self.dir.clone(), id, why }
    }

    /// Neither directory can contain the other. Check before either tree is created, also
    /// resolving aliases to the nearest existing ancestor if the destination is absent.
    pub(crate) fn check_location(&self, config: &Path) -> Result<(), BackupError> {
        let dir = parent_of(config);
        for (store, config_dir) in
            [(absolute(&self.dir), absolute(dir)), (resolved(&self.dir), resolved(dir))]
        {
            if store.starts_with(&config_dir) {
                return Err(self.overlap(dir, false));
            }
            if config_dir.starts_with(&store) {
                return Err(self.overlap(dir, true));
            }
        }
        let (config_fd, config_tail) = directory_prefix(dir).map_err(|e| self.io(e))?;
        let (store_fd, store_tail) = match directory_prefix(&self.dir) {
            Ok(prefix) => prefix,
            // StoreDir reports an existing non-directory with the appropriate refusal.
            Err(e) if e.kind() == io::ErrorKind::NotADirectory => return Ok(()),
            Err(e) => return Err(self.io(e)),
        };
        self.check_directories(dir, &config_fd, &config_tail, &store_fd, &store_tail)
    }

    fn overlap(&self, dir: &Path, config_inside: bool) -> BackupError {
        let why = if config_inside {
            format!("the config directory `{}` is inside the generation store", dir.display())
        } else {
            format!("it is inside the config directory `{}`", dir.display())
        };
        BackupError::Unsafe {
            path: self.dir.clone(),
            why,
            fix: "put the config directory and XDG_STATE_HOME in separate trees".to_owned(),
        }
    }

    fn io(&self, source: impl Into<io::Error>) -> BackupError {
        BackupError::Io { path: self.dir.clone(), source: source.into() }
    }

    fn check_directories(
        &self,
        config_dir: &Path,
        config: impl AsFd,
        config_tail: &[OsString],
        store: impl AsFd,
        store_tail: &[OsString],
    ) -> Result<(), BackupError> {
        if store_tail.is_empty()
            && is_inside(&config, &fstat(&store).map_err(|e| self.io(e))?).map_err(|e| self.io(e))?
        {
            return Err(self.overlap(config_dir, true));
        }
        if config_tail.is_empty()
            && is_inside(&store, &fstat(&config).map_err(|e| self.io(e))?).map_err(|e| self.io(e))?
        {
            return Err(self.overlap(config_dir, false));
        }
        let mounts = Mounts::read().map_err(|e| self.io(e))?;
        let config = mounts.tree(config, config_tail).map_err(|e| self.io(e))?;
        let store = mounts.tree(store, store_tail).map_err(|e| self.io(e))?;
        if store.contains(&config) {
            return Err(self.overlap(config_dir, true));
        }
        if config.contains(&store) {
            return Err(self.overlap(config_dir, false));
        }
        if store.intersects(&config) {
            return Err(BackupError::Unsafe {
                path: self.dir.clone(),
                why: format!("its mounted subtrees overlap the config directory `{}`", config_dir.display()),
                fix: "put the config directory and XDG_STATE_HOME in separate trees".to_owned(),
            });
        }
        Ok(())
    }

    pub(crate) fn check_config_directory(
        &self,
        config_dir: &Path,
        config: impl AsFd,
        missing: &[OsString],
        locked: &Locked,
    ) -> Result<(), BackupError> {
        let store =
            StoreDir::open(&self.dir)?.ok_or_else(|| self.io(io::Error::from(io::ErrorKind::NotFound)))?;
        if !same_directory(store.fd(), locked.store.fd()).map_err(|e| self.io(e))? {
            return Err(self.io(io::Error::other("the generation store directory changed during the write")));
        }
        self.check_directories(config_dir, config, missing, locked.store.fd(), &[])
    }

    /// Creates the store if needed and locks it until the returned guard drops.
    pub(crate) fn lock(&self, config: &Path) -> Result<Locked, BackupError> {
        self.check_location(config)?;
        let dir = parent_of(config);
        let (config_fd, tail) = directory_prefix(dir).map_err(|e| self.io(e))?;
        let store = StoreDir::open_for_write(&self.dir, |parent, remaining| {
            self.check_directories(dir, &config_fd, &tail, parent, remaining)
        })?;
        self.check_directories(dir, &config_fd, &tail, store.fd(), &[])?;
        let lock = store.lock().map_err(|e| self.io(e))?;
        Ok(Locked { store, _lock: lock })
    }

    /// Keeps a verified generation of `kept` for `config` in the store `locked` holds, or reuses
    /// the newest verified one when it holds the same thing. Only an `Ok` lets the caller replace
    /// the file.
    pub(crate) fn keep<E: From<BackupError>>(
        &self,
        locked: &Locked,
        config: &Path,
        kept: &Kept<'_>,
        reason: Reason,
        before_publish: impl FnOnce() -> Result<(), E>,
    ) -> Result<GenerationId, E> {
        let store = &locked.store;
        let io_err = |source| BackupError::Io { path: self.dir.clone(), source };
        let config = absolute(config);
        let manifest = describe(&config, kept, reason);
        let ids = store.ids()?;
        if let Some(id) = newest_same(store, &ids, &manifest) {
            // Read-back can see a prior rename whose directory sync failed.
            before_publish()?;
            store.sync().map_err(io_err)?;
            return Ok(id);
        }
        let id = GenerationId(ids.last().map_or(1, |last| last.0.saturating_add(1)));
        let manifest = Manifest { id: id.0, ..manifest };
        let text = toml::to_string(&manifest).map_err(|e| io_err(io::Error::other(e)))?;
        let data = match kept {
            Kept::File { bytes, .. } => Some(*bytes),
            Kept::Absent { .. } => None,
        };
        store.build(id, &text, data, before_publish)?;
        // Read it back as a later restore would: only a generation that verifies, holding exactly
        // these bytes, authorizes the write.
        match store.inspect(id) {
            Inspected::Read { health: Health::Verified, data: read, manifest: m, .. }
                if m == manifest && read.as_deref() == data =>
            {
                Ok(id)
            }
            _ => Err(BackupError::Damaged {
                store: self.dir.clone(),
                id,
                why: "it did not read back as written".to_owned(),
            }
            .into()),
        }
    }
}

/// The newest verified generation of the same config holding the same state as `wanted`. A
/// factory reset is reused only from another factory reset, so the file it replaced is always
/// listed as the one before a factory reset.
fn newest_same(store: &StoreDir, ids: &[GenerationId], wanted: &Manifest) -> Option<GenerationId> {
    ids.iter().rev().find_map(|&id| match store.inspect(id) {
        Inspected::Read { manifest, health: Health::Verified, .. } if manifest.config == wanted.config => {
            let same = manifest.kind == wanted.kind
                && manifest.mode == wanted.mode
                && manifest.len == wanted.len
                && manifest.blake3 == wanted.blake3
                && manifest.dir_existed == wanted.dir_existed
                && (wanted.reason != Reason::FactoryReset || manifest.reason == wanted.reason);
            Some(same.then_some(id))
        }
        _ => None,
    })?
}

fn describe(config: &Path, kept: &Kept<'_>, reason: Reason) -> Manifest {
    let (kind, mode, bytes, dir_existed): (Kind, u32, &[u8], _) = match kept {
        Kept::File { bytes, mode } => (Kind::File, mode & 0o777, bytes, None),
        Kept::Absent { dir_existed } => (Kind::Absent, 0, &[], Some(*dir_existed)),
    };
    Manifest {
        format: FORMAT,
        id: 0,
        config: config.to_path_buf(),
        reason,
        kind,
        mode,
        len: bytes.len() as u64,
        blake3: blake3::hash(bytes).to_hex().to_string(),
        dir_existed,
    }
}

fn generation(id: GenerationId, manifest: &Manifest, kind: GenerationKind, health: Health) -> Generation {
    Generation { id, reason: manifest.reason, kind, mode: manifest.mode, len: manifest.len, health }
}

/// The path a manifest records: absolute, symlinks not resolved (the name the user knows).
fn absolute(path: &Path) -> PathBuf {
    std::path::absolute(path).unwrap_or_else(|_| path.to_path_buf())
}
