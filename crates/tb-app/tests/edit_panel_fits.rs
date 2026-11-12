//! Functional: the edit panel, the issue list and the status stay usable inside the configurator
//! window whatever the user types or adds. A long label is shown shortened in the button list and
//! never pushes the panel's fields out of the window; a layer with many buttons never pushes an
//! issue or the status out of reach (an element below the window can be scrolled into view).
//! Driven through the real view with `rune::Runtime`; each test edits its own config file under
//! cargo's temp dir.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod support {
    pub mod ui;
}

use rune::Runtime;
use support::ui::*;
use tb_app::State;
use tb_core::{ButtonId, Chord};

/// Rounding slack in points.
const EPS: f32 = 0.5;

fn open(test: &str) -> Runtime<State> {
    configurator(fresh_dir("edit_panel_fits", test).join("config.toml"))
}

fn element(rt: &Runtime<State>, key: &str) -> rune::ElementInfo {
    rt.elements()
        .into_iter()
        .find(|e| e.key.as_deref() == Some(key) && !e.exiting)
        .unwrap_or_else(|| panic!("no element keyed {key}"))
}

/// Whether `r` (`[x, y, w, h]`) lies wholly inside the window.
fn inside_window(r: [f32; 4]) -> bool {
    let (w, h) = tb_app::WINDOW;
    r[0] >= -EPS && r[1] >= -EPS && r[0] + r[2] <= w + EPS && r[1] + r[3] <= h + EPS
}

fn intersect(a: [f32; 4], b: [f32; 4]) -> bool {
    a[0] + EPS < b[0] + b[2]
        && b[0] + EPS < a[0] + a[2]
        && a[1] + EPS < b[1] + b[3]
        && b[1] + EPS < a[1] + a[3]
}

/// Whether the element keyed `key` is drawn whole inside the window: its rect is inside the window
/// and no ancestor clips any of it.
fn fully_shown(rt: &Runtime<State>, key: &str) -> bool {
    let e = element(rt, key);
    let unclipped = (0..4).all(|i| (e.visible[i] - e.rect[i]).abs() <= EPS);
    inside_window(e.rect) && unclipped && e.rect[2] > 0.0
}

/// Wheels the scroll view under the window point nearest the element keyed `key` toward it until
/// the element is drawn whole inside the window; whether it got there.
fn scroll_into_view(rt: &mut Runtime<State>, key: &str) -> bool {
    let (width, height) = tb_app::WINDOW;
    for _ in 0..80 {
        if fully_shown(rt, key) {
            return true;
        }
        let target = element(rt, key);
        let mut bounds = [0.0_f32, 0.0, width, height];
        for owner in rt.elements().iter().filter(|owner| {
            owner.key.as_deref().is_some_and(|key| key.ends_with("scroll"))
                && target.path.starts_with(&format!("{}/", owner.path))
        }) {
            let right = (bounds[0] + bounds[2]).min(owner.visible[0] + owner.visible[2]);
            let bottom = (bounds[1] + bounds[3]).min(owner.visible[1] + owner.visible[3]);
            bounds[0] = bounds[0].max(owner.visible[0]);
            bounds[1] = bounds[1].max(owner.visible[1]);
            bounds[2] = right - bounds[0];
            bounds[3] = bottom - bounds[1];
        }
        let rect = target.rect;
        let at_x = (rect[0] + rect[2] / 2.0).clamp(bounds[0] + 1.0, bounds[0] + bounds[2] - 1.0);
        let at_y = (rect[1] + rect[3] / 2.0).clamp(bounds[1] + 1.0, bounds[1] + bounds[3] - 1.0);
        // Each B column owns scrolling above the footer, so its viewport bounds the reveal.
        let dy = if rect[1] + rect[3] > bounds[1] + bounds[3] { -60.0 } else { 60.0 };
        rt.wheel(at_x, at_y, dy);
        settle(rt);
    }
    fully_shown(rt, key)
}

const PANEL_FIELDS: [&str; 10] = [
    "panel",
    "panel-id",
    "field-label",
    "field-width",
    "field-fill",
    "field-text",
    "field-border",
    "action-none",
    "action-key",
    "action-command",
];

fn long_label() -> String {
    "a".repeat(200)
}

/// The configurator with F1 selected and labelled with 200 characters.
fn with_long_label(test: &str) -> Runtime<State> {
    let mut rt = open(test);
    tap(&mut rt, "preview-f1");
    type_into(&mut rt, "field-label", &long_label());
    let f1 = default_layer(&rt).buttons().find(|b| b.id.as_str() == "f1").unwrap().label.clone();
    assert_eq!(f1, long_label(), "the whole label reaches the config");
    rt
}

#[test]
fn a_long_label_keeps_every_edit_panel_field_inside_the_window() {
    let mut rt = with_long_label("long_label_fields");
    for key in PANEL_FIELDS.into_iter().chain(["field-action"]) {
        let r = element(&rt, key).rect;
        assert!(inside_window(r), "{key} lies inside the {:?} window: {r:?}", tb_app::WINDOW);
    }
    for key in ["no-issues", "status", "save"] {
        assert!(
            scroll_into_view(&mut rt, key),
            "{key} can be brought into view: {:?}",
            element(&rt, key).rect
        );
    }
}

#[test]
fn a_long_label_is_shown_shortened_in_the_button_list() {
    let rt = with_long_label("long_label_row");
    let row = element(&rt, "row-f1");
    assert!(inside_window(row.rect), "row-f1 lies inside the window: {:?}", row.rect);
    let panel = element(&rt, "panel").rect;
    assert!(!intersect(row.rect, panel), "row-f1 {:?} does not run into the edit panel {panel:?}", row.rect);
    let inside = format!("{}/", row.path);
    for d in rt.elements().into_iter().filter(|d| d.path.starts_with(&inside) && !d.exiting) {
        assert!(inside_window(d.visible), "what row-f1 shows stays inside the window: {:?}", d.visible);
        assert!(!intersect(d.visible, panel), "row-f1 text {:?} does not run into the edit panel", d.visible);
    }
}

#[test]
fn with_twenty_five_buttons_every_issue_the_status_and_the_panel_can_be_brought_into_view() {
    let mut rt = open("twenty_five_buttons");
    for _ in 0..12 {
        assert!(scroll_into_view(&mut rt, "add-button"), "the growing workspace add control stays reachable");
        tap(&mut rt, "add-button");
    }
    assert_eq!(default_layer(&rt).buttons().count(), 25);
    rt.update(|s: &mut State, _| {
        let layer = s.config.layout.layer_mut(Chord::Default).unwrap();
        for b in layer.groups.iter_mut().flat_map(|g| g.buttons.iter_mut()) {
            if ["button-1", "button-2", "button-3"].contains(&b.id.as_str()) {
                b.style.fill = Some("nope".to_owned());
            }
        }
    });
    settle(&mut rt);
    let issues: Vec<String> =
        (0..64).map(|n| format!("issue-{n}")).take_while(|k| text_of(&rt, k).is_some()).collect();
    assert_eq!(issues.len(), 3, "three unknown colours are listed: {issues:?}");
    assert_eq!(rt.state.selected, Some(ButtonId::new("button-12")), "the last added button is selected");
    for key in issues.iter().map(String::as_str).chain(["status", "save", "field-label", "field-fill"]) {
        assert!(
            scroll_into_view(&mut rt, key),
            "{key} can be brought into view: {:?}",
            element(&rt, key).rect
        );
    }
}
