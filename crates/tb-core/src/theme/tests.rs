use super::*;

#[test]
fn the_bundled_fallback_parses() {
    BarTheme::from_omarchy_colors(FALLBACK_COLORS).unwrap();
}

#[test]
fn maps_omarchy_tokens_to_bar_roles() {
    let theme = BarTheme::from_omarchy_colors(
        "mode = \"dark\"\naccent = \"#7aa2f7\"\nbackground = \"#1a1b26\"\nforeground = \"#a9b1d6\"\nred = \"#f7768e\"",
    )
    .unwrap();
    assert_eq!(theme.background, Rgba::opaque(0x1a, 0x1b, 0x26));
    assert_eq!(theme.button_text, Rgba::opaque(0xa9, 0xb1, 0xd6));
    assert_eq!(theme.button_fill, Rgba { r: 0xa9, g: 0xb1, b: 0xd6, a: FILL_ALPHA });
    assert_eq!(theme.pressed_fill, theme.accent);
    assert_eq!(theme.pressed_text, theme.background);
    assert_eq!(theme.radius, 0, "Omarchy chrome is square");
}

#[test]
fn per_button_overrides_resolve_tokens_and_hex() {
    let theme = BarTheme::omarchy_fallback();
    assert_eq!(theme.resolve("red"), Some(Rgba::opaque(0xf7, 0x76, 0x8e)));
    assert_eq!(theme.resolve("#00ff0080"), Some(Rgba { r: 0, g: 255, b: 0, a: 0x80 }));
    assert_eq!(theme.resolve("no-such-token"), None);
}

#[test]
fn missing_required_tokens_are_reported() {
    let err = BarTheme::from_omarchy_colors("accent = \"#7aa2f7\"\nforeground = \"#a9b1d6\"").unwrap_err();
    assert!(matches!(err, ThemeError::Missing("background")), "{err}");
}

#[test]
fn malformed_hex_is_reported_with_its_token() {
    let err = BarTheme::from_omarchy_colors("background = \"#12\"").unwrap_err();
    assert!(matches!(err, ThemeError::BadColor { ref token, .. } if token == "background"), "{err}");
}
