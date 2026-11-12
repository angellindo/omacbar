//! Group cards are a working list, independent of physical button widths.

use crate::app::{State, pressable};
use crate::shell::{layer_header, words};
use rune::{Cx, Drag, El, Phase, ScrollSpec, Tx, col, el, row};
use tb_core::reorder::{Slot, move_button};
use tb_core::{Button, ButtonId, Group, Layer};
use tb_render::view::color;

type Target = (ButtonId, [f32; 4]);

pub(crate) fn workspace(s: &State, cx: &mut Cx<State>, height: f32) -> El {
    let targets: Vec<_> = s
        .layer()
        .into_iter()
        .flat_map(Layer::buttons)
        .filter_map(|b| cx.window_layout(&format!("row-{}", b.id)).map(|r| (b.id.clone(), r)))
        .collect();
    let mut content = col().w(624.0).ml(24.0).child(layer_header(s));
    if let Some(layer) = s.layer() {
        for (index, group) in layer.groups.iter().enumerate() {
            content = content.child(group_cards(s, index, group, &targets).mb(14.0));
        }
    }
    content = content
        .child(
            pressable(&s.theme, "add-group", "+ add group", false)
                .size(624.0, 38.0)
                .on_press(|s: &mut State| s.add(true)),
        )
        .child(
            pressable(&s.theme, "add-button", "+ add button", false)
                .w(624.0)
                .mt(8.0)
                .on_press(|s: &mut State| s.add(false)),
        )
        .pb(24.0);
    col().key("workspace").size(672.0, height).min_w(672.0).child(
        col()
            .key("workspace-scroll")
            .size(672.0, height)
            .scroll(ScrollSpec { stick_bottom: false })
            .child(content),
    )
}

fn group_cards(s: &State, index: usize, group: &Group, targets: &[Target]) -> El {
    let count = group.buttons.len();
    let columns = count.clamp(1, 3);
    let width = (624.0 - 6.0 * (columns - 1) as f32) / columns as f32;
    let mut group_view = col().key(format!("group-{index}")).w(624.0).gap(0.0).child(
        row()
            .w(624.0)
            .h(20.0)
            .justify_between()
            .child(words(s, "", &format!("GROUP {} · {count} buttons", index + 1)).w(420.0))
            .child(
                pressable(&s.theme, &format!("add-button-group-{index}"), "+ button", false)
                    .w(104.0)
                    .h(20.0)
                    .padding(1.0)
                    .on_press(move |s: &mut State| s.add_to_group(index)),
            ),
    );
    for (n, chunk) in group.buttons.chunks(3).enumerate() {
        group_view = group_view.child(
            row()
                .w(624.0)
                .gap(6.0)
                .mt(if n == 0 { 0.0 } else { 8.0 })
                .children(chunk.iter().map(|b| card(s, b, width, targets.to_vec()))),
        );
    }
    if count == 0 {
        group_view = group_view.child(words(s, "", "empty group · add its first button").h(53.0));
    }

    group_view
}

fn card(s: &State, b: &Button, width: f32, targets: Vec<Target>) -> El {
    let selected = s.selected.as_ref() == Some(&b.id);
    let invalid = s.button_invalid(&b.id);
    let id = b.id.clone();
    el().key(format!("row-{}", b.id))
        .role("button")
        .label(b.label.clone())
        .size(width, 53.0)
        .fill(color(s.theme.button_fill))
        .border(1.0, color(s.theme.button_border))
        .on_drag(move |s: &mut State, d: &mut Drag, tx: &mut Tx| card_drag(s, &id, &targets, d, tx))
        .child(
            col().w(width).px(10.0).pt(7.0).child(words(s, "", &b.label).w(width - 20.0).truncate()).child(
                words(s, "", &format!("{} · {}{}", b.id, b.width, if invalid { " · ! issue" } else { "" }))
                    .w(width - 20.0)
                    .truncate(),
            ),
        )
        .children(selected.then(|| {
            el().key(format!("card-selection-{}", b.id))
                .absolute(0.0, 0.0)
                .size(width, 53.0)
                .border(2.0, color(s.theme.accent))
        }))
        .children(invalid.then(|| {
            el().key(format!("card-error-{}", b.id))
                .absolute(3.0, 3.0)
                .size(width - 6.0, 47.0)
                .border(1.0, color(s.error_color()))
        }))
}

fn card_drag(s: &mut State, id: &ButtonId, targets: &[Target], d: &mut Drag, tx: &mut Tx) {
    match d.phase {
        Phase::Down => {
            s.select(Some(id.clone()));
            tx.blur();
        }
        Phase::Move => {}
        Phase::Up => {
            if d.cancelled() || d.dx.abs() + d.dy.abs() < 4.0 {
                return;
            }
            let [x, y] = d.window_point();
            let target =
                targets.iter().find(|(_, r)| x >= r[0] && x <= r[0] + r[2] && y >= r[1] && y <= r[1] + r[3]);
            let Some((target, rect)) = target.filter(|(target, _)| target != id) else { return };
            if let Some(layer) = s.layer_mut() {
                move_card(layer, id, target, x < rect[0] + rect[2] / 2.0);
            }
        }
    }
}

fn slot(layer: &Layer, id: &ButtonId) -> Option<Slot> {
    layer
        .groups
        .iter()
        .enumerate()
        .find_map(|(group, g)| g.buttons.iter().position(|b| &b.id == id).map(|index| Slot { group, index }))
}

fn move_card(layer: &mut Layer, id: &ButtonId, target: &ButtonId, before: bool) {
    let (Some(from), Some(at)) = (slot(layer, id), slot(layer, target)) else { return };
    let removed_before = usize::from(from.group == at.group && from.index < at.index);
    let to = Slot { group: at.group, index: at.index + usize::from(!before) - removed_before };
    if move_button(layer, from, to).is_ok() && layer.groups[from.group].buttons.is_empty() {
        layer.groups.remove(from.group);
    }
}
