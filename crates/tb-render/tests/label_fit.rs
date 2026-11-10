//! Labels are measured with Rune's text system at the theme's font size, the way the bar draws
//! them; a label wider than its label room is both a warning and cut to the room with an ellipsis.
//! Element-tree assertions run everywhere; pixel assertions need a GPU adapter (lavapipe is fine).
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use rune::{Env, Runtime};
use tb_core::config::{Config, ConfigWarning, warnings};
use tb_core::{BarSize, BarTheme, Chord, Rect, SpacingRules};
use tb_render::{BarScene, Frame, bar_view, build_scene, label_width, render_frame};

const LONG_LABEL: &str = "F5 with a label far too long to fit inside its button on the bar";

/// The theme's font size: 20 px (button standard § Label).
const FONT: u16 = 20;

fn config_with_f5_label(label: Option<&str>) -> Config {
    let mut config = Config::default();
    if let Some(label) = label {
        let layer = config.layout.layer_mut(Chord::Default).unwrap();
        let f5 = layer
            .groups
            .iter_mut()
            .flat_map(|g| g.buttons.iter_mut())
            .find(|b| b.id.as_str() == "f5")
            .unwrap();
        label.clone_into(&mut f5.label);
    }
    config
}

fn scene_of(config: &Config) -> BarScene {
    let theme = BarTheme::omarchy_fallback();
    assert_eq!(theme.font_size, FONT);
    build_scene(
        config.layout.layer(Chord::Default).unwrap(),
        &theme,
        BarSize::MACBOOK_PRO,
        &SpacingRules::STANDARD,
        None,
    )
    .unwrap()
}

fn runtime(scene: &BarScene) -> Runtime<BarScene> {
    let mut rt = Runtime::new(scene.clone(), bar_view, (2170.0, 60.0), Env::new(true, 1.0));
    rt.frame(0.0);
    rt
}

fn rect_of(scene: &BarScene, id: &str) -> Rect {
    scene.buttons.iter().find(|b| b.id.as_str() == id).unwrap().rect
}

/// The presented `[x, y, width, height]` of a button's label element.
fn label_rect(rt: &Runtime<BarScene>, id: &str) -> [f32; 4] {
    rt.presented(&format!("{id}-label")).unwrap_or_else(|| panic!("{id}-label not presented")).0
}

fn assert_close(got: f32, want: f32, tolerance: f32, what: &str) {
    assert!((got - want).abs() <= tolerance, "{what}: got {got}, want {want} ± {tolerance}");
}

fn same_colour(a: [u8; 4], b: [u8; 4]) -> bool {
    a.iter().zip(b).take(3).all(|(x, y)| x.abs_diff(y) <= 2)
}

/// Rows of a button's label band: the 20 px label centred in the 44 px button (y 8..52).
const LABEL_ROWS: std::ops::RangeInclusive<u32> = 16..=44;

#[test]
fn every_default_label_is_presented_at_its_measured_width() {
    let config = Config::default();
    let scene = scene_of(&config);
    let rt = runtime(&scene);
    for b in &scene.buttons {
        let [_, _, width, _] = label_rect(&rt, b.id.as_str());
        let measured = label_width(&b.label, FONT);
        assert!(measured > 0, "{}: {measured}", b.id);
        assert_close(width, measured as f32, 1.0, b.id.as_str());
    }
}

#[test]
fn a_label_is_measured_on_one_line_as_the_bar_draws_it() {
    assert_eq!(label_width("F5\nF5", FONT), label_width("F5 F5", FONT));
    assert_eq!(label_width("F5\tF5", FONT), label_width("F5 F5", FONT));
    assert!(label_width("F5 F5", FONT) > label_width("F5", FONT));
}

#[test]
fn a_label_is_measured_at_the_given_font_size() {
    assert!(label_width(LONG_LABEL, FONT) > label_width(LONG_LABEL, 10));
    assert!(label_width("F5", FONT) > label_width("F5", 10));
}

