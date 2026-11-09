//! The edit panel: the selected button's fields (label, width, colours, action), each applied to
//! the config as it is typed, so the preview and the issue list follow live; the toolbar that
//! adds and removes buttons and groups and saves; and every `ConfigIssue` of the config.

use rune::{El, TextEdit, Tx, col, edit_access, row, text, text_input};
use tb_core::config::validate;
use tb_core::edit::{add_button, add_group, remove_button};
use tb_core::{Action, BarSize, Button, SpacingRules, Width};
use tb_render::view::color;

use crate::app::{State, body, pressable};

/// One text field of the edit panel.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Field {
    Label,
    Width,
    Fill,
    Text,
    Border,
    Action,
}

/// What an action does, chosen with the `action-*` buttons.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ActionKind {
    None,
    Key,
    Command,
}

impl ActionKind {
    fn of(action: &Action) -> ActionKind {
        match action {
            Action::None => ActionKind::None,
            Action::Key { .. } => ActionKind::Key,
            Action::Command { .. } => ActionKind::Command,
        }
    }

    /// The action of this kind that the action field's `text` describes.
    fn action(self, text: &str) -> Action {
        match self {
            ActionKind::None => Action::None,
            ActionKind::Key => Action::Key { keys: key_names(text) },
            ActionKind::Command => Action::Command { run: text.to_owned() },
        }
    }
}

/// `+`-separated evdev key names, spaces trimmed and empty parts dropped: `LeftCtrl + F5`.
fn key_names(text: &str) -> Vec<String> {
    text.split('+').map(str::trim).filter(|k| !k.is_empty()).map(str::to_owned).collect()
}

/// The text an action shows in the action field: keys joined by `+`, or the command line.
fn action_text(action: &Action) -> String {
    match action {
        Action::Key { keys } => keys.join("+"),
        Action::Command { run } => run.clone(),
        Action::None => String::new(),
    }
}

/// A colour field's text as a style override: empty means none.
fn colour(text: &str) -> Option<String> {
    let text = text.trim();
    (!text.is_empty()).then(|| text.to_owned())
}

/// The edit panel's text buffers for the selected button, refilled when the selection changes.
#[derive(Default)]
pub(crate) struct Fields {
    label: TextEdit,
    width: TextEdit,
    fill: TextEdit,
    text: TextEdit,
    border: TextEdit,
    action: TextEdit,
    /// The width text does not parse; the button keeps its last width.
    width_error: bool,
}

impl Fields {
    pub(crate) fn of(button: Option<&Button>) -> Fields {
        let Some(b) = button else { return Fields::default() };
        let edit = |s: Option<&str>| TextEdit::new(s.unwrap_or_default());
        Fields {
            label: TextEdit::new(b.label.as_str()),
            width: TextEdit::new(b.width.to_string()),
            fill: edit(b.style.fill.as_deref()),
            text: edit(b.style.text.as_deref()),
            border: edit(b.style.border.as_deref()),
            action: TextEdit::new(action_text(&b.action)),
            width_error: false,
        }
    }

    fn edit(&mut self, field: Field) -> &mut TextEdit {
        match field {
            Field::Label => &mut self.label,
            Field::Width => &mut self.width,
            Field::Fill => &mut self.fill,
            Field::Text => &mut self.text,
            Field::Border => &mut self.border,
            Field::Action => &mut self.action,
        }
    }

    fn get(&self, field: Field) -> &TextEdit {
        match field {
            Field::Label => &self.label,
            Field::Width => &self.width,
            Field::Fill => &self.fill,
            Field::Text => &self.text,
            Field::Border => &self.border,
            Field::Action => &self.action,
        }
    }
}

impl State {
    /// Applies the text of `field` to the selected button.
    fn apply(&mut self, field: Field) {
        let text = self.fields.get(field).text.clone();
        let width = (field == Field::Width).then(|| text.parse::<Width>());
        if let Some(parsed) = &width {
            self.fields.width_error = parsed.is_err();
        }
        let Some(b) = self.selected_button_mut() else { return };
        match field {
            Field::Label => b.label = text,
            Field::Width => {
                if let Some(Ok(w)) = width {
                    b.width = w;
                }
            }
            Field::Fill => b.style.fill = colour(&text),
            Field::Text => b.style.text = colour(&text),
            Field::Border => b.style.border = colour(&text),
            // The action field is hidden for `none`, so it only ever edits a key or command.
            Field::Action => b.action = ActionKind::of(&b.action).action(&text),
        }
    }

    fn set_action_kind(&mut self, kind: ActionKind) {
        let text = self.fields.action.text.clone();
        if let Some(b) = self.selected_button_mut() {
            b.action = kind.action(&text);
        }
    }

