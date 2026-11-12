//! Shared config diagnostics plus unsaved width parse issues, with navigation targets.

use crate::app::State;
use crate::shell::words;
use rune::{El, col};
use tb_core::config::{ConfigIssue, validate, warnings};
use tb_core::{BarSize, ButtonId, Chord, Rgba, SpacingRules};

pub(crate) struct Issue {
    pub(crate) chord: Option<Chord>,
    pub(crate) id: Option<ButtonId>,
    pub(crate) message: String,
}

impl Issue {
    fn of(issue: &ConfigIssue) -> Self {
        let (chord, id) = match issue {
            ConfigIssue::DuplicateId { chord, id }
            | ConfigIssue::BadId { chord, id }
            | ConfigIssue::EmptyLabel { chord, id }
            | ConfigIssue::UnknownColor { chord, id, .. }
            | ConfigIssue::UnknownKey { chord, id, .. }
            | ConfigIssue::NoKeys { chord, id }
            | ConfigIssue::EmptyCommand { chord, id } => (Some(*chord), Some(id.clone())),
            ConfigIssue::DuplicateLayer(chord) | ConfigIssue::Layout { chord, .. } => (Some(*chord), None),
            ConfigIssue::NoDefaultLayer => (Some(Chord::Default), None),
            ConfigIssue::Version(_) | ConfigIssue::Syntax(_) => (None, None),
        };
        Self { chord, id, message: issue.to_string() }
    }
}

impl State {
    pub(crate) fn effective_issues(&self) -> Vec<Issue> {
        let mut found: Vec<_> =
            validate(&self.config, BarSize::MACBOOK_PRO, &SpacingRules::STANDARD, &self.theme)
                .iter()
                .map(Issue::of)
                .collect();
        found.extend(self.invalid_widths().into_iter().map(|(chord, id)| Issue {
            message: format!("layer `{chord:?}`: button `{id}` width is not a width: auto, span N or px N"),
            chord: Some(chord),
            id: Some(id),
        }));
        found
    }

    pub(crate) fn button_invalid(&self, id: &ButtonId) -> bool {
        self.effective_issues()
            .iter()
            .any(|i| i.chord == Some(self.chord) && i.id.as_ref().is_none_or(|b| b == id))
    }

    pub(crate) fn error_color(&self) -> Rgba {
        self.theme.resolve("red").unwrap_or(self.theme.button_text)
    }
}

pub(crate) fn issues(s: &State) -> El {
    let found = s.effective_issues();
    let mut view = col().key("issues").gap(6.0).w_full();
    if found.is_empty() {
        view = view.child(words(s, "no-issues", "no issues"));
    }
    for (n, issue) in found.into_iter().enumerate() {
        view = view.child(
            words(s, &format!("issue-{n}"), &issue.message)
                .color(tb_render::view::color(s.error_color()))
                .role("button")
                .on_press_tx(move |s: &mut State, tx: &mut rune::Tx| {
                    if let Some(chord) = issue.chord {
                        s.show_layer(chord);
                        s.select(issue.id.clone());
                    }
                    tx.blur();
                    tx.scroll_into_view("panel");
                }),
        );
    }
    let found = warnings(&s.config, BarSize::MACBOOK_PRO, &SpacingRules::STANDARD, |label| {
        tb_render::label_width(label, s.theme.font_size)
    });
    for (n, warning) in found.iter().enumerate() {
        view = view.child(words(s, &format!("warning-{n}"), &format!("warning: {warning}")));
    }
    view
}

pub(crate) fn button_issues(s: &State) -> El {
    let mut view = col().key("button-issues").gap(4.0).w_full();
    for issue in s.effective_issues().into_iter().filter(|i| {
        i.chord == Some(s.chord) && i.id.as_ref().is_none_or(|id| s.selected.as_ref() == Some(id))
    }) {
        view = view.child(
            words(s, "", &format!("! {}", issue.message)).color(tb_render::view::color(s.error_color())),
        );
    }
    view
}
