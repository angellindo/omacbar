//! Saves that record why they replaced the file: the generation a protected file keeps names the
//! reason (ADR-0015), e.g. the save of a factory reset the configurator loaded as a draft.

use tb_core::BarTheme;
use tb_core::config::{Config, to_toml};

use super::{ConfigFile, ConfigFileError, Content, DiskVersion, WriteKind, check, resolve_theme};
use crate::backup::Reason;

impl ConfigFile {
    /// `save_resolved`, recording `reason` in the generation a protected file keeps of what the
    /// save replaces (a factory reset's save records `Reason::FactoryReset`).
    pub fn save_resolved_for(
        &mut self,
        config: Config,
        active: &BarTheme,
        reason: Reason,
    ) -> Result<BarTheme, ConfigFileError> {
        let theme = resolve_theme(&config.theme, active)?;
        check(&config, &theme)?;
        let seen = self.seen.clone();
        self.swap_as(&seen, Content::Text(&to_toml(&config)?), WriteKind::Save, reason)?;
        self.config = config;
        Ok(theme)
    }

    /// `save_over_resolved`, recording `reason` in the generation a protected file keeps of what
    /// the overwrite replaces (the `.bak` is kept as for every overwrite).
    pub fn save_over_resolved_for(
        &mut self,
        config: Config,
        active: &BarTheme,
        acknowledged: &DiskVersion,
        reason: Reason,
    ) -> Result<BarTheme, ConfigFileError> {
        let theme = resolve_theme(&config.theme, active)?;
        check(&config, &theme)?;
        self.swap_as(acknowledged, Content::Text(&to_toml(&config)?), WriteKind::Overwrite, reason)?;
        self.config = config;
        Ok(theme)
    }
}
