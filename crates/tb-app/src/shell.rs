//! Variant B's three columns and native half-size footer. Each long region owns its scroll.

use crate::app::{State, body, pressable, save_key};
use crate::diagnostics::issues;
use crate::panel::panel;
use crate::preview::preview;
use crate::workspace::workspace;
use rune::{Cx, El, ScrollSpec, col, el, row, text};
use tb_core::{Action, BarSize, Chord, SpacingRules, place};
use tb_render::view::color;

pub(crate) fn words(s: &State, key: &str, message: &str) -> El {
    let view = text(message).font(body()).w_full().wrap().color(color(s.theme.button_text));
    if key.is_empty() { view } else { view.key(key) }
}

pub(crate) const fn canonical(chord: Chord) -> &'static str {
    match chord {
        Chord::Default => "default",
        Chord::Fn => "fn",
        Chord::FnCtrl => "fn-ctrl",
        Chord::FnAlt => "fn-alt",
        Chord::FnShift => "fn-shift",
        Chord::FnSuper => "fn-super",
    }
}

pub(crate) fn view(s: &State, cx: &mut Cx<State>) -> El {
    // Bound each column to the actual available height: Rune keeps wheels in the nearest
    // viewport, so a taller nested viewport could trap controls below a short window.
    let height = cx.layout_size("editor").map_or(760.0, |size| size[1]);
    let body_height = (height - 117.0).max(200.0);
    col().key("editor").w_full().h_full().fill(color(s.theme.background)).scroll_xy().on_key(save_key).child(
        col()
            .w(1200.0)
            .min_w(1200.0)
            .min_h(body_height + 117.0)
            .child(
                row()
                    .w(1200.0)
                    .h(body_height)
                    .min_h(body_height)
                    .child(sidebar(s, body_height))
                    .child(workspace(s, cx, body_height))
                    .child(
                        col()
                            .key("edit-panel-scroll")
                            .size(320.0, body_height)
                            .min_w(320.0)
                            .scroll(ScrollSpec { stick_bottom: false })
                            .child(panel(s)),
                    )
                    .child(
                        el().key("sidebar-divider")
                            .absolute(208.0, 0.0)
                            .size(1.0, body_height)
                            .fill(color(s.theme.button_border)),
                    )
                    .child(
                        el().key("edit-panel-divider")
                            .absolute(880.0, 0.0)
                            .size(1.0, body_height)
                            .fill(color(s.theme.button_border)),
                    ),
            )
            .child(footer(s)),
    )
}

fn sidebar(s: &State, height: f32) -> El {
    let entries = Chord::ALL.into_iter().map(|chord| layer_row(s, chord));
    col().key("sidebar").size(208.0, height).min_w(208.0).child(
        col().key("sidebar-scroll").size(208.0, height).scroll(ScrollSpec { stick_bottom: false }).child(
            col()
                .w(208.0)
                .min_h(643.0)
                .px(20.0)
                .pt(18.0)
                .pb(20.0)
                .child(words(s, "app-title", "Touch Bar"))
                .child(words(s, "config-basename", "config.toml").mt(4.0))
                .child(words(s, "layers-title", "LAYERS").mt(28.0).h(22.0))
                .children(entries)
                .child(col().min_h(240.0).grow(1.0).justify_end().children(
                    s.save_blocked_reason().map(|reason| words(s, "save-blocked-reason", &reason).mb(8.0)),
                ))
                .child(save_control(s))
                .child(words(s, "dirty-summary", &s.dirty_summary()).mt(4.0))
                .child(words(s, "last-saved", &s.last_saved_text()))
                .child(
                    words(s, "status", &s.save_blocked_reason().unwrap_or_else(|| s.status.clone())).mt(6.0),
                )
                .child(issues(s).mt(12.0)),
        ),
    )
}

