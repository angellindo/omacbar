//! The user config file (`~/.config/touchbar-customizer/config.toml`): parse and validate.

use std::collections::BTreeSet;

use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::chord::Chord;
use crate::geometry::BarSize;
use crate::layout::{LayoutError, place};
use crate::model::{ButtonId, Layout};
use crate::spacing::SpacingRules;
use crate::theme::BarTheme;

pub const CONFIG_VERSION: u32 = 1;

/// Where colours come from.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(tag = "source", rename_all = "kebab-case", deny_unknown_fields)]
pub enum ThemeSource {
    /// Follow the active Omarchy theme (`~/.local/state/omarchy/current/theme/colors.toml`).
    #[default]
    Omarchy,
    /// A specific Omarchy-format `colors.toml`.
    File { path: String },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Config {
    pub version: u32,
    #[serde(default)]
    pub theme: ThemeSource,
    #[serde(flatten)]
    pub layout: Layout,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            version: CONFIG_VERSION,
            theme: ThemeSource::Omarchy,
            layout: crate::presets::default_layout(),
        }
    }
}

/// One problem found by [`validate`]. Validation reports all problems, not just the first.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum ConfigIssue {
    #[error("unsupported config version {0} (this build reads version {CONFIG_VERSION})")]
    Version(u32),
    #[error("there is no `default` layer; the F-key row must always exist")]
    NoDefaultLayer,
    #[error("layer `{0:?}` is defined more than once")]
    DuplicateLayer(Chord),
    #[error("layer `{chord:?}`: button id `{id}` is used more than once")]
    DuplicateId { chord: Chord, id: ButtonId },
    #[error("layer `{chord:?}`: button id `{id}` must be kebab-case (a-z, 0-9, -)")]
    BadId { chord: Chord, id: ButtonId },
    #[error("layer `{chord:?}`: button `{id}` has an empty label")]
    EmptyLabel { chord: Chord, id: ButtonId },
    #[error("layer `{chord:?}`: button `{id}` uses unknown colour `{color}`")]
    UnknownColor { chord: Chord, id: ButtonId, color: String },
    #[error("layer `{chord:?}` does not fit: {error}")]
    Layout { chord: Chord, error: LayoutError },
    /// The text is not valid TOML or not a config shape; the message is the parser's.
    #[error("the config is not valid: {0}")]
    Syntax(String),
}

#[derive(Debug, Error)]
pub enum ConfigError {
    #[error("config is not valid TOML: {0}")]
    Toml(#[from] toml::de::Error),
    #[error("config could not be written: {0}")]
    Write(#[from] toml::ser::Error),
}

pub fn parse(src: &str) -> Result<Config, ConfigError> {
    Ok(toml::from_str(src)?)
}

pub fn to_toml(config: &Config) -> Result<String, ConfigError> {
    Ok(toml::to_string_pretty(config)?)
}

/// Checks a parsed config against the bar, the spacing rules and the theme it will be drawn with.
pub fn validate(config: &Config, size: BarSize, rules: &SpacingRules, theme: &BarTheme) -> Vec<ConfigIssue> {
    let mut issues = Vec::new();
    if config.version != CONFIG_VERSION {
        issues.push(ConfigIssue::Version(config.version));
    }
    if config.layout.layer(Chord::Default).is_none() {
        issues.push(ConfigIssue::NoDefaultLayer);
    }
    let mut chords = BTreeSet::new();
    for layer in &config.layout.layers {
        let chord = layer.chord;
        if !chords.insert(chord) {
            issues.push(ConfigIssue::DuplicateLayer(chord));
        }
        let mut ids = BTreeSet::new();
        for b in layer.buttons() {
            if !b.id.is_valid() {
                issues.push(ConfigIssue::BadId { chord, id: b.id.clone() });
            }
            if !ids.insert(&b.id) {
                issues.push(ConfigIssue::DuplicateId { chord, id: b.id.clone() });
            }
            if b.label.trim().is_empty() {
                issues.push(ConfigIssue::EmptyLabel { chord, id: b.id.clone() });
            }
            for color in [&b.style.fill, &b.style.text, &b.style.border].into_iter().flatten() {
                if theme.resolve(color).is_none() {
                    issues.push(ConfigIssue::UnknownColor { chord, id: b.id.clone(), color: color.clone() });
                }
            }
        }
        if let Err(error) = place(layer, size, rules) {
            issues.push(ConfigIssue::Layout { chord, error });
        }
    }
    issues
}

/// Parses and validates config text: the config, or every issue (a parse error is one `Syntax` issue).
pub fn parse_valid(
    src: &str,
    size: BarSize,
    rules: &SpacingRules,
    theme: &BarTheme,
) -> Result<Config, Vec<ConfigIssue>> {
    let config = parse(src).map_err(|e| vec![ConfigIssue::Syntax(e.to_string())])?;
    let issues = validate(&config, size, rules, theme);
    if issues.is_empty() { Ok(config) } else { Err(issues) }
}

#[cfg(test)]
mod tests;
