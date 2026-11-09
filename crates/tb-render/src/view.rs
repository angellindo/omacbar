//! The Rune view of a [`BarScene`]. One point = one device pixel (render at scale 1).

use rune::tok::TextStyle;
use rune::{Color, Cx, El, Family, el, text};
use tb_core::Rgba;

use crate::scene::{BarScene, one_line};

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

pub fn bar_view(s: &BarScene, _: &mut Cx<BarScene>) -> El {
    let label_style = label_style(f32::from(s.font_size));
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
