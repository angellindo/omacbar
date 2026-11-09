//! The active Omarchy theme on disk (ADR-0008): its `colors.toml` under the Omarchy state
//! directory of one home, and the watch for a theme switch.

use std::ffi::OsString;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use tb_core::BarTheme;
use tb_core::theme::ThemeError;
use thiserror::Error;

use crate::watch::{DirWatch, WatchFailure};

/// Where the shipped `theme-set.d` hook writes, under `<home>/.local/state`.
const HOOK_DIR: &str = "touchbar-customizer";
/// The file the hook writes the new theme's name to.
const HOOK_FILE: &str = "theme-set";

/// The active Omarchy theme under one home directory. Touches nothing on disk until `watch`.
#[derive(Debug, Clone)]
pub struct ActiveTheme {
    /// `<home>/.local/state/omarchy/current`.
    current: PathBuf,
    colors: PathBuf,
    /// `<home>/.local/state/touchbar-customizer`.
    hook_dir: PathBuf,
}

/// A watch for a theme switch under one home. `Send`, so it can block on its own thread.
#[derive(Debug)]
pub struct ThemeWatch {
    dirs: DirWatch,
}

/// What [`ActiveTheme::load`] found.
#[derive(Debug)]
pub struct LoadedTheme {
    pub theme: BarTheme,
    /// Why the fallback is shown; `None` when `theme` came from the file.
    pub fallback: Option<ThemeFileError>,
}

#[derive(Debug, Error)]
pub enum ThemeFileError {
    #[error("theme file `{}`: {source}", path.display())]
    Io { path: PathBuf, source: io::Error },
    #[error("theme file `{}`: {source}", path.display())]
    Invalid { path: PathBuf, source: ThemeError },
    #[error("the theme directory `{}` is no longer watched (it was removed)", dir.display())]
    WatchEnded { dir: PathBuf },
}

impl ActiveTheme {
    /// The active theme under `home`: colours at
    /// `<home>/.local/state/omarchy/current/theme/colors.toml`; the hook writes
    /// `<home>/.local/state/touchbar-customizer/theme-set`.
    pub fn under_home(home: &Path) -> Self {
        let state = home.join(".local").join("state");
        let current = state.join("omarchy").join("current");
        Self { colors: current.join("theme").join("colors.toml"), current, hook_dir: state.join(HOOK_DIR) }
    }

    /// `<home>/.local/state/omarchy/current/theme/colors.toml`.
    pub fn colors_path(&self) -> &Path {
        &self.colors
    }

    /// The bar theme from the active `colors.toml`. When the file is missing, unreadable or not a
    /// valid Omarchy theme: the bundled Tokyo Night (`BarTheme::omarchy_fallback`) and the reason
    /// naming the file. Creates nothing.
    pub fn load(&self) -> LoadedTheme {
        let read = fs::read_to_string(&self.colors)
            .map_err(|source| ThemeFileError::Io { path: self.colors.clone(), source })
            .and_then(|text| {
                BarTheme::from_omarchy_colors(&text)
                    .map_err(|source| ThemeFileError::Invalid { path: self.colors.clone(), source })
            });
        match read {
            Ok(theme) => LoadedTheme { theme, fallback: None },
            Err(reason) => LoadedTheme { theme: BarTheme::omarchy_fallback(), fallback: Some(reason) },
        }
    }

    /// Watches for a theme switch: the rename of `current/theme` or a write of
    /// `current/theme.name`, and the hook's `theme-set` file. Creates
    /// `<home>/.local/state/touchbar-customizer/` if missing. Err (creating nothing) when
    /// `<home>/.local/state/omarchy/current/` does not exist. Registered before it returns, so no
    /// later switch is missed.
    pub fn watch(&self) -> Result<ThemeWatch, ThemeFileError> {
        let io_at = |dir: &Path| {
            let path = dir.to_path_buf();
            move |source| ThemeFileError::Io { path, source }
        };
        let mut dirs = DirWatch::new().map_err(io_at(&self.current))?;
        // Omarchy renames a staged `next-theme` directory to `theme`, then writes `theme.name`.
        dirs.add(&self.current, vec![OsString::from("theme"), OsString::from("theme.name")])
            .map_err(io_at(&self.current))?;
        fs::create_dir_all(&self.hook_dir).map_err(io_at(&self.hook_dir))?;
        dirs.add(&self.hook_dir, vec![OsString::from(HOOK_FILE)]).map_err(io_at(&self.hook_dir))?;
        Ok(ThemeWatch { dirs })
    }
}

impl ThemeWatch {
    /// Blocks until a theme switch or a hook signal. Changes read together count once. Other
    /// names in the watched directories (`background`, `theme.generation`, `next-theme`) and
    /// edits inside `current/theme/` are not switches. A watched directory going away →
    /// `WatchEnded` naming it.
    pub fn next_change(&mut self) -> Result<(), ThemeFileError> {
        self.dirs.next_change().map_err(|e| match e {
            WatchFailure::Io { dir, source } => ThemeFileError::Io { path: dir, source },
            WatchFailure::Ended { dir } => ThemeFileError::WatchEnded { dir },
        })
    }
}
