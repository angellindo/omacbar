//! Per-button text buffers survive selection, reorder and action-kind switches.

use rune::TextEdit;
use tb_core::{Action, Button, Width};

use crate::app::State;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Field {
    Label,
    Width,
    WidthValue,
    Fill,
    Text,
    Border,
    Action,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ActionKind {
    None,
    Key,
    Command,
}

impl ActionKind {
    pub(crate) fn of(action: &Action) -> Self {
        match action {
            Action::None => Self::None,
            Action::Key { .. } => Self::Key,
            Action::Command { .. } => Self::Command,
        }
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) enum WidthMode {
    #[default]
    Auto,
    Span,
    Px,
}

/// The selected buffer is moved into the cache before navigating, never copied into another id.
#[derive(Default)]
pub(crate) struct Fields {
    label: TextEdit,
    pub(crate) width: TextEdit,
    width_value: TextEdit,
    fill: TextEdit,
    text: TextEdit,
    border: TextEdit,
    key: TextEdit,
    command: TextEdit,
    pub(crate) kind: Option<ActionKind>,
    pub(crate) mode: WidthMode,
    pub(crate) width_error: bool,
}

impl Fields {
    pub(crate) fn of(button: Option<&Button>) -> Self {
        let Some(b) = button else { return Self::default() };
        let mut out = Self {
            label: TextEdit::new(&b.label),
            width: TextEdit::new(b.width.to_string()),
            fill: TextEdit::new(b.style.fill.as_deref().unwrap_or_default()),
            text: TextEdit::new(b.style.text.as_deref().unwrap_or_default()),
            border: TextEdit::new(b.style.border.as_deref().unwrap_or_default()),
            kind: Some(ActionKind::of(&b.action)),
            ..Self::default()
        };
        match &b.action {
            Action::Key { keys } => out.key = TextEdit::new(keys.join("+")),
            Action::Command { run } => out.command = TextEdit::new(run),
            Action::None => {}
        }
        out.width_controls(b.width);
        out
    }

    fn width_controls(&mut self, width: Width) {
        let (mode, value) = match width {
            Width::Auto => (WidthMode::Auto, String::new()),
            Width::Span(n) => (WidthMode::Span, n.to_string()),
            Width::Px(n) => (WidthMode::Px, n.to_string()),
        };
        self.mode = mode;
        self.width_value = TextEdit::new(value);
    }

    pub(crate) fn edit(&mut self, field: Field) -> &mut TextEdit {
        match field {
            Field::Label => &mut self.label,
            Field::Width => &mut self.width,
            Field::WidthValue => &mut self.width_value,
            Field::Fill => &mut self.fill,
            Field::Text => &mut self.text,
            Field::Border => &mut self.border,
            Field::Action if self.kind == Some(ActionKind::Key) => &mut self.key,
            Field::Action => &mut self.command,
        }
    }

    pub(crate) fn get(&self, field: Field) -> &TextEdit {
        match field {
            Field::Label => &self.label,
            Field::Width => &self.width,
            Field::WidthValue => &self.width_value,
            Field::Fill => &self.fill,
            Field::Text => &self.text,
            Field::Border => &self.border,
            Field::Action if self.kind == Some(ActionKind::Key) => &self.key,
            Field::Action => &self.command,
        }
    }

    fn action(&self, kind: ActionKind) -> Action {
        match kind {
            ActionKind::None => Action::None,
            ActionKind::Key => Action::Key {
                keys: self
                    .key
                    .text
                    .split('+')
                    .map(str::trim)
                    .filter(|s| !s.is_empty())
                    .map(str::to_owned)
                    .collect(),
            },
            ActionKind::Command => Action::Command { run: self.command.text.clone() },
        }
    }
}

impl State {
    pub(crate) fn apply(&mut self, field: Field) {
        if field == Field::WidthValue {
            let prefix = if self.fields.mode == WidthMode::Px { "px" } else { "span" };
            self.fields.width = TextEdit::new(format!("{prefix} {}", self.fields.width_value.text));
        }
        let text = self.fields.get(field).text.clone();
        let parsed = self.fields.width.text.parse::<Width>();
        if matches!(field, Field::Width | Field::WidthValue) {
            self.fields.width_error = parsed.is_err();
            if field == Field::Width
                && let Ok(width) = &parsed
            {
                self.fields.width_controls(*width);
            }
        }
        let action = self.fields.action(self.fields.kind.unwrap_or(ActionKind::None));
        let Some(b) = self.selected_button_mut() else { return };
        match field {
            Field::Label => b.label = text,
            Field::Width | Field::WidthValue => {
                if let Ok(width) = parsed {
                    b.width = width;
                }
            }
            Field::Fill => b.style.fill = colour(&text),
            Field::Text => b.style.text = colour(&text),
            Field::Border => b.style.border = colour(&text),
            Field::Action => b.action = action,
        }
    }

    pub(crate) fn set_width_mode(&mut self, mode: WidthMode) {
        if mode == self.fields.mode && !self.fields.width_error {
            return;
        }
        self.fields.mode = mode;
        self.fields.width = TextEdit::new(match mode {
            WidthMode::Auto => "auto".to_owned(),
            WidthMode::Span => "span 1".to_owned(),
            WidthMode::Px => "px 116".to_owned(),
        });
        self.apply(Field::Width);
    }

    pub(crate) fn key_choice_allowed(&self) -> bool {
        !self.chord.holds_modifier()
    }

    pub(crate) fn set_action_kind(&mut self, kind: ActionKind) {
        if kind == ActionKind::Key && !self.key_choice_allowed() {
            return;
        }
        self.fields.kind = Some(kind);
        let action = self.fields.action(kind);
        if let Some(b) = self.selected_button_mut() {
            b.action = action;
        }
    }
}

fn colour(text: &str) -> Option<String> {
    let value = text.trim();
    (!value.is_empty()).then(|| value.to_owned())
}
