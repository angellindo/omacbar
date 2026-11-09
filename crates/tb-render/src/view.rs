//! The Rune view of a [`BarScene`]. One point = one device pixel (render at scale 1).

use rune::tok::TextStyle;
use rune::{Color, Cx, El, Family, el, text};
use tb_core::Rgba;

use crate::scene::{BarScene, SceneButton, one_line};

/// Converts a theme colour (8-bit sRGB) to Rune's linear colour.
pub fn color(c: Rgba) -> Color {
    rune::color::hex(&format!("#{:02x}{:02x}{:02x}{:02x}", c.r, c.g, c.b, c.a)).unwrap_or(Color::TRANSPARENT)
}

/// The style every bar label is drawn and measured in, so the two cannot drift: `size` px,
/// weight 600, Rune's `Family::SystemMono` = fontconfig `monospace` (Omarchy: `JetBrainsMono Nerd
/// Font`). Without one Rune tries `DejaVu Sans Mono`, `Liberation Mono`, `Noto Sans Mono`,
/// `Ubuntu Mono`, then its mono slot (`docs/design/theming.md` § Label font).
pub fn label_style(size: f32) -> TextStyle {
    TextStyle::new(size, 600.0).family(Family::SystemMono)
}

/// Width in device px of `label` as [`bar_view`] draws it at `font_size`: one line, Rune's text
/// system, rounded up.
pub fn label_width(label: &str, font_size: u16) -> u32 {
    let (width, _) = rune::text::measure(&one_line(label), &[], label_style(f32::from(font_size)), None);
    #[expect(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        reason = "clamped to the bar's pixel range and already rounded up by Rune"
    )]
    let width = width.clamp(0.0, f32::from(u16::MAX)) as u32;
    width
}

/// One bar button as the bar draws it, every length times `scale` (1: the bar itself, device px).
/// Its fill and border are drawn on a rounded rect clipped to the placement, and the label is one
/// line inside the side label padding, ellipsized to the room left. The caller keys it.
pub fn bar_button(s: &BarScene, b: &SceneButton, scale: f32) -> El {
    let px = |v: u16| f32::from(v) * scale;
    el().role("button")
        .label(b.label.clone())
        .absolute(px(b.rect.x), px(b.rect.y))
        .size(px(b.rect.width), px(b.rect.height))
        .radius(px(s.radius))
        // Safety net under the one-line, ellipsized label: nothing a button draws leaves its
        // placement, which is the damage the daemon presents for it.
        .clip_rounded(px(s.radius))
        .fill(color(b.fill))
        .border(px(s.border_width), color(b.border))
        .px(px(s.label_padding))
        .center()
        // One line (see `build_scene`), ellipsized to the padded width.
        .child(
            text(b.label.clone())
                .key(format!("{}-label", b.id.as_str()))
                .font(label_style(px(s.font_size)))
                .color(color(b.text))
                .truncate(),
        )
}

pub fn bar_view(s: &BarScene, _: &mut Cx<BarScene>) -> El {
    el().key("bar")
        .size(f32::from(s.size.width), f32::from(s.size.height))
        .fill(color(s.background))
        .children(s.buttons.iter().map(|b| bar_button(s, b, 1.0).key(b.id.as_str())))
}