#[test]
fn a_long_label_measured_by_rune_is_one_label_too_wide_warning() {
    let config = config_with_f5_label(Some(LONG_LABEL));
    let found = warnings(&config, BarSize::MACBOOK_PRO, &SpacingRules::STANDARD, |l| label_width(l, FONT));
    let measured = label_width(LONG_LABEL, FONT);
    assert!(measured > 119, "{measured}");
    // F-keys on the default layer are 143 px wide (button standard § Width): room 143 - 2 x 12.
    assert_eq!(
        found,
        vec![ConfigWarning::LabelTooWide {
            chord: Chord::Default,
            id: tb_core::ButtonId::new("f5"),
            width: measured,
            room: 119
        }]
    );
}

#[test]
fn the_default_labels_measured_by_rune_have_no_warning() {
    let found =
        warnings(&Config::default(), BarSize::MACBOOK_PRO, &SpacingRules::STANDARD, |l| label_width(l, FONT));
    assert_eq!(found, vec![]);
}

#[test]
fn a_long_label_is_cut_to_its_room_and_no_other_label_is_cut() {
    let config = config_with_f5_label(Some(LONG_LABEL));
    let scene = scene_of(&config);
    let rt = runtime(&scene);
    let f5 = rect_of(&scene, "f5");
    let [x, _, width, _] = label_rect(&rt, "f5");
    assert_close(width, 119.0, 0.5, "f5 label width (its room)");
    assert_close(x, f32::from(f5.x) + 12.0, 0.5, "f5 label start (after the label padding)");
    assert!(width < label_width(LONG_LABEL, FONT) as f32, "the label is cut");
    for b in scene.buttons.iter().filter(|b| b.id.as_str() != "f5") {
        let [_, _, width, _] = label_rect(&rt, b.id.as_str());
        assert_close(width, label_width(&b.label, FONT) as f32, 1.0, b.id.as_str());
    }
}

fn differs_somewhere(a: &Frame, b: &Frame, xs: std::ops::Range<u32>) -> bool {
    xs.flat_map(|x| LABEL_ROWS.map(move |y| (x, y))).any(|(x, y)| !same_colour(a.pixel(x, y), b.pixel(x, y)))
}

fn assert_same_in(a: &Frame, b: &Frame, xs: std::ops::Range<u32>, what: &str) {
    for x in xs {
        for y in LABEL_ROWS {
            assert!(
                same_colour(a.pixel(x, y), b.pixel(x, y)),
                "{what}: pixel ({x}, {y}) is {:?}, default layer has {:?}",
                a.pixel(x, y),
                b.pixel(x, y)
            );
        }
    }
}

#[test]
fn a_long_label_inks_its_room_to_the_end_and_leaves_the_padding_and_its_neighbours_untouched() {
    let long = scene_of(&config_with_f5_label(Some(LONG_LABEL)));
    let short = scene_of(&config_with_f5_label(None));
    let long_frame = render_frame(long.clone());
    let short_frame = render_frame(short);
    assert_eq!((long_frame.width, long_frame.height), (2170, 60));
    if std::env::var_os("TB_WRITE_SNAPSHOTS").is_some() {
        long_frame.write_png(std::path::Path::new("../../out/long-label.png")).unwrap();
    }

    let f5 = rect_of(&long, "f5");
    let (left, right) = (u32::from(f5.x), u32::from(f5.right()));
    // The short "F5" is centred, so near both ends of the room the default frame shows bare fill.
    // At the new 143 px width, whole mono glyphs can leave 24 px after their last inked
    // column. Include one rounding pixel around the final two cells, not only bare fill.
    assert!(
        differs_somewhere(&long_frame, &short_frame, right - 12 - 25..right - 12),
        "the long label reaches the last two mono cells plus one rounding px (the ellipsis)"
    );
    assert!(
        differs_somewhere(&long_frame, &short_frame, left + 12..left + 12 + 24),
        "the long label starts at the beginning of its room"
    );
    assert_same_in(&long_frame, &short_frame, right - 11..right - 1, "f5 right label padding");
    assert_same_in(&long_frame, &short_frame, left + 1..left + 11, "f5 left label padding");
    for id in ["f4", "f6"] {
        let r = rect_of(&long, id);
        assert_same_in(&long_frame, &short_frame, u32::from(r.x)..u32::from(r.right()), id);
    }
}
