//! Generations (ADR-0015): before a protected write replaces the config file, a private,
//! verified, durable copy of what it replaces (the exact bytes and permission bits, or the fact
//! that no file was there) is kept in the user's state directory. Generations are never deleted
//! by this library; an unchanged save reuses the newest verified one instead of adding a copy.

mod manifest;
mod store;

use std::ffi::OsStr;
use std::fmt;
use std::fs::File;
use std::io;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::durable::{parent_of, resolved};
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
    /// The id a store directory name stands for: digits only, no leading zero.
    fn parse(name: &str) -> Option<Self> {
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
    /// that does not exist yet lists nothing; generations whose manifest cannot be read name no
    /// config and are not listed (they stay in the store).
    pub fn list(&self, config: &Path) -> Result<Vec<Generation>, BackupError> {
        let Some(store) = StoreDir::open(&self.dir)? else { return Ok(Vec::new()) };
        let config = absolute(config);
        let mut out = Vec::new();
        for id in store.ids()?.into_iter().rev() {
            if let Inspected::Read { manifest, kind, health, .. } = store.inspect(id)
                && manifest.config == config
            {
                out.push(generation(id, &manifest, kind, health));
            }
        }
        Ok(out)
    }

    /// Checks one generation; `Damaged` says why it cannot be trusted.
    pub fn verify(&self, id: &GenerationId) -> Result<Generation, BackupError> {
        let store =
            StoreDir::open(&self.dir)?.ok_or(BackupError::NotFound { store: self.dir.clone(), id: *id })?;
        match store.inspect(*id) {
            Inspected::Missing => Err(BackupError::NotFound { store: self.dir.clone(), id: *id }),
            Inspected::Unreadable(why) => Err(BackupError::Damaged { store: self.dir.clone(), id: *id, why }),
            Inspected::Read { health: Health::Damaged(why), .. } => {
                Err(BackupError::Damaged { store: self.dir.clone(), id: *id, why })
            }
            Inspected::Read { manifest, kind, health, .. } => Ok(generation(*id, &manifest, kind, health)),
        }
    }

    /// Refuses a store at or inside the directory of `config`: a restore replaces that
    /// directory's file, and the copies must not live where they could be lost with it. Checked
    /// lexically and through symlinks, before anything is created.
    pub(crate) fn check_location(&self, config: &Path) -> Result<(), BackupError> {
        let config_dir = parent_of(config);
        let inside = absolute(&self.dir).starts_with(absolute(config_dir))
            || resolved(&self.dir).starts_with(resolved(config_dir));
        if !inside {
            return Ok(());
        }
        Err(BackupError::Unsafe {
            path: self.dir.clone(),
            why: format!("it is inside the config directory `{}`", config_dir.display()),
            fix: "set XDG_STATE_HOME to a directory outside it".to_owned(),
        })
    }

    /// Creates the store if needed, refusing one that cannot be trusted (or that lies inside the
    /// directory of `config`), and locks it until the returned guard drops.
    pub(crate) fn lock(&self, config: &Path) -> Result<Locked, BackupError> {
        self.check_location(config)?;
        let store = StoreDir::open_for_write(&self.dir)?;
        let lock = store.lock().map_err(|source| BackupError::Io { path: self.dir.clone(), source })?;
        Ok(Locked { store, _lock: lock })
    }

    /// Keeps a verified generation of `kept` for `config` in the store `locked` holds, or reuses
    /// the newest verified one when it holds the same thing. Only an `Ok` lets the caller replace
    /// the file.
    pub(crate) fn keep(
        &self,
        locked: &Locked,
        config: &Path,
        kept: &Kept<'_>,
        reason: Reason,
    ) -> Result<GenerationId, BackupError> {
        let store = &locked.store;
        let io_err = |source| BackupError::Io { path: self.dir.clone(), source };
        let config = absolute(config);
        let manifest = describe(&config, kept, reason);
        let ids = store.ids()?;
        if let Some(id) = newest_same(store, &ids, &manifest) {
            return Ok(id);
        }
        let id = GenerationId(ids.last().map_or(1, |last| last.0.saturating_add(1)));
        let manifest = Manifest { id: id.0, ..manifest };
        let text = toml::to_string(&manifest).map_err(|e| io_err(io::Error::other(e)))?;
        let data = match kept {
            Kept::File { bytes, .. } => Some(*bytes),
            Kept::Absent { .. } => None,
        };
        store.build(id, &text, data).map_err(io_err)?;
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
            }),
        }
    }
}

/// The newest verified generation of the same config holding the same state as `wanted`.
fn newest_same(store: &StoreDir, ids: &[GenerationId], wanted: &Manifest) -> Option<GenerationId> {
    ids.iter().rev().find_map(|&id| match store.inspect(id) {
        Inspected::Read { manifest, health: Health::Verified, .. } if manifest.config == wanted.config => {
            let same = manifest.kind == wanted.kind
                && manifest.mode == wanted.mode
                && manifest.len == wanted.len
                && manifest.blake3 == wanted.blake3
                && manifest.dir_existed == wanted.dir_existed;
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
