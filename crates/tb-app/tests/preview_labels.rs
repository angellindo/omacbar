//! Functional: the configurator's preview draws a label the way the bar does: one line inside the
//! button's label room (its width minus the label padding on each side, at preview scale), cut
//! short when it is wider, never over a neighbouring button. Driven through the real view with
//! `rune::Runtime`; each test edits its own config file under cargo's temp dir.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod support {
    pub mod ui;
}

use rune::Runtime;
use support::ui::*;
use tb_app::State;
use tb_core::{BarSize, SpacingRules};
use tb_render::label_width;

/// Rounding slack in points.
const EPS: f32 = 0.5;

fn long_label() -> String {
    "a".repeat(200)
}

/// The configurator with F1 labelled with 200 characters through the edit panel.
fn with_long_f1_label(test: &str) -> Runtime<State> {
    let mut rt = configurator(fresh_dir("preview_labels", test).join("config.toml"));
    tap(&mut rt, "preview-f1");
    type_into(&mut rt, "field-label", &long_label());
    let f1 = default_layer(&rt).buttons().find(|b| b.id.as_str() == "f1").unwrap().label.clone();
    assert_eq!(f1, long_label(), "the whole label reaches the config");
    rt
}

/// Preview points per bar pixel.
fn scale(rt: &Runtime<State>) -> f32 {
    rect(rt, "preview")[2] / f32::from(BarSize::MACBOOK_PRO.width)
}

/// The text element drawing `label` inside the element keyed `key`.
fn label_in(rt: &Runtime<State>, key: &str, label: &str) -> rune::ElementInfo {
    let elements = rt.elements();
    let button = elements.iter().find(|e| e.key.as_deref() == Some(key) && !e.exiting).unwrap();
    let inside = format!("{}/", button.path);
    elements
        .into_iter()
        .find(|e| e.path.starts_with(&inside) && !e.exiting && e.text.as_deref() == Some(label))
        .unwrap_or_else(|| panic!("{key} draws no text reading the label"))
}

#[test]
fn a_long_label_stays_inside_its_preview_button_within_the_label_padding() {
    let rt = with_long_f1_label("inside_label_room");
    let pad = f32::from(SpacingRules::STANDARD.label_padding) * scale(&rt);
    let button = rect(&rt, "preview-f1");
    let label = label_in(&rt, "preview-f1", &long_label());
    for (what, r) in [("rect", label.rect), ("visible", label.visible)] {
        assert!(
            r[0] >= button[0] + pad - EPS,
            "label {what} {r:?} starts inside F1's label room {button:?} (pad {pad})"
        );
        assert!(
            r[0] + r[2] <= button[0] + button[2] - pad + EPS,
            "label {what} {r:?} ends inside F1's label room {button:?} (pad {pad})"
        );
    }
}

#[test]
fn a_long_label_does_not_run_over_the_next_button() {
    let rt = with_long_f1_label("not_over_f2");
    let f2 = rect(&rt, "preview-f2");
    let label = label_in(&rt, "preview-f1", &long_label());
    for (what, r) in [("rect", label.rect), ("visible", label.visible)] {
        assert!(r[0] + r[2] <= f2[0] + EPS, "F1's label {what} {r:?} stops before F2 {f2:?}");
    }
}

#[test]
fn a_long_label_is_cut_shorter_than_its_full_width() {
    let rt = with_long_f1_label("cut_short");
    let full = label_width(&long_label(), rt.state.theme.font_size) as f32 * scale(&rt);
    let label = label_in(&rt, "preview-f1", &long_label());
    assert!(label.visible[2] > 0.0, "some of the label is drawn");
    assert!(label.rect[2] < full / 2.0, "the label is cut short: drawn {} of {full} pt", label.rect[2]);
}

#[test]
fn a_short_label_is_drawn_whole_inside_its_button() {
    let mut rt = configurator(fresh_dir("preview_labels", "short_label").join("config.toml"));
    tap(&mut rt, "preview-f1");
    type_into(&mut rt, "field-label", "go");
    let full = label_width("go", rt.state.theme.font_size) as f32 * scale(&rt);
    let button = rect(&rt, "preview-f1");
    let label = label_in(&rt, "preview-f1", "go");
    assert!(
        label.rect[2] >= full - 1.0,
        "a label that fits is not cut: drawn {} of {full} pt",
        label.rect[2]
    );
    let centre = label.rect[0] + label.rect[2] / 2.0;
    assert!((centre - (button[0] + button[2] / 2.0)).abs() <= 1.0, "the label stays centred in its button");
}
