//! The bar preview: the shown layer exactly as the renderer lays it out, scaled down. Pressing a
//! button selects it; dragging it along the row and releasing drops it before or after the button
//! under the pointer (`tb_core::reorder::drop_button`).

use rune::{Drag, El, Phase, Tx, el, text};
use tb_core::reorder::drop_button;
use tb_core::{BarSize, ButtonId, Rgba, SpacingRules};
use tb_render::{BarScene, SceneButton, bar_button, build_scene, view::color};

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
        return el().key("preview").size(1085.0, 30.0).fill(color(t.background)).child(
            text(if s.layer().is_none_or(|l| l.buttons().next().is_none()) {
                "empty layer · add a button or group"
            } else {
                "this layer does not fit the bar"
            })
            .color(color(t.accent)),
        );
    };
    let k = PREVIEW_SCALE;
    let dragged = |b: &&SceneButton| s.drag.as_ref().is_some_and(|d| d.id == b.id);
    // The dragged button comes last so it is drawn over the buttons it passes.
    let (lifted, resting): (Vec<_>, Vec<_>) = scene.buttons.iter().partition(dragged);
    el().key("preview")
        .size(f32::from(scene.size.width) * k, f32::from(scene.size.height) * k)
        .fill(color(scene.background))
        .children(resting.into_iter().chain(lifted).map(|b| {
            let dx = s.drag.as_ref().filter(|d| d.id == b.id).map(|d| d.dx);
            let (id, left) = (b.id.clone(), b.rect.x);
            // Drawn by the bar's own button view at preview scale: radius, clip, border, label
            // padding and the ellipsized label are the bar's.
            let mut button = bar_button(&scene, b, k)
                .key(format!("preview-{}", b.id))
                .on_drag(move |s: &mut State, d: &mut Drag, tx: &mut Tx| on_drag(s, &id, left, d, tx));
            if let Some(dx) = dx {
                // Fills are translucent: the lifted button is backed by the bar so what it passes
                // over does not show through.
                button = button
                    .absolute(f32::from(b.rect.x) * k + dx, f32::from(b.rect.y) * k)
                    .fill(color(over(b.fill, scene.background)));
            }
            button
        }))
        .children(scene.buttons.iter().flat_map(|button| decorations(s, button)))
}

/// These marks never enter the shared scene or physical pressed state.
fn decorations(s: &State, button: &SceneButton) -> Vec<El> {
    let left = f32::from(button.rect.x) * PREVIEW_SCALE;
    let top = f32::from(button.rect.y) * PREVIEW_SCALE;
    let width = f32::from(button.rect.width) * PREVIEW_SCALE;
    let height = f32::from(button.rect.height) * PREVIEW_SCALE;
    let dx = s.drag.as_ref().filter(|d| d.id == button.id).map_or(0.0, |d| d.dx);
    let mut marks = Vec::new();
    if s.selected.as_ref() == Some(&button.id) {
        marks.push(
            el().key(format!("preview-selection-{}", button.id))
                .absolute(left + dx, top)
                .size(width, height)
                .border(2.0, color(s.theme.accent)),
        );
    }
    if s.button_invalid(&button.id) {
        marks.push(
            el().key(format!("preview-error-{}", button.id))
                .absolute(left + dx + 3.0, top + 3.0)
                .size(width - 6.0, height - 6.0)
                .border(1.0, color(s.error_color())),
        );
    }
    marks
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
