//! Loading and saving the config file. The current config is always the last valid one. Every
//! write is compare-and-swap (ADR-0013): it replaces only the version this `ConfigFile` last read
//! or wrote, so a stale editor never replaces a newer file.

mod disk;

use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use tb_core::config::{Config, ConfigError, ConfigIssue, ThemeSource, parse, parse_valid, to_toml, validate};
use tb_core::{BarSize, BarTheme, SpacingRules};
use thiserror::Error;

use crate::ThemeFileError;
pub use disk::DiskVersion;
use disk::{DirLock, version_at, write_atomically, write_backup};

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
    /// An acknowledged overwrite at a symlink is refused: replacing the link would lose it, and
    /// writing through it would change a file outside the config directory.
    #[error("config file `{}` is a symbolic link; overwriting it is refused, nothing was written", path.display())]
    Symlink { path: PathBuf },
}

/// The config file at one path, the last valid config read from or saved to it, and the version
/// of the file it last saw (what a save may replace).
#[derive(Debug)]
pub struct ConfigFile {
    path: PathBuf,
    config: Config,
    seen: DiskVersion,
    backup: Option<PathBuf>,
}

impl ConfigFile {
    /// The config file at `path`. Touches nothing on disk. Until a load or save succeeds the
    /// current config is `Config::default()`, and a save may only create the file, never replace
    /// one this config file has not read.
    pub fn new(path: impl Into<PathBuf>) -> Self {
        Self { path: path.into(), config: Config::default(), seen: DiskVersion::ABSENT, backup: None }
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
        let seen = version_at(&self.path).map_err(|e| self.io(e))?;
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
        let found = version_at(&self.path).map_err(|e| self.io(e))?.0;
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
        match self.swap(&DiskVersion::ABSENT, &to_toml(&default)?, false) {
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
        self.swap(&seen, &to_toml(&config)?, false)?;
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
        self.swap(&seen, &to_toml(&config)?, false)?;
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
        let theme = resolve_theme(&config.theme, active)?;
        check(&config, &theme)?;
        self.swap(acknowledged, &to_toml(&config)?, true)?;
        self.config = config;
        Ok(())
    }

    /// Writes `text` only if the file is `expected`; records it as seen. With `backup`, the
    /// replaced bytes are kept first (a backup that exists while the rename did not happen still
    /// holds the version on disk, so an interruption loses nothing).
    fn swap(&mut self, expected: &DiskVersion, text: &str, backup: bool) -> Result<(), ConfigFileError> {
        let io_at = |at: &Path, source| ConfigFileError::Io { path: at.to_path_buf(), source };
        // Unlocked first look: a stale save must not even create a removed directory.
        let current = version_at(&self.path).map_err(|e| self.io(e))?;
        if current != *expected {
            return Err(ConfigFileError::Changed(current));
        }
        let dir = dir_of(&self.path);
        fs::create_dir_all(dir).map_err(|e| io_at(dir, e))?;
        let lock = DirLock::take(dir).map_err(|e| io_at(dir, e))?;
        let current = version_at(&self.path).map_err(|e| self.io(e))?;
        if current != *expected {
            return Err(ConfigFileError::Changed(current));
        }
        if backup && fs::symlink_metadata(&self.path).is_ok_and(|m| m.file_type().is_symlink()) {
            return Err(ConfigFileError::Symlink { path: self.path.clone() });
        }
        let kept = match (&current.0, backup) {
            (Some(bytes), true) => Some(write_backup(&self.path, bytes).map_err(|e| io_at(dir, e))?),
            _ => None,
        };
        // Last check right before the rename: narrows the window for editors that take no lock.
        let recheck = || match version_at(&self.path) {
            Ok(now) if now == *expected => Ok(()),
            Ok(now) => Err(ConfigFileError::Changed(now)),
            Err(e) => Err(io_at(&self.path, e)),
        };
        let written = write_atomically(&self.path, text, io_at, recheck);
        if written.is_err()
            && let Some(kept) = &kept
        {
            // Nothing was replaced, so the copy of the unreplaced file is not a backup of anything.
            let _ = fs::remove_file(kept);
        }
        written?;
        // Best effort: the rename is done; durability of the directory entry is a bonus.
        let _ = lock.sync();
        self.seen = DiskVersion(Some(text.as_bytes().to_vec()));
        if backup {
            self.backup = kept;
        }
        Ok(())
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

/// The directory that holds `path`; a bare file name (`config.toml`) is in the current directory,
/// named `.` because inotify and other syscalls reject an empty path.
pub(crate) fn dir_of(path: &Path) -> &Path {
    match path.parent() {
        Some(dir) if !dir.as_os_str().is_empty() => dir,
        _ => Path::new("."),
    }
}
