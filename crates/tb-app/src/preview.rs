//! The bar preview: the shown layer exactly as the renderer lays it out, scaled down. Pressing a
//! button selects it; dragging it along the row and releasing drops it before or after the button
//! under the pointer (`tb_core::reorder::drop_button`).

use rune::{Drag, El, Phase, Tx, el, text};
use tb_core::reorder::drop_button;
use tb_core::{BarSize, ButtonId, Rgba, SpacingRules};
use tb_render::{BarScene, SceneButton, build_scene, label_style, view::color};

use crate::app::State;

/// Preview scale: the 2170 px bar shown at half size fits the window.
pub(crate) const PREVIEW_SCALE: f32 = 0.5;

/// A button being dragged in the preview: its id and the pointer's travel in window points.
pub(crate) struct Dragging {
    pub(crate) id: ButtonId,
    pub(crate) dx: f32,
}

fn scene(s: &State) -> Option<BarScene> {
    let layer = s.layer()?;
    build_scene(layer, &s.theme, BarSize::MACBOOK_PRO, &SpacingRules::STANDARD, None).ok()
}

/// The bar exactly as the renderer lays it out, scaled down; the selected button has an accent
/// border, and the dragged one is drawn where the pointer has carried it.
pub(crate) fn preview(s: &State) -> El {
    let t = &s.theme;
    let Some(scene) = scene(s) else {
        return text("this layer does not fit the bar").key("preview").color(color(t.accent));
    };
    let k = PREVIEW_SCALE;
    let label = label_style(f32::from(scene.font_size) * k);
    let dragged = |b: &&SceneButton| s.drag.as_ref().is_some_and(|d| d.id == b.id);
    // The dragged button comes last so it is drawn over the buttons it passes.
    let (lifted, resting): (Vec<_>, Vec<_>) = scene.buttons.iter().partition(dragged);
    el().key("preview")
        .size(f32::from(scene.size.width) * k, f32::from(scene.size.height) * k)
        .fill(color(scene.background))
        .children(resting.into_iter().chain(lifted).map(|b| {
            let dx = s.drag.as_ref().filter(|d| d.id == b.id).map(|d| d.dx);
            let marked = dx.is_some() || s.selected.as_ref() == Some(&b.id);
            let (id, left) = (b.id.clone(), b.rect.x);
            el().key(format!("preview-{}", b.id))
                .role("button")
                .absolute(f32::from(b.rect.x) * k + dx.unwrap_or(0.0), f32::from(b.rect.y) * k)
                .size(f32::from(b.rect.width) * k, f32::from(b.rect.height) * k)
                // Fills are translucent: the lifted button is backed by the bar so what it passes
                // over does not show through.
                .fill(color(if dx.is_some() { over(b.fill, scene.background) } else { b.fill }))
                .border(if marked { 2.0 } else { 1.0 }, color(if marked { t.accent } else { b.border }))
                .center()
                .on_drag(move |s: &mut State, d: &mut Drag, tx: &mut Tx| on_drag(s, &id, left, d, tx))
                .child(text(b.label.clone()).font(label).color(color(b.text)))
        }))
}

/// `top` painted over `bottom`, as one opaque colour.
fn over(top: Rgba, bottom: Rgba) -> Rgba {
    let a = u16::from(top.a);
    let mix = |t: u8, b: u8| {
        let v = (u16::from(t) * a + u16::from(b) * (255 - a) + 127) / 255;
        u8::try_from(v).unwrap_or(u8::MAX)
    };
    Rgba::opaque(mix(top.r, bottom.r), mix(top.g, bottom.g), mix(top.b, bottom.b))
}

/// Down selects the button (and takes the keyboard from any field, so Alt+arrows reach
/// `on_key`); Move carries it; Up drops it at the bar x under the pointer.
fn on_drag(s: &mut State, id: &ButtonId, left: u16, d: &mut Drag, tx: &mut Tx) {
    match d.phase {
        Phase::Down => {
            s.select(Some(id.clone()));
            tx.blur();
            s.drag = Some(Dragging { id: id.clone(), dx: 0.0 });
        }
        Phase::Move => {
            if let Some(drag) = &mut s.drag {
                drag.dx = d.dx;
            }
        }
        Phase::Up => {
            s.drag = None;
            if d.cancelled() {
                return;
            }
            let x = bar_x(left, d.start_x + d.dx);
            if let Some(layer) = s.layer_mut() {
                // Only a drawn button can be dragged, so its layer fits the bar and has placements.
                let _moved = drop_button(layer, id, x, BarSize::MACBOOK_PRO, &SpacingRules::STANDARD);
            }
        }
    }
}

/// The bar x (device px) of a point `travel` preview points right of a button's left edge `left`.
fn bar_x(left: u16, travel: f32) -> u16 {
    let x = (f32::from(left) + travel / PREVIEW_SCALE).round();
    #[expect(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        reason = "clamped to the u16 range and rounded at the Rune boundary"
    )]
    let x = x.clamp(0.0, f32::from(u16::MAX)) as u16;
    x
}