    /// Adds a button (or a group holding one) after the selected button and selects it.
    pub(crate) fn add(&mut self, group: bool) {
        let after = self.selected.clone();
        let Some(layer) = self.layer_mut() else { return };
        let id = if group { add_group(layer, after.as_ref()) } else { add_button(layer, after.as_ref()) };
        self.select(Some(id));
    }

    fn remove(&mut self) {
        let Some(id) = self.selected.clone() else { return };
        if let Some(layer) = self.layer_mut() {
            remove_button(layer, &id);
        }
        self.select(None);
    }
}

/// The edit panel for the selected button, or a hint when nothing is selected.
pub(crate) fn panel(s: &State) -> El {
    let t = &s.theme;
    let Some(b) = s.selected_button() else {
        return col().key("panel").child(
            text("press a button in the bar or the list to edit it")
                .key("panel-empty")
                .font(body())
                .color(color(t.button_text)),
        );
    };
    let kind = ActionKind::of(&b.action);
    let kinds = [
        ("action-none", "none", ActionKind::None),
        ("action-key", "key", ActionKind::Key),
        ("action-command", "command", ActionKind::Command),
    ];
    let kind_buttons = row().gap(8.0).children(kinds.into_iter().map(|(key, label, k)| {
        pressable(t, key, label, k == kind).on_press(move |s: &mut State| s.set_action_kind(k))
    }));
    col()
        .key("panel")
        .gap(6.0)
        .child(labelled(
            s,
            "id",
            text(b.id.to_string()).key("panel-id").font(body()).color(color(t.button_text)),
        ))
        .child(labelled(s, "label", field(s, "field-label", Field::Label)))
        .child(labelled(s, "width", field(s, "field-width", Field::Width)))
        .children(s.fields.width_error.then(|| note(s, "width-error", "not a width: auto, span N or px N")))
        .children(colour_field(s, "fill", Field::Fill, b.style.fill.as_deref()))
        .children(colour_field(s, "text", Field::Text, b.style.text.as_deref()))
        .children(colour_field(s, "border", Field::Border, b.style.border.as_deref()))
        .child(labelled(s, "action", kind_buttons))
        .children(
            (kind != ActionKind::None).then(|| labelled(s, "", field(s, "field-action", Field::Action))),
        )
}

/// A colour field and, for a hex value the theme draws, the note that it ignores theme switches.
fn colour_field(s: &State, name: &str, which: Field, value: Option<&str>) -> Vec<El> {
    let hex = value.is_some_and(|v| v.starts_with('#') && s.theme.resolve(v).is_some());
    let mut out = vec![labelled(s, name, field(s, &format!("field-{name}"), which))];
    if hex {
        out.push(note(s, &format!("hex-{name}"), "hex: does not follow theme switches"));
    }
    out
}

fn labelled(s: &State, name: &str, control: El) -> El {
    row().gap(8.0).child(text(name).w(64.0).font(body()).color(color(s.theme.button_text))).child(control)
}

fn note(s: &State, key: &str, words: &str) -> El {
    text(words).key(key).font(body()).color(color(s.theme.accent))
}

/// A one-line text field bound to the panel buffer of `field`; every change is applied at once.
fn field(s: &State, key: &str, which: Field) -> El {
    let t = &s.theme;
    text_input(s.fields.get(which), body())
        .single_line()
        .key(key)
        .w(280.0)
        .padding(4.0)
        .fill(color(t.button_fill))
        .border(1.0, color(t.button_border))
        .color(color(t.button_text))
        .on_edit(edit_access(move |s: &mut State| s.fields.edit(which)))
        .on_input_change(move |s: &mut State, _: &mut Tx| s.apply(which))
}

/// Add a button or a group after the selected button, remove it, save the config.
pub(crate) fn toolbar(s: &State) -> El {
    let t = &s.theme;
    row()
        .gap(8.0)
        .child(pressable(t, "add-button", "add button", false).on_press(|s: &mut State| s.add(false)))
        .child(pressable(t, "add-group", "add group", false).on_press(|s: &mut State| s.add(true)))
        .child(pressable(t, "remove-button", "remove button", false).on_press(|s: &mut State| s.remove()))
        .child(pressable(t, "save", "save", false).on_press(State::save))
}

/// Every `ConfigIssue` of the whole config, checked as `save` checks it.
pub(crate) fn issues(s: &State) -> El {
    let t = &s.theme;
    let found = validate(&s.config, BarSize::MACBOOK_PRO, &SpacingRules::STANDARD, &s.theme);
    let list = col().key("issues").gap(2.0);
    if found.is_empty() {
        return list.child(text("no issues").key("no-issues").font(body()).color(color(t.button_text)));
    }
    list.children(found.iter().enumerate().map(|(n, issue)| {
        text(issue.to_string()).key(format!("issue-{n}")).w_full().wrap().font(body()).color(color(t.accent))
    }))
}
