//! The Rune view of a [`BarScene`]. One point = one device pixel (render at scale 1).

use rune::tok::TextStyle;
use rune::{Color, Cx, El, el, text};
use tb_core::Rgba;

use crate::scene::BarScene;

/// Converts a theme colour (8-bit sRGB) to Rune's linear colour.
pub fn color(c: Rgba) -> Color {
    rune::color::hex(&format!("#{:02x}{:02x}{:02x}{:02x}", c.r, c.g, c.b, c.a)).unwrap_or(Color::TRANSPARENT)
}

pub fn bar_view(s: &BarScene, _: &mut Cx<BarScene>) -> El {
    let label_style = TextStyle::new(f32::from(s.font_size), 600.0).mono();
    let buttons = s.buttons.iter().map(|b| {
        el().key(b.id.as_str())
            .role("button")
            .label(b.label.clone())
            .absolute(f32::from(b.rect.x), f32::from(b.rect.y))
            .size(f32::from(b.rect.width), f32::from(b.rect.height))
            .radius(f32::from(s.radius))
            // Safety net under the one-line, ellipsized label: nothing a button draws leaves its
            // placement, which is the damage the daemon presents for it.
            .clip_rounded(f32::from(s.radius))
            .fill(color(b.fill))
            .border(f32::from(s.border_width), color(b.border))
            .px(f32::from(s.label_padding))
            .center()
            // One line (see `build_scene`), ellipsized to the padded width.
            .child(
                text(b.label.clone())
                    .key(format!("{}-label", b.id.as_str()))
                    .font(label_style)
                    .color(color(b.text))
                    .truncate(),
            )
    });
    el().key("bar")
        .size(f32::from(s.size.width), f32::from(s.size.height))
        .fill(color(s.background))
        .children(buttons)
}