fn layer_row(s: &State, chord: Chord) -> El {
    let count = s.config.layout.layer(chord).map_or(0, |l| l.buttons().count());
    let invalid = s.effective_issues().iter().any(|i| i.chord == Some(chord));
    let state = if invalid {
        "! error"
    } else if count == 0 {
        "○ empty"
    } else {
        "· valid"
    };
    let tab = format!("tab-{chord:?}").to_lowercase();
    let selected = chord == s.chord;
    el().key(tab)
        .role("button")
        .label(chord.label())
        .size(168.0, 34.0)
        .min_h(34.0)
        .fill(color(if selected { s.theme.button_fill } else { s.theme.background }))
        .border(1.0, color(if selected { s.theme.accent } else { s.theme.background }))
        .on_press_tx(move |s: &mut State, tx: &mut rune::Tx| {
            s.show_layer(chord);
            tx.blur();
        })
        .child(words(s, "", chord.label()).absolute(8.0, 2.0).w(108.0).truncate())
        .child(
            words(s, &format!("layer-state-{}", canonical(chord)), &format!("{state} · {count}"))
                .absolute(8.0, 18.0)
                .w(145.0)
                .truncate()
                .color(color(if invalid { s.error_color() } else { s.theme.button_text })),
        )
        .children(s.layer_dirty(chord).then(|| {
            words(s, &format!("dirty-layer-{}", canonical(chord)), "●")
                .absolute(146.0, 3.0)
                .w(14.0)
                .color(color(s.theme.accent))
        }))
}

fn save_control(s: &State) -> El {
    let button = pressable(&s.theme, "save", "save · Ctrl+S", true).size(168.0, 34.0);
    if s.save_blocked_reason().is_some() {
        button.disabled(true).opacity(0.5)
    } else {
        button.on_press(State::save)
    }
}

pub(crate) fn layer_header(s: &State) -> El {
    let (buttons, groups) = s.layer().map_or((0, 0), |l| (l.buttons().count(), l.groups.len()));
    el().key("layer-header")
        .w(624.0)
        .h(102.0)
        .min_h(102.0)
        .child(words(s, "layer-chord", s.chord.label()).absolute(0.0, 23.0).w(180.0))
        .child(
            words(s, "layer-instruction", "choose a group, then edit a button")
                .absolute(190.0, 23.0)
                .w(434.0),
        )
        .child(
            words(s, "layer-counts", &format!("{buttons} buttons · {groups} groups"))
                .absolute(0.0, 60.0)
                .w(220.0),
        )
        .child(words(s, "layer-widths", &width_summary(s)).absolute(225.0, 60.0).w(399.0))
}

fn width_summary(s: &State) -> String {
    let Some(layer) = s.layer() else { return "empty layer · add a button".to_owned() };
    let Ok(placed) = place(layer, BarSize::MACBOOK_PRO, &SpacingRules::STANDARD) else {
        return "resolved widths unavailable: layer does not fit".to_owned();
    };
    let mut widths: Vec<_> = placed.iter().map(|p| p.rect.width).collect();
    widths.sort_unstable();
    widths.dedup();
    match (widths.first(), widths.last()) {
        (Some(a), Some(b)) if a == b => format!("resolved {a} px"),
        (Some(a), Some(b)) => format!("resolved {a}–{b} px"),
        _ => "empty layer · no resolved widths".to_owned(),
    }
}

fn footer(s: &State) -> El {
    let action = s.selected_button().map_or_else(
        || "select a button to inspect its action".to_owned(),
        |b| match &b.action {
            Action::None => format!("{} · no action", b.id),
            Action::Key { keys } => format!("{} · key {}", b.id, keys.join(" + ")),
            Action::Command { run } => format!("{} · command {run}", b.id),
        },
    );
    el().key("footer").size(1200.0, 117.0).min_h(117.0).fill(color(s.theme.button_fill))
        .child(words(s, "preview-caption", "TOUCH BAR PREVIEW").absolute(24.0, 16.0).w(720.0))
        .child(words(s, "preview-scale", "2170 × 60 · 50%").absolute(960.0, 16.0).w(216.0))
        .child(row().key("preview-scroll").absolute(0.0, 38.0).size(1200.0, 32.0).scroll_x()
            .child(el().w(1200.0).min_w(1200.0).h(32.0)
                .child(el().absolute(56.0, 0.0).size(1087.0, 32.0).border(1.0, color(s.theme.button_border))
                    .child(preview(s).absolute(1.0, 1.0)))))
        .child(words(s, "action-description", &action).absolute(24.0, 83.0).w(1152.0).h(18.0).truncate())
        .child(words(s, "file-status", if s.saving.saved_this_session { "Saved config/file · daemon watches this file; simulator acknowledgement unavailable" } else if s.file.is_some() { "Config file · no save this session; daemon watches this file; simulator acknowledgement unavailable" } else { "Config file unavailable · simulator acknowledgement unavailable" })
            .absolute(24.0, 101.0).w(1152.0).h(16.0).truncate())
}
