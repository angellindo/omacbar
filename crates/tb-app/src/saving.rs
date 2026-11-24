//! A baseline tracks actual config differences, and only successful explicit saves update time.
//! A save refused because the file changed on disk (a conflict, ADR-0013) never loses either
//! version: a clean editor shows the newer file, a dirty one keeps its draft until the user
//! reloads or overwrites.

use std::io;

use crate::app::{NO_HOME, State};
use crate::drafts::Fields;
use tb_config::{ConfigFileError, DiskVersion, Reason};
use tb_core::config::Config;
use tb_core::{ButtonId, Chord};

pub(crate) struct Saving {
    pub(crate) baseline: Option<Config>,
    /// Why no save may replace the file for this session (the selected theme failed at open).
    pub(crate) blocked: Option<String>,
    pub(crate) last_saved: Option<u64>,
    pub(crate) saved_this_session: bool,
    /// The disk version a dirty save was refused over; reload and overwrite are offered.
    pub(crate) conflict: Option<DiskVersion>,
    clock: fn() -> Option<u64>,
}

impl Default for Saving {
    fn default() -> Self {
        Self {
            baseline: None,
            blocked: None,
            last_saved: None,
            saved_this_session: false,
            conflict: None,
            clock: || None,
        }
    }
}

impl State {
    /// Injects epoch seconds for successful explicit saves; opening files never samples it.
    pub fn set_save_clock(&mut self, clock: fn() -> Option<u64>) {
        self.saving.clock = clock;
    }

    pub(crate) fn invalid_widths(&self) -> Vec<(Chord, ButtonId)> {
        let mut found: Vec<_> = self
            .drafts
            .iter()
            .filter(|((chord, id), fields)| {
                fields.width_error
                    && self.config.layout.layer(*chord).is_some_and(|l| l.buttons().any(|b| &b.id == id))
            })
            .map(|((chord, id), _)| (*chord, id.clone()))
            .collect();
        if self.fields.width_error
            && let Some(id) = &self.selected
        {
            found.push((self.chord, id.clone()));
        }
        found
    }

    pub(crate) fn layer_dirty(&self, chord: Chord) -> bool {
        let changed = self
            .saving
            .baseline
            .as_ref()
            .is_none_or(|b| b.layout.layer(chord) != self.config.layout.layer(chord));
        changed || self.invalid_widths().iter().any(|(c, _)| *c == chord)
    }

    pub(crate) fn dirty(&self) -> bool {
        self.saving.baseline.as_ref() != Some(&self.config) || !self.invalid_widths().is_empty()
    }

    pub(crate) fn dirty_summary(&self) -> String {
        let layers = Chord::ALL.into_iter().filter(|&c| self.layer_dirty(c)).count();
        if !self.dirty() {
            return "no pending changes".to_owned();
        }
        if self.saving.baseline.is_none() {
            return "unsaved replacement config (no accepted baseline)".to_owned();
        }
        if layers == 0 {
            return "pending config changes · 0 layers (theme/version)".to_owned();
        }
        format!("pending changes in {layers} layer(s)")
    }

    pub(crate) fn save_blocked_reason(&self) -> Option<String> {
        if self.file.is_none() {
            return Some(NO_HOME.to_owned());
        }
        if let Some(reason) = &self.saving.blocked {
            return Some(reason.clone());
        }
        let issues = self.effective_issues();
        if issues.is_empty() {
            return None;
        }
        Some(format!(
            "not saved: {} issue(s); fix {}",
            issues.len(),
            issues.iter().map(|i| i.message.as_str()).collect::<Vec<_>>().join("; ")
        ))
    }

    /// One guard for both the actual Save control and Ctrl+S, including every cached draft.
    pub(crate) fn save(&mut self) {
        if let Some(reason) = self.save_blocked_reason() {
            self.status = reason;
            return;
        }
        let reason = self.write_reason(Reason::Save);
        let Some(file) = &mut self.file else { return };
        // The selected theme file may have changed since open: resolve it again; Omarchy is the
        // active theme, never the theme a previously shown config selected.
        match file.save_resolved_for(self.config.clone(), &self.active, reason) {
            Ok(theme) => {
                self.theme = theme;
                self.saved(None);
            }
            Err(ConfigFileError::Changed(version)) => self.refused_as_changed(version),
            Err(e) => self.not_saved(&e),
        }
    }

    /// The conflict resolution that keeps the draft: overwrite the version the refusal named,
    /// keeping its bytes in a backup. A further change on disk is a new conflict.
    pub(crate) fn overwrite(&mut self) {
        if let Some(reason) = self.save_blocked_reason() {
            self.status = reason;
            return;
        }
        let reason = self.write_reason(Reason::Overwrite);
        let (Some(file), Some(version)) = (&mut self.file, self.saving.conflict.clone()) else { return };
        match file.save_over_resolved_for(self.config.clone(), &self.active, &version, reason) {
            Ok(theme) => {
                // Exactly the theme this overwrite was validated against: no second read.
                self.theme = theme;
                let backup = file.backup().map(|b| b.display().to_string());
                self.saved(backup);
            }
            Err(ConfigFileError::Changed(now)) => self.keep_draft(now),
            Err(e) => self.not_saved(&e),
        }
    }

