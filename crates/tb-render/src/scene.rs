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
                label: one_line(&button.label),
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

/// The label as the bar shows it: one line (button standard § Label), so every control character
/// (line breaks, tabs) becomes a space. A label still too wide is ellipsized by the view.
fn one_line(label: &str) -> String {
    label.chars().map(|c| if c.is_control() { ' ' } else { c }).collect()
}

#[cfg(test)]
mod tests;
