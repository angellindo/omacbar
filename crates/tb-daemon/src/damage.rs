//! Damage: which rects of the bar a new scene changed against the scene last presented.

use tb_core::Rect;
use tb_render::{BarScene, SceneButton};

/// The rects to present when `new` replaces `old`. Empty when nothing changed.
///
/// Same bar look and the same button ids in the same rects: the rects of the buttons whose look
/// changed. Anything else (nothing presented yet, background, radius, border, font or label padding changed,
/// buttons moved, added or removed): the whole bar, since pixels between buttons may change too.
pub(crate) fn damage(old: Option<&BarScene>, new: &BarScene) -> Vec<Rect> {
    let whole = vec![Rect { x: 0, y: 0, width: new.size.width, height: new.size.height }];
    let Some(old) = old else { return whole };
    if old == new {
        return Vec::new();
    }
    let same_frame = old.size == new.size
        && old.background == new.background
        && old.radius == new.radius
        && old.border_width == new.border_width
        && old.font_size == new.font_size
        && old.label_padding == new.label_padding
        && old.buttons.len() == new.buttons.len()
        && old.buttons.iter().zip(&new.buttons).all(|(a, b)| a.id == b.id && a.rect == b.rect);
    if !same_frame {
        return whole;
    }
    old.buttons.iter().zip(&new.buttons).filter(|(a, b)| !same_look(a, b)).map(|(_, b)| b.rect).collect()
}

fn same_look(a: &SceneButton, b: &SceneButton) -> bool {
    a.label == b.label && a.fill == b.fill && a.text == b.text && a.border == b.border
}