    /// The conflict resolution that drops the draft: show the file on disk, if it resolves.
    pub(crate) fn reload(&mut self) {
        self.status = match self.adopt() {
            Ok(shown) => format!("reloaded {shown}: your changes were discarded"),
            Err(why) => format!("not reloaded: {why}; your changes are kept"),
        };
    }

    /// A write while the default buttons are a pending draft is a factory reset's save.
    const fn write_reason(&self, otherwise: Reason) -> Reason {
        if self.reset_pending() { Reason::FactoryReset } else { otherwise }
    }

    fn saved(&mut self, backup: Option<String>) {
        let Some(file) = &self.file else { return };
        let kept = backup.map(|b| format!("; the replaced version is kept in {b}")).unwrap_or_default();
        let before_reset = self
            .reset
            .take()
            .and_then(|_| file.last_generation_path())
            .map(|g| format!("; the file before the factory reset is kept in {}", g.display()))
            .unwrap_or_default();
        self.status = format!("saved config {}{kept}{before_reset}", file.path().display());
        self.saving.baseline = Some(self.config.clone());
        self.saving.last_saved = (self.saving.clock)();
        self.saving.saved_this_session = true;
        self.saving.conflict = None;
        self.refresh_history();
    }

    fn not_saved(&mut self, e: &ConfigFileError) {
        let Some(file) = &self.file else { return };
        let shown = file.path().display();
        self.status = match e {
            ConfigFileError::Invalid(issues) => {
                let fix = issues.iter().map(ToString::to_string).collect::<Vec<_>>().join("; ");
                format!("not saved {shown}: {} issue(s); fix {fix}", issues.len())
            }
            e => format!("not saved {shown}: {e}"),
        };
    }

    /// A clean editor has nothing to lose: it shows the newer file instead. A dirty one keeps
    /// everything it shows and offers the two resolutions.
    fn refused_as_changed(&mut self, version: DiskVersion) {
        if self.dirty() {
            self.keep_draft(version);
            return;
        }
        match self.adopt() {
            Ok(_) => "config changed on disk; showing the newer version (nothing saved)"
                .clone_into(&mut self.status),
            Err(why) => {
                self.saving.conflict = Some(version);
                self.status = format!("config changed on disk: {why}; nothing saved");
            }
        }
    }

    fn keep_draft(&mut self, version: DiskVersion) {
        let name = self.file_name();
        let what = if version.is_absent() { "was removed" } else { "changed on disk" };
        self.status = format!("not saved: {name} {what} since it was opened; your changes are kept");
        self.saving.conflict = Some(version);
    }

    /// Reads the file through a fresh `ConfigFile` and, only if it resolves, makes it the editor's
    /// file, config and baseline. A failure changes nothing: the editor's file still remembers the
    /// version it read, so bytes the user never accepted cannot authorize a later save.
    pub(crate) fn adopt(&mut self) -> Result<String, String> {
        let Some(file) = &self.file else { return Err(NO_HOME.to_owned()) };
        let name = self.file_name();
        let mut fresh = file.reopened();
        let theme = match fresh.reload_resolved(&self.active) {
            Ok(theme) => theme,
            Err(ConfigFileError::Io { source, .. }) if source.kind() == io::ErrorKind::NotFound => {
                return Err(format!("{name} was removed"));
            }
            Err(ConfigFileError::Invalid(issues)) => {
                return Err(format!("{name} has {} issue(s)", issues.len()));
            }
            Err(e) => return Err(e.to_string()),
        };
        let shown = fresh.path().display().to_string();
        self.theme = theme;
        self.config = fresh.config().clone();
        self.saving.baseline = Some(self.config.clone());
        self.saving.conflict = None;
        self.file = Some(fresh);
        self.reset = None;
        self.drafts.clear();
        self.drag = None;
        if self.selected_button().is_none() {
            self.selected = None;
        }
        self.fields = Fields::of(self.selected_button());
        self.opened = self.config.clone();
        self.refresh_history();
        Ok(shown)
    }

    pub(crate) fn file_name(&self) -> String {
        let name = self.file.as_ref().and_then(|f| f.path().file_name());
        name.map_or_else(|| "the config file".to_owned(), |n| n.to_string_lossy().into_owned())
    }

    pub(crate) fn last_saved_text(&self) -> String {
        self.saving.last_saved.map_or_else(
            || {
                if self.saving.saved_this_session {
                    "saved this session · time unavailable".to_owned()
                } else {
                    "not saved this session".to_owned()
                }
            },
            |seconds| {
                let day = seconds % 86_400;
                format!("last saved {:02}:{:02}:{:02} UTC", day / 3_600, day / 60 % 60, day % 60)
            },
        )
    }
}
