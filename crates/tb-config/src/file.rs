//! Loading and saving the config file. The current config is always the last valid one.

use std::fs;
use std::io::{self, Write};
use std::path::{Path, PathBuf};

use tb_core::config::{Config, ConfigError, ConfigIssue, parse_valid, to_toml, validate};
use tb_core::{BarSize, BarTheme, SpacingRules};
use thiserror::Error;

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
    #[error("the config directory `{}` is no longer watched (it was removed)", dir.display())]
    WatchEnded { dir: PathBuf },
}

/// The config file at one path and the last valid config read from or saved to it.
#[derive(Debug)]
pub struct ConfigFile {
    path: PathBuf,
    config: Config,
}

impl ConfigFile {
    /// The config file at `path`. Touches nothing on disk. Until a load or save succeeds the
    /// current config is `Config::default()`.
    pub fn new(path: impl Into<PathBuf>) -> Self {
        Self { path: path.into(), config: Config::default() }
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    /// The current config: always the last valid one read or saved.
    pub fn config(&self) -> &Config {
        &self.config
    }

    /// Reads and checks the file. A missing file gets the default config written atomically.
    /// On an error the current config is unchanged.
    pub fn load(&mut self, theme: &BarTheme) -> Result<Loaded, ConfigFileError> {
        let text = match fs::read_to_string(&self.path) {
            Ok(text) => text,
            Err(e) if e.kind() == io::ErrorKind::NotFound => {
                let default = Config::default();
                write_atomically(&self.path, &to_toml(&default)?)?;
                self.config = default;
                return Ok(Loaded::WroteDefault);
            }
            Err(source) => return Err(self.io(source)),
        };
        match parse_valid(&text, BarSize::MACBOOK_PRO, &SpacingRules::STANDARD, theme) {
            Ok(config) => {
                self.config = config;
                Ok(Loaded::Read)
            }
            Err(issues) => Ok(Loaded::Rejected(issues)),
        }
    }

    /// Checks `config`; if valid, writes it atomically and makes it current. An invalid config
    /// is refused with every issue and nothing is written.
    pub fn save(&mut self, config: Config, theme: &BarTheme) -> Result<(), ConfigFileError> {
        let issues = validate(&config, BarSize::MACBOOK_PRO, &SpacingRules::STANDARD, theme);
        if !issues.is_empty() {
            return Err(ConfigFileError::Invalid(issues));
        }
        write_atomically(&self.path, &to_toml(&config)?)?;
        self.config = config;
        Ok(())
    }

    fn io(&self, source: io::Error) -> ConfigFileError {
        ConfigFileError::Io { path: self.path.clone(), source }
    }
}

/// Writes `text` to a temp file next to `path`, syncs it and renames it over `path`, so a reader
/// sees the old file or the new one, never half of it. The directory is created if missing; on
/// any failure the temp file is removed.
fn write_atomically(path: &Path, text: &str) -> Result<(), ConfigFileError> {
    let io_at = |at: &Path| {
        let at = at.to_path_buf();
        move |source| ConfigFileError::Io { path: at, source }
    };
    let dir = path.parent().unwrap_or(Path::new(""));
    fs::create_dir_all(dir).map_err(io_at(dir))?;
    let temp = temp_path(path);
    let written = write_synced(&temp, text)
        .map_err(io_at(&temp))
        .and_then(|()| fs::rename(&temp, path).map_err(io_at(path)));
    if written.is_err() {
        // Best effort: the write already failed, and that error is the one worth reporting.
        let _ = fs::remove_file(&temp);
    }
    written
}

fn write_synced(path: &Path, text: &str) -> io::Result<()> {
    let mut file = fs::File::create(path)?;
    file.write_all(text.as_bytes())?;
    file.sync_all()
}

/// `.<file name>.<pid>.tmp` in the same directory: same filesystem (so `rename` is atomic), and
/// the pid keeps two processes from sharing a temp file.
fn temp_path(path: &Path) -> PathBuf {
    let name = path.file_name().map_or_else(Default::default, |n| n.to_string_lossy().into_owned());
    path.with_file_name(format!(".{name}.{}.tmp", std::process::id()))
}
