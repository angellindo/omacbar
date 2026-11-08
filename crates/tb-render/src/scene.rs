//! What to draw: placed buttons with every colour already resolved against the theme.

use tb_core::{BarSize, BarTheme, ButtonId, Layer, LayoutError, Rect, Rgba, SpacingRules, place};

#[derive(Debug, Clone, PartialEq)]
pub struct SceneButton {
    pub id: ButtonId,
    pub label: String,
    pub rect: Rect,
    pub fill: Rgba,
    pub text: Rgba,
    pub border: Rgba,
}

/// Everything the view needs. The view reads nothing else.
#[derive(Debug, Clone, PartialEq)]
pub struct BarScene {
    pub size: BarSize,
    pub background: Rgba,
    pub radius: u16,
    pub border_width: u16,
    pub font_size: u16,
    /// Room kept on each side of a label; a longer label is ellipsized (button standard § Label).
    pub label_padding: u16,
    pub buttons: Vec<SceneButton>,
}

/// Lays out `layer` and resolves colours. `pressed` draws that button in its pressed state.
pub fn build_scene(
    layer: &Layer,
    theme: &BarTheme,
    size: BarSize,
    rules: &SpacingRules,
    pressed: Option<&ButtonId>,
) -> Result<BarScene, LayoutError> {
    let placed = place(layer, size, rules)?;
    let buttons = layer
        .buttons()
        .zip(placed)
        .map(|(button, p)| {
            let is_pressed = pressed == Some(&button.id);
            let pick = |over: &Option<String>, idle: Rgba, down: Rgba| {
                if is_pressed { down } else { over.as_deref().and_then(|c| theme.resolve(c)).unwrap_or(idle) }
            };
            SceneButton {
                id: button.id.clone(),
                label: button.label.clone(),
                rect: p.rect,
                fill: pick(&button.style.fill, theme.button_fill, theme.pressed_fill),
                text: pick(&button.style.text, theme.button_text, theme.pressed_text),
                border: pick(&button.style.border, theme.button_border, theme.pressed_fill),
            }
        })
        .collect();
    Ok(BarScene {
        size,
        background: theme.background,
        radius: theme.radius,
        border_width: theme.border_width,
        font_size: theme.font_size,
        label_padding: rules.label_padding,
        buttons,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use tb_core::{Chord, presets};

    fn scene(pressed: Option<&ButtonId>) -> BarScene {
        let layout = presets::default_layout();
        let theme = BarTheme::omarchy_fallback();
        build_scene(
            layout.layer(Chord::FnCtrl).unwrap(),
            &theme,
            BarSize::MACBOOK_PRO,
            &SpacingRules::STANDARD,
            pressed,
        )
        .unwrap()
    }

    #[test]
    fn idle_buttons_use_theme_roles_unless_overridden() {
        let theme = BarTheme::omarchy_fallback();
        let s = scene(None);
        let shot = s.buttons.iter().find(|b| b.id.as_str() == "screenshot").unwrap();
        assert_eq!((shot.fill, shot.text), (theme.button_fill, theme.button_text));
        let lock = s.buttons.iter().find(|b| b.id.as_str() == "lock").unwrap();
        assert_eq!(lock.fill, theme.resolve("red").unwrap(), "lock overrides its fill with the `red` token");
    }

    #[test]
    fn the_pressed_button_uses_the_pressed_roles_even_when_overridden() {
        let theme = BarTheme::omarchy_fallback();
        let lock = ButtonId::new("lock");
        let s = scene(Some(&lock));
        let b = s.buttons.iter().find(|b| b.id == lock).unwrap();
        assert_eq!((b.fill, b.text), (theme.pressed_fill, theme.pressed_text));
    }
}
