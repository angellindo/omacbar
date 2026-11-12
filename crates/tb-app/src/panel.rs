//! The selected button's fields; all validation and width measurement use the shared domain.

use crate::app::{State, body, pressable};
use crate::diagnostics::button_issues;
use crate::drafts::{ActionKind, Field, WidthMode};
use crate::shell::words;
use rune::{El, Tx, col, edit_access, row, text_input};
use tb_core::{Action, BarSize, SpacingRules, place};
use tb_render::view::color;

pub(crate) fn panel(s: &State) -> El {
    let Some(b) = s.selected_button() else {
        return col()
            .key("panel")
            .w(320.0)
            .px(21.0)
            .pr(20.0)
            .pt(23.0)
            .child(words(s, "edit-title", "EDIT BUTTON"))
            .child(words(s, "panel-empty", "press a button in the bar or the list to edit it").mt(34.0));
    };
    col()
        .key("panel")
        .w(320.0)
        .px(21.0)
        .pr(20.0)
        .pt(23.0)
        .pb(24.0)
        .child(
            col()
                .h(52.0)
                .child(words(s, "edit-title", "EDIT BUTTON"))
                .child(words(s, "panel-id", b.id.as_str()).mt(2.0)),
        )
        .child(words(s, "label-caption", "Label").h(20.0))
        .child(field(s, "field-label", Field::Label, false).mt(7.0))
        .child(words(s, "label-room", &label_room(s)).mt(6.0))
        .child(width_controls(s).mt(16.0))
        .child(action_controls(s).mt(16.0))
        .child(colours(s).mt(16.0))
        .child(button_issues(s).mt(8.0))
        .child(
            pressable(&s.theme, "remove-button", "remove button", false)
                .w(279.0)
                .h(32.0)
                .mt(18.0)
                .on_press(State::remove),
        )
}

fn width_controls(s: &State) -> El {
    let mode = s.fields.mode;
    let choices = [
        ("width-auto", "auto", WidthMode::Auto),
        ("width-span", "span", WidthMode::Span),
        ("width-px", "px", WidthMode::Px),
    ];
    let resolved = resolved_width(s)
        .map_or_else(|| "resolved width unavailable".to_owned(), |w| format!("resolved {w} px"));
    col()
        .w(279.0)
        .gap(6.0)
        .child(row().w(279.0).h(31.0).gap(6.0).children(choices.into_iter().map(|(key, label, choice)| {
            pressable(&s.theme, key, label, mode == choice)
                .w(89.0)
                .h(31.0)
                .padding(6.0)
                .on_press(move |s: &mut State| s.set_width_mode(choice))
        })))
        .child(field(s, "field-width", Field::Width, false))
        .children((mode != WidthMode::Auto).then(|| {
            row()
                .w(279.0)
                .gap(6.0)
                .child(
                    words(
                        s,
                        "width-value-caption",
                        if mode == WidthMode::Span {
                            "shares (1–4 suggested)"
                        } else {
                            "pixels (minimum 64)"
                        },
                    )
                    .w(177.0),
                )
                .child(field(s, "field-width-value", Field::WidthValue, false).w(96.0))
        }))
        .children(s.fields.width_error.then(|| {
            words(s, "width-error", "! not a width: auto, span N or px N").color(color(s.error_color()))
        }))
        .child(words(s, "width-resolved", &resolved))
}

fn resolved_width(s: &State) -> Option<u16> {
    let id = s.selected.as_ref()?;
    place(s.layer()?, BarSize::MACBOOK_PRO, &SpacingRules::STANDARD)
        .ok()?
        .into_iter()
        .find(|p| &p.id == id)
        .map(|p| p.rect.width)
}

fn label_room(s: &State) -> String {
    let Some(b) = s.selected_button() else { return "label room unavailable".to_owned() };
    let measured = tb_render::label_width(&b.label, s.theme.font_size);
    resolved_width(s).map_or_else(
        || format!("label {measured} px · room unavailable"),
        |width| {
            format!(
                "label {measured} px / room {} px",
                width.saturating_sub(SpacingRules::STANDARD.label_padding.saturating_mul(2))
            )
        },
    )
}

fn action_controls(s: &State) -> El {
    let Some(b) = s.selected_button() else { return col() };
    let kind = ActionKind::of(&b.action);
    let choices = [
        ("action-none", "none", ActionKind::None),
        ("action-key", "key", ActionKind::Key),
        ("action-command", "command", ActionKind::Command),
    ];
    let buttons = choices.into_iter().map(|(key, label, choice)| {
        let button = pressable(&s.theme, key, label, kind == choice)
            .h(31.0)
            .padding(5.0)
            .w(if choice == ActionKind::Command { 101.0 } else { 83.0 });
        if choice == ActionKind::Key && !s.key_choice_allowed() {
            button.disabled(true).opacity(0.5)
        } else {
            button.on_press(move |s: &mut State| s.set_action_kind(choice))
        }
    });
    let policy = if s.chord.holds_modifier() {
        if b.id.as_str() == "esc" && matches!(b.action, Action::Key { .. }) {
            "Existing ESC key exception: held modifiers still combine (modifier leak). Prefer a command."
        } else {
            "Modifier leak: keys combine with the held modifier. New key actions are unavailable; use command or none. Existing key details are read-only."
        }
    } else {
        "none is a no-op · key or command runs only when used on the bar"
    };
    col()
        .w(279.0)
        .gap(6.0)
        .child(words(s, "action-caption", "Action"))
        .child(row().w(279.0).h(31.0).gap(6.0).children(buttons))
        .child(words(s, "action-policy", policy))
        .children((kind != ActionKind::None).then(|| {
            field(s, "field-action", Field::Action, kind == ActionKind::Key && !s.key_choice_allowed())
                .h(34.0)
        }))
}

fn colours(s: &State) -> El {
    let Some(b) = s.selected_button() else { return col() };
    let mut view = col().w(279.0).gap(6.0).child(words(
        s,
        "color-caption",
        "Colours · theme token or #hex (empty inherits)",
    ));
    for (name, which, value) in [
        ("fill", Field::Fill, &b.style.fill),
        ("text", Field::Text, &b.style.text),
        ("border", Field::Border, &b.style.border),
    ] {
        view = view.child(
            row()
                .w(279.0)
                .gap(6.0)
                .child(words(s, "", name).w(52.0))
                .child(field(s, &format!("field-{name}"), which, false).w(221.0)),
        );
        if value.as_ref().is_some_and(|v| v.starts_with('#') && s.theme.resolve(v).is_some()) {
            view = view.child(words(s, &format!("hex-{name}"), "hex: does not follow theme switches"));
        }
    }
    view
}

fn field(s: &State, key: &str, which: Field, readonly: bool) -> El {
    let input = text_input(s.fields.get(which), body())
        .single_line()
        .key(key)
        .w(279.0)
        .h(32.0)
        .padding(5.0)
        .fill(color(s.theme.button_fill))
        .border(1.0, color(s.theme.button_border))
        .color(color(s.theme.button_text));
    if readonly {
        return input.disabled(true);
    }
    input
        .on_edit(edit_access(move |s: &mut State| s.fields.edit(which)))
        .on_input_change(move |s: &mut State, _: &mut Tx| s.apply(which))
}
