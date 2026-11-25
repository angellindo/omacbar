//! Loading the default buttons (a factory reset) is a draft: only the editor and its preview
//! change until a save. The editor before the reset (config with its pending edits, every
//! button's field text, selection and shown layer) is kept whole, so cancel brings it back
//! exactly; the theme source and version of the shown config are never part of the reset.

use std::collections::BTreeMap;

use rune::{El, col};
use tb_config::DiskVersion;
use tb_core::config::{Config, ThemeSource};
use tb_core::{ButtonId, Chord};

use crate::app::{State, pressable};
use crate::drafts::Fields;
use crate::shell::words;

/// What the editor held when the default buttons were loaded.
pub(crate) struct BeforeReset {
    config: Config,
    conflict: Option<DiskVersion>,
    chord: Chord,
    selected: Option<ButtonId>,
    fields: Fields,
    drafts: BTreeMap<(Chord, ButtonId), Fields>,
}

impl State {
    /// Shows the factory layout for the editor's theme as unsaved changes. Nothing is written; a
    /// second reset keeps the editor from before the first one.
    pub(crate) fn reset_factory(&mut self) {
        if self.reset.is_none() {
            self.reset = Some(BeforeReset {
                config: self.config.clone(),
                // An overwrite offered for the previous draft does not apply to the preset.
                conflict: self.saving.conflict.take(),
                chord: self.chord,
                selected: self.selected.clone(),
                fields: std::mem::take(&mut self.fields),
                drafts: std::mem::take(&mut self.drafts),
            });
        }
        self.config.layout = Config::default_for(&self.theme).layout;
        self.chord = Chord::Default;
        self.selected = None;
        self.fields = Fields::default();
        self.drafts.clear();
        self.drag = None;
        "default buttons loaded as a draft: nothing is written until you save".clone_into(&mut self.status);
    }

    /// Brings back the editor exactly as it was before the reset. Disk is not touched.
    pub(crate) fn reset_cancel(&mut self) {
        let Some(before) = self.reset.take() else { return };
        self.config = before.config;
        self.saving.conflict = before.conflict;
        self.chord = before.chord;
        self.selected = before.selected;
        self.fields = before.fields;
        self.drafts = before.drafts;
        self.drag = None;
        "default buttons discarded: your previous draft is back".clone_into(&mut self.status);
    }

    pub(crate) const fn reset_pending(&self) -> bool {
        self.reset.is_some()
    }

    /// What a pending reset keeps: the theme source of the shown config.
    fn reset_notice(&self) -> String {
        let source = match &self.config.theme {
            ThemeSource::Omarchy => "Omarchy (the active theme)".to_owned(),
            ThemeSource::File { path } => format!("the theme file {path}"),
        };
        format!(
            "default buttons are a draft: the theme source is kept ({source}); save writes them, \
             keeping your file as a generation; cancel brings back your draft"
        )
    }
}

/// Load the default buttons, and while that draft is pending, what it keeps and its cancel.
pub(crate) fn reset_controls(s: &State) -> El {
    let load = pressable(&s.theme, "reset-factory", "default buttons", false).size(168.0, 34.0);
    let region = col().key("reset").child(load.on_press(State::reset_factory));
    if !s.reset_pending() {
        return region;
    }
    region.child(words(s, "reset-notice", &s.reset_notice()).mt(4.0)).child(
        pressable(&s.theme, "reset-cancel", "cancel reset", false)
            .size(168.0, 34.0)
            .mt(4.0)
            .on_press(State::reset_cancel),
    )
}
