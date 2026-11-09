//! Bar labels are set in the system monospace font (fontconfig `monospace`; on Omarchy the
//! `JetBrainsMono Nerd Font`), measured and drawn alike. These cases hold on any machine with a
//! monospace font installed: they assert that every character takes the same advance, never a
//! particular font's metrics. Element-tree only (no GPU).
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use rune::{Env, Runtime};
use tb_core::{BarSize, BarTheme, Chord, SpacingRules, presets};
use tb_render::{BarScene, bar_view, build_scene, label_width};

/// The theme's font size: 20 px (button standard § Label).
const FONT: u16 = 20;

/// Narrow and wide glyphs: equal in a monospace font, far apart in a proportional one.
const NARROW: &str = "iiiiiiii";
const WIDE: &str = "MMMMMMMM";

fn assert_close(got: f32, want: f32, tolerance: f32, what: &str) {
    assert!((got - want).abs() <= tolerance, "{what}: got {got}, want {want} ± {tolerance}");
}

#[test]
fn narrow_and_wide_labels_of_the_same_length_measure_the_same() {
    let narrow = label_width(NARROW, FONT);
    let wide = label_width(WIDE, FONT);
    assert!(narrow > 0, "{narrow}");
    assert_eq!(narrow, wide, "`{NARROW}` and `{WIDE}` in the label font");
    assert_eq!(label_width("1l1l", FONT), label_width("WOWO", FONT), "digits and letters share the advance");
}

#[test]
fn the_bar_draws_narrow_and_wide_labels_at_the_same_measured_width() {
    let mut layout = presets::default_layout();
    let layer = layout.layer_mut(Chord::Default).unwrap();
    for b in layer.groups.iter_mut().flat_map(|g| g.buttons.iter_mut()) {
        match b.id.as_str() {
            "f1" => NARROW.clone_into(&mut b.label),
            "f2" => WIDE.clone_into(&mut b.label),
            _ => {}
        }
    }
    let theme = BarTheme::omarchy_fallback();
    assert_eq!(theme.font_size, FONT);
    let scene = build_scene(
        layout.layer(Chord::Default).unwrap(),
        &theme,
        BarSize::MACBOOK_PRO,
        &SpacingRules::STANDARD,
        None,
    )
    .unwrap();
    let mut rt: Runtime<BarScene> = Runtime::new(scene, bar_view, (2170.0, 60.0), Env::new(true, 1.0));
    rt.frame(0.0);

    let width = |id: &str| rt.presented(&format!("{id}-label")).unwrap_or_else(|| panic!("{id}-label")).0[2];
    let (narrow, wide) = (width("f1"), width("f2"));
    assert_close(narrow, wide, 0.5, "f1 `iiiiiiii` vs f2 `MMMMMMMM` drawn width");
    assert_close(narrow, label_width(NARROW, FONT) as f32, 1.0, "f1 drawn vs measured");
    assert_close(wide, label_width(WIDE, FONT) as f32, 1.0, "f2 drawn vs measured");
}
