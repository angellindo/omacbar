//! The active Omarchy theme on disk (ADR-0008): its `colors.toml` under the Omarchy state
//! directory of one home.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use tb_core::BarTheme;
use tb_core::theme::ThemeError;
use thiserror::Error;

/// The active Omarchy theme under one home directory. Touches nothing on disk until `watch`.
#[derive(Debug, Clone)]
pub struct ActiveTheme {
    colors: PathBuf,
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
    /// `<home>/.local/state/omarchy/current/theme/colors.toml`.
    pub fn under_home(home: &Path) -> Self {
        Self { colors: omarchy_current(home).join("theme").join("colors.toml") }
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
}

/// `<home>/.local/state/omarchy/current`, where `omarchy theme set` leaves the active theme.
fn omarchy_current(home: &Path) -> PathBuf {
    home.join(".local").join("state").join("omarchy").join("current")
}
