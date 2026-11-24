//! Loading and saving the config file. The current config is always the last valid one. Every
//! write is compare-and-swap (ADR-0013): it replaces only the version this `ConfigFile` last read
//! or wrote, so a stale editor never replaces a newer file. A protected config file (ADR-0015)
//! also keeps a verified generation of what each write replaces, and refuses paths it could not
//! keep and restore exactly.

mod disk;
mod recover;

use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use tb_core::config::{Config, ConfigError, ConfigIssue, ThemeSource, parse, parse_valid, to_toml, validate};
use tb_core::{BarSize, BarTheme, SpacingRules};
use thiserror::Error;

use crate::ThemeFileError;
use crate::backup::{BackupError, Backups, GenerationId, Kept, Locked, Reason};
use crate::durable::{create_dir_all_durably, parent_of};
pub use disk::DiskVersion;
use disk::{DirLock, Found, Refused, version_at, version_followed, write_atomically, write_backup};

/// Permission bits of a config file a protected write creates: it is personal.
const NEW_FILE_MODE: u32 = 0o600;

/// `<home>/.config/touchbar-customizer/config.toml`.
pub fn default_path(home: &Path) -> PathBuf {
    home.join(".config").join("touchbar-customizer").join("config.toml")
}

/// What [`ConfigFile::load`] found.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Loaded {
    /// The file was missing: the default config was written (directory created) and is current.
    WroteDefault,
    /// The file is valid and is now the current config.
    Read,
    /// The file is invalid: every issue; the current config is unchanged and the file untouched.
    Rejected(Vec<ConfigIssue>),
}

#[derive(Debug, Error)]
pub enum ConfigFileError {
    #[error("config file `{}`: {source}", path.display())]
    Io { path: PathBuf, source: io::Error },
    #[error("the config was not saved, it has {} issue(s)", .0.len())]
    Invalid(Vec<ConfigIssue>),
    #[error(transparent)]
    Serialize(#[from] ConfigError),
    #[error(transparent)]
    Theme(#[from] ThemeFileError),
    #[error("the config directory `{}` is no longer watched (it was removed)", dir.display())]
    WatchEnded { dir: PathBuf },
    /// The file is no longer the version this config file read or wrote (another editor saved,
    /// it was removed, or it was never read): nothing was written. Holds the version now on disk.
    #[error("the config file changed on disk since it was read; nothing was written")]
    Changed(DiskVersion),
    /// A write at a symlink is refused (every protected write, and an acknowledged overwrite):
    /// replacing the link would lose it, and writing through it would change a file outside the
    /// config directory. Reads still follow it.
    #[error("config file `{}` is a symbolic link; writing it is refused, nothing was written", path.display())]
    Symlink { path: PathBuf },
    /// A protected config file is something it cannot keep and restore exactly: a directory,
    /// FIFO, socket, device, or a file with more than one name. Nothing was read or written.
    #[error(
        "config file `{}` is a {kind}; only a regular file with one name can be kept and replaced, nothing was written",
        path.display()
    )]
    Unsupported { path: PathBuf, kind: String },
    /// The generation of what a protected write would replace could not be kept and verified;
    /// nothing was written.
    #[error("the config was not saved: {0}")]
    Backup(#[from] BackupError),
    /// Restoring needs the generation store, and this config file was opened without one.
    #[error("config file `{}` has no generation store to restore from", path.display())]
    Unprotected { path: PathBuf },
}

/// Which write replaces the file, and the reason its generation records.
#[derive(Clone, Copy, PartialEq, Eq)]
enum WriteKind {
    Default,
    Save,
    Overwrite,
    Recover,
}

impl WriteKind {
    fn reason(self) -> Reason {
        match self {
            Self::Default => Reason::FirstWrite,
            Self::Save => Reason::Save,
            Self::Overwrite => Reason::Overwrite,
            Self::Recover => Reason::Recover,
        }
    }
}

/// What a write puts at the path: serialized text (new files get private bits), the exact bytes
/// and permission bits of a restored generation, or no file at all.
#[derive(Clone, Copy)]
enum Content<'a> {
    Text(&'a str),
    Exact { bytes: &'a [u8], mode: u32 },
    Remove,
}

impl<'a> Content<'a> {
    /// The bytes the path holds once the write is done (`None`: no file).
    fn bytes(self) -> Option<&'a [u8]> {
        match self {
            Self::Text(text) => Some(text.as_bytes()),
            Self::Exact { bytes, .. } => Some(bytes),
            Self::Remove => None,
        }
    }
}

/// The config file at one path, the last valid config read from or saved to it, and the version
/// of the file it last saw (what a save may replace).
#[derive(Debug)]
pub struct ConfigFile {
    path: PathBuf,
    config: Config,
    seen: DiskVersion,
    backup: Option<PathBuf>,
    protection: Option<Backups>,
    last_generation: Option<GenerationId>,
}

impl ConfigFile {
    /// The config file at `path`. Touches nothing on disk. Until a load or save succeeds the
    /// current config is `Config::default()`, and a save may only create the file, never replace
    /// one this config file has not read.
    pub fn new(path: impl Into<PathBuf>) -> Self {
        Self {
            path: path.into(),
            config: Config::default(),
            seen: DiskVersion::ABSENT,
            backup: None,
            protection: None,
            last_generation: None,
        }
    }

