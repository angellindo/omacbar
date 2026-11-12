//! A baseline tracks actual config differences, and only successful explicit saves update time.

use crate::app::{NO_HOME, State};
use tb_core::config::Config;
use tb_core::{ButtonId, Chord};

pub(crate) struct Saving {
    pub(crate) baseline: Option<Config>,
    pub(crate) last_saved: Option<u64>,
    pub(crate) saved_this_session: bool,
    clock: fn() -> Option<u64>,
}

impl Default for Saving {
    fn default() -> Self {
        Self { baseline: None, last_saved: None, saved_this_session: false, clock: || None }
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
        let Some(file) = &mut self.file else { return };
        match file.save(self.config.clone(), &self.theme) {
            Ok(()) => {
                self.status = format!("saved config {}", file.path().display());
                self.saving.baseline = Some(self.config.clone());
                self.saving.last_saved = (self.saving.clock)();
                self.saving.saved_this_session = true;
            }
            Err(e) => self.status = format!("not saved {}: {e}", file.path().display()),
        }
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
