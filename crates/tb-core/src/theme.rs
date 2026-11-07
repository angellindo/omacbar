//! Bar theme tokens, derived from an Omarchy `colors.toml` (see `docs/design/theming.md`).

use std::collections::BTreeMap;

use thiserror::Error;

/// 8-bit sRGB colour with straight alpha.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Rgba {
    pub r: u8,
    pub g: u8,
    pub b: u8,
    pub a: u8,
}

impl Rgba {
    pub const fn opaque(r: u8, g: u8, b: u8) -> Self {
        Self { r, g, b, a: 255 }
    }

    #[must_use]
    pub const fn with_alpha(self, a: u8) -> Self {
        Self { a, ..self }
    }

    /// Parses `#rrggbb` or `#rrggbbaa`.
    pub fn parse_hex(s: &str) -> Option<Self> {
        let hex = s.strip_prefix('#')?;
        let byte = |i: usize| u8::from_str_radix(hex.get(i..i + 2)?, 16).ok();
        match hex.len() {
            6 => Some(Self::opaque(byte(0)?, byte(2)?, byte(4)?)),
            8 => Some(Self { r: byte(0)?, g: byte(2)?, b: byte(4)?, a: byte(6)? }),
            _ => None,
        }
    }
}

#[derive(Debug, Error)]
pub enum ThemeError {
    #[error("colors.toml is not valid TOML: {0}")]
    Toml(#[from] toml::de::Error),
    #[error("colour token `{token}` has an invalid value `{value}` (want #rrggbb)")]
    BadColor { token: String, value: String },
    #[error("colors.toml is missing the required token `{0}`")]
    Missing(&'static str),
}

/// Everything the renderer needs to paint the bar.
#[derive(Debug, Clone, PartialEq)]
pub struct BarTheme {
    pub background: Rgba,
    pub button_fill: Rgba,
    pub button_border: Rgba,
    pub button_text: Rgba,
    pub pressed_fill: Rgba,
    pub pressed_text: Rgba,
    pub accent: Rgba,
    /// Corner radius in px. Omarchy chrome is square.
    pub radius: u16,
    pub border_width: u16,
    pub font_size: u16,
    /// Every colour token from the source theme, for per-button overrides (`accent`, `red`, ...).
    pub tokens: BTreeMap<String, Rgba>,
}

/// Alpha of the idle button fill over the background (Omarchy `[controls] normal-fill-alpha` = 0.04).
const FILL_ALPHA: u8 = 10;
/// Alpha of the idle button border (Omarchy `[controls] normal-border-alpha` = 0.4).
const BORDER_ALPHA: u8 = 102;

impl BarTheme {
    /// Builds a theme from the text of an Omarchy `colors.toml`.
    pub fn from_omarchy_colors(src: &str) -> Result<Self, ThemeError> {
        let table: BTreeMap<String, toml::Value> = toml::from_str(src)?;
        let mut tokens = BTreeMap::new();
        for (token, value) in &table {
            if let toml::Value::String(s) = value
                && s.starts_with('#')
            {
                let c = Rgba::parse_hex(s)
                    .ok_or_else(|| ThemeError::BadColor { token: token.clone(), value: s.clone() })?;
                tokens.insert(token.clone(), c);
            }
        }
        let get = |name: &'static str| tokens.get(name).copied().ok_or(ThemeError::Missing(name));
        let background = get("background")?;
        let foreground = get("foreground")?;
        let accent = get("accent")?;
        Ok(Self {
            background,
            button_fill: foreground.with_alpha(FILL_ALPHA),
            button_border: foreground.with_alpha(BORDER_ALPHA),
            button_text: foreground,
            pressed_fill: accent,
            pressed_text: background,
            accent,
            radius: 0,
            border_width: 1,
            font_size: 20,
            tokens,
        })
    }

    /// The built-in fallback (Omarchy Tokyo Night), used when no Omarchy theme is installed.
    pub fn omarchy_fallback() -> Self {
        // Parsing a constant we ship; a failure is a bug caught by `the_bundled_fallback_parses`.
        Self::from_omarchy_colors(FALLBACK_COLORS).unwrap_or_else(|_| Self::minimal())
    }

    fn minimal() -> Self {
        let fg = Rgba::opaque(0xa9, 0xb1, 0xd6);
        let bg = Rgba::opaque(0x1a, 0x1b, 0x26);
        let accent = Rgba::opaque(0x7a, 0xa2, 0xf7);
        Self {
            background: bg,
            button_fill: fg.with_alpha(FILL_ALPHA),
            button_border: fg.with_alpha(BORDER_ALPHA),
            button_text: fg,
            pressed_fill: accent,
            pressed_text: bg,
            accent,
            radius: 0,
            border_width: 1,
            font_size: 20,
            tokens: BTreeMap::new(),
        }
    }

    /// Resolves a per-button colour override: a token name or a `#hex` literal.
    pub fn resolve(&self, color: &str) -> Option<Rgba> {
        if color.starts_with('#') { Rgba::parse_hex(color) } else { self.tokens.get(color).copied() }
    }
}

const FALLBACK_COLORS: &str = include_str!("theme/tokyo-night.colors.toml");

#[cfg(test)]
mod tests;