    /// Like `new`, but every write first keeps a verified generation of what it replaces in
    /// `backups`, and paths that cannot be kept exactly are refused before anything changes:
    /// a symlink (written through or replaced), a hard-linked file, anything but a regular file
    /// (also on reads, which never block on a FIFO), and a store that cannot be trusted.
    pub fn protected(path: impl Into<PathBuf>, backups: Backups) -> Self {
        Self { protection: Some(backups), ..Self::new(path) }
    }

    /// The same file with the same protection, as freshly opened: nothing read or seen.
    #[must_use]
    pub fn reopened(&self) -> Self {
        Self { protection: self.protection.clone(), ..Self::new(self.path.clone()) }
    }

    /// The generation that holds what the last protected write replaced (a reused one when the
    /// write replaced the same thing again); `None` before any protected write.
    pub fn last_generation(&self) -> Option<&GenerationId> {
        self.last_generation.as_ref()
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    /// The current config: always the last valid one read or saved.
    pub fn config(&self) -> &Config {
        &self.config
    }

    /// Where the last acknowledged overwrite kept the bytes it replaced (`None` if it replaced
    /// nothing, or no overwrite happened).
    pub fn backup(&self) -> Option<&Path> {
        self.backup.as_deref()
    }

    /// Reads a candidate, resolves its saved theme source, then validates and accepts both.
    /// Rejection leaves the current config untouched. A missing file gets the default for the
    /// active theme (`Config::default_for`), written atomically unless another writer created the
    /// file meanwhile, which is then read instead.
    pub fn load_resolved(&mut self, active: &BarTheme) -> Result<BarTheme, ConfigFileError> {
        let Some(text) = self.read_or_default(active)? else { return Ok(active.clone()) };
        self.accept_resolved(&text, active)
    }

    /// Like `load_resolved`, but a missing file is a read failure, never a request to replace
    /// the accepted config with the default during a save.
    pub fn reload_resolved(&mut self, active: &BarTheme) -> Result<BarTheme, ConfigFileError> {
        let seen = self.version_now()?;
        self.seen = seen.clone();
        let Some(bytes) = seen.0 else {
            return Err(self.io(io::Error::new(io::ErrorKind::NotFound, "the config file was removed")));
        };
        let text = self.text(bytes)?;
        self.accept_resolved(&text, active)
    }

    fn accept_resolved(&mut self, text: &str, active: &BarTheme) -> Result<BarTheme, ConfigFileError> {
        let candidate =
            parse(text).map_err(|e| ConfigFileError::Invalid(vec![ConfigIssue::Syntax(e.to_string())]))?;
        let theme = resolve_theme(&candidate.theme, active)?;
        check(&candidate, &theme)?;
        self.config = candidate;
        Ok(theme)
    }

    /// Validates the accepted config against a new active Omarchy theme without reading disk.
    /// Callers use this only while the accepted source is Omarchy.
    pub fn check_active_theme(&self, active: &BarTheme) -> Result<(), ConfigFileError> {
        check(&self.config, active)
    }

    /// Reads and checks the file. A missing file gets the default for `theme`
    /// (`Config::default_for`) written atomically, unless another writer created it meanwhile.
    /// On an error the current config is unchanged.
    pub fn load(&mut self, theme: &BarTheme) -> Result<Loaded, ConfigFileError> {
        let Some(text) = self.read_or_default(theme)? else { return Ok(Loaded::WroteDefault) };
        match parse_valid(&text, BarSize::MACBOOK_PRO, &SpacingRules::STANDARD, theme) {
            Ok(config) => {
                self.config = config;
                Ok(Loaded::Read)
            }
            Err(issues) => Ok(Loaded::Rejected(issues)),
        }
    }

    /// The file's text, recording it as the version seen (valid or not: it is what the user is
    /// shown). A missing file is created as the default for `theme`, which is then current
    /// (`None`); if another writer created the file first, its text is returned instead.
    fn read_or_default(&mut self, theme: &BarTheme) -> Result<Option<String>, ConfigFileError> {
        let found = self.version_now()?.0;
        let bytes = match found {
            Some(bytes) => bytes,
            None => match self.write_default(theme)? {
                Some(theirs) => theirs,
                None => return Ok(None),
            },
        };
        let text = self.text(bytes.clone())?;
        self.seen = DiskVersion(Some(bytes));
        Ok(Some(text))
    }

    /// Creates the missing file as the default for `theme` (`Ok(None)`, now current), or returns
    /// the bytes of the file another writer created first.
    fn write_default(&mut self, theme: &BarTheme) -> Result<Option<Vec<u8>>, ConfigFileError> {
        let default = Config::default_for(theme);
        match self.swap(&DiskVersion::ABSENT, Content::Text(&to_toml(&default)?), WriteKind::Default) {
            Ok(()) => {
                self.config = default;
                Ok(None)
            }
            Err(ConfigFileError::Changed(DiskVersion(Some(bytes)))) => Ok(Some(bytes)),
            Err(e) => Err(e),
        }
    }

    fn text(&self, bytes: Vec<u8>) -> Result<String, ConfigFileError> {
        String::from_utf8(bytes).map_err(|e| self.io(io::Error::new(io::ErrorKind::InvalidData, e)))
    }

    /// Checks `config`; if valid and the file is still the version this config file last saw,
    /// writes it atomically and makes it current. An invalid config is refused with every issue,
    /// a newer file with `Changed`; either way nothing is written.
    pub fn save(&mut self, config: Config, theme: &BarTheme) -> Result<(), ConfigFileError> {
        check(&config, theme)?;
        let seen = self.seen.clone();
        self.swap(&seen, Content::Text(&to_toml(&config)?), WriteKind::Save)?;
        self.config = config;
        Ok(())
    }

    /// Like `save`, but first resolves the candidate's own theme source again (the selected file
    /// is re-read; Omarchy is `active`) and validates against it, as `load_resolved` does.
    /// Returns the resolved theme. On any error nothing is written and the current config stays.
    pub fn save_resolved(&mut self, config: Config, active: &BarTheme) -> Result<BarTheme, ConfigFileError> {
        let theme = resolve_theme(&config.theme, active)?;
        check(&config, &theme)?;
        let seen = self.seen.clone();
        self.swap(&seen, Content::Text(&to_toml(&config)?), WriteKind::Save)?;
        self.config = config;
        Ok(theme)
    }

    /// An overwrite the user acknowledged after a `Changed` refusal: like `save_resolved`, but
    /// it replaces `acknowledged` (the version that refusal named) instead of the version last
    /// seen, and first keeps the replaced bytes in a new private backup (see [`Self::backup`]).
    /// A further change since the acknowledgement is `Changed` again; a symlink is refused.
    pub fn save_over(
        &mut self,
        config: Config,
        active: &BarTheme,
        acknowledged: &DiskVersion,
    ) -> Result<(), ConfigFileError> {
        self.save_over_resolved(config, active, acknowledged).map(drop)
    }

    /// `save_over`, returning the theme the config was validated against (the selected file as
    /// read for this save, or `active`), so the caller shows exactly what was checked.
    pub fn save_over_resolved(
        &mut self,
        config: Config,
        active: &BarTheme,
        acknowledged: &DiskVersion,
    ) -> Result<BarTheme, ConfigFileError> {
        let theme = resolve_theme(&config.theme, active)?;
        check(&config, &theme)?;
        self.swap(acknowledged, Content::Text(&to_toml(&config)?), WriteKind::Overwrite)?;
        self.config = config;
        Ok(theme)
    }

    /// Writes `content` only if the file is `expected`; records it as seen. An overwrite first keeps
    /// the replaced bytes in a `.bak` (a backup that exists while the rename did not happen still
    /// holds the version on disk, so an interruption loses nothing). A protected file first keeps
    /// a verified generation; any refusal comes before the file or its directory changes.
    fn swap(
        &mut self,
        expected: &DiskVersion,
        content: Content<'_>,
        kind: WriteKind,
    ) -> Result<(), ConfigFileError> {
        let io_at = |at: &Path, source| ConfigFileError::Io { path: at.to_path_buf(), source };
        // Unlocked first look: a stale save must not even create a removed directory.
        let current = self.version_now()?;
        if current != *expected {
            return Err(ConfigFileError::Changed(current));
        }
        // A protected write locks the store before it looks at or creates the directory, so
        // `dir_existed` is decided while no other protected writer can create it (ADR-0015).
        // Order: store lock, then directory lock, for every protected write.
        let store = match &self.protection {
            Some(backups) => {
                backups.check_location(&self.path)?;
                // A path that cannot be kept is refused before the store is created.
                disk::probe(&self.path).map_err(|r| refused(&self.path, r))?;
                Some(backups.lock(&self.path)?)
            }
            None => None,
        };
        let dir = dir_of(&self.path);
        let dir_existed = dir.is_dir();
        create_dir_all_durably(dir, 0o777).map_err(|e| io_at(dir, e))?;
        let lock = DirLock::take(dir).map_err(|e| io_at(dir, e))?;
        let current = self.version_now()?;
        if current != *expected {
            return Err(ConfigFileError::Changed(current));
        }
        let (mode, generation) = match (&self.protection, &store) {
            (Some(backups), Some(locked)) => {
                let (mode, id) = self.keep_generation(backups, locked, expected, dir_existed, kind)?;
                (Some(mode), Some(id))
            }
            _ if kind == WriteKind::Overwrite
                && fs::symlink_metadata(&self.path).is_ok_and(|m| m.file_type().is_symlink()) =>
            {
                return Err(ConfigFileError::Symlink { path: self.path.clone() });
            }
            _ => (None, None),
        };
        let kept = match (&current.0, kind) {
            (Some(bytes), WriteKind::Overwrite) => {
                Some(write_backup(&self.path, bytes).map_err(|e| io_at(dir, e))?)
            }
            _ => None,
        };
        let written = self.put(expected, content, mode);
        if written.is_err()
            && let Some(kept) = &kept
        {
            // Nothing was replaced, so the copy of the unreplaced file is not a backup of anything.
            let _ = fs::remove_file(kept);
        }
        written?;
        // Best effort: the rename is done; durability of the directory entry is a bonus.
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

    /// Under the directory lock, after any generation is kept: puts `content` at the path (an
    /// atomic write with `mode`, or a removal), right after a last re-compare that narrows the
    /// window for editors that take no lock.
    fn put(
        &self,
        expected: &DiskVersion,
        content: Content<'_>,
        mode: Option<u32>,
    ) -> Result<(), ConfigFileError> {
        let io_at = |at: &Path, source| ConfigFileError::Io { path: at.to_path_buf(), source };
        let protected = self.protection.is_some();
        let recheck = || match version(&self.path, protected) {
            Ok(now) if now == *expected => Ok(()),
            Ok(now) => Err(ConfigFileError::Changed(now)),
            Err(e) => Err(e),
        };
        match content {
            Content::Text(text) => write_atomically(&self.path, text.as_bytes(), mode, io_at, recheck),
            Content::Exact { bytes, mode } => write_atomically(&self.path, bytes, Some(mode), io_at, recheck),
            Content::Remove => {
                recheck().and_then(|()| fs::remove_file(&self.path).map_err(|e| io_at(&self.path, e)))
            }
        }
    }

    /// Under the directory lock: looks at the path without following a symlink, and keeps a
    /// verified generation of what is there (exact bytes and bits, or the absence). Returns the
    /// bits the new file gets (the replaced file's, or private for a new one) and the generation.
    fn keep_generation(
        &self,
        backups: &Backups,
        locked: &Locked,
        expected: &DiskVersion,
        dir_existed: bool,
        kind: WriteKind,
    ) -> Result<(u32, GenerationId), ConfigFileError> {
        let found = disk::probe(&self.path).map_err(|r| refused(&self.path, r))?;
        let (kept, now, mode) = match &found {
            Found::Absent => (Kept::Absent { dir_existed }, DiskVersion::ABSENT, NEW_FILE_MODE),
            Found::File { bytes, mode } => {
                (Kept::File { bytes, mode: *mode }, DiskVersion(Some(bytes.clone())), *mode)
            }
        };
        if now != *expected {
            return Err(ConfigFileError::Changed(now));
        }
        let id = backups.keep(locked, &self.path, &kept, kind.reason())?;
        Ok((mode, id))
    }

    /// The version on disk now, read as this file reads (protected: regular files only).
    fn version_now(&self) -> Result<DiskVersion, ConfigFileError> {
        version(&self.path, self.protection.is_some())
    }

    fn io(&self, source: io::Error) -> ConfigFileError {
        ConfigFileError::Io { path: self.path.clone(), source }
    }
}

fn check(config: &Config, theme: &BarTheme) -> Result<(), ConfigFileError> {
    let issues = validate(config, BarSize::MACBOOK_PRO, &SpacingRules::STANDARD, theme);
    if issues.is_empty() { Ok(()) } else { Err(ConfigFileError::Invalid(issues)) }
}

fn resolve_theme(source: &ThemeSource, active: &BarTheme) -> Result<BarTheme, ThemeFileError> {
    let ThemeSource::File { path } = source else { return Ok(active.clone()) };
    let path = PathBuf::from(path);
    let text =
        fs::read_to_string(&path).map_err(|source| ThemeFileError::Io { path: path.clone(), source })?;
    BarTheme::from_omarchy_colors(&text).map_err(|source| ThemeFileError::Invalid { path, source })
}

fn version(path: &Path, protected: bool) -> Result<DiskVersion, ConfigFileError> {
    if protected {
        version_followed(path).map_err(|r| refused(path, r))
    } else {
        version_at(path).map_err(|source| ConfigFileError::Io { path: path.to_path_buf(), source })
    }
}

fn refused(path: &Path, why: Refused) -> ConfigFileError {
    let path = path.to_path_buf();
    match why {
        Refused::Symlink => ConfigFileError::Symlink { path },
        Refused::Unsupported(kind) => ConfigFileError::Unsupported { path, kind },
        Refused::Io(source) => ConfigFileError::Io { path, source },
    }
}

/// The directory that holds `path`; a bare file name (`config.toml`) is in the current directory,
/// named `.` because inotify and other syscalls reject an empty path.
pub(crate) fn dir_of(path: &Path) -> &Path {
    parent_of(path)
}
