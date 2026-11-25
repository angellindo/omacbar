//! How the recovery contract reads the configurator at a window size, with real wheel input only.
//!
//! A control is scrolled into view whole and must fit its viewport: each wheel step is at most a
//! notch and never larger than what is still hidden, so a control that fits is never stepped over.
//! A passage of text is read top to bottom: the whole text is laid out inside its box (nothing
//! truncated, faded out or wider than the box) and every band of the box, edge to edge, is seen in
//! some view. A passage that fits is seen in one view; a taller one in overlapping views from its
//! first line to its last. Only the recovery contract uses this; the shared `configurator.rs` driver
//! keeps its own `reveal`.
#![allow(dead_code)]

use rune::{ElementInfo, Runtime};

/// The largest wheel step, in points: the shared driver's notch.
const NOTCH: f32 = 60.0;
/// The largest sideways wheel step, in points.
const SIDE_NOTCH: f32 = 80.0;
/// Layout rounding tolerated between a rect and what is seen of it, in points.
const SLACK: f32 = 1.0;

/// What reading a passage took.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Reading {
    /// Views it was read in: 1 when it fits its viewport.
    pub views: usize,
    /// Height of its box, in points.
    pub height: f32,
    /// Height of the viewport it was read through, in points.
    pub viewport: f32,
}

/// Lets every animation and scroll glide land.
pub fn rest<S>(rt: &mut Runtime<S>) {
    for _ in 0..30 {
        rt.frame(1.0 / 60.0);
    }
    rt.settle_all();
    rt.frame(0.0);
}

/// The one live element keyed `key`.
pub fn live<S>(rt: &Runtime<S>, key: &str) -> ElementInfo {
    let mut found = rt.elements().into_iter().filter(|e| e.key.as_deref() == Some(key) && !e.exiting);
    let e = found.next().unwrap_or_else(|| panic!("missing public Runtime key {key}"));
    assert!(found.next().is_none(), "ambiguous Runtime key {key}");
    e
}

/// Its box lies in the window and nothing of it is clipped.
pub fn whole(e: &ElementInfo, size: (f32, f32)) -> bool {
    e.rect[2] > 0.0
        && e.rect[3] > 0.0
        && e.rect[0] >= -SLACK
        && e.rect[1] >= -SLACK
        && e.rect[0] + e.rect[2] <= size.0 + SLACK
        && e.rect[1] + e.rect[3] <= size.1 + SLACK
        && (0..4).all(|i| (e.rect[i] - e.visible[i]).abs() <= SLACK)
}

/// The part of the window through which `e` can be scrolled into sight: the window cut by the
/// visible part of every scroll viewport that holds it.
fn viewport<S>(rt: &Runtime<S>, e: &ElementInfo, size: (f32, f32)) -> [f32; 4] {
    let mut clip = [0.0, 0.0, size.0, size.1];
    for owner in rt.elements().iter().filter(|o| {
        (o.scroll_axes[0] || o.scroll_axes[1]) && !o.exiting && e.path.starts_with(&format!("{}/", o.path))
    }) {
        let right = (clip[0] + clip[2]).min(owner.visible[0] + owner.visible[2]);
        let bottom = (clip[1] + clip[3]).min(owner.visible[1] + owner.visible[3]);
        clip[0] = clip[0].max(owner.visible[0]);
        clip[1] = clip[1].max(owner.visible[1]);
        clip[2] = (right - clip[0]).max(0.0);
        clip[3] = (bottom - clip[1]).max(0.0);
    }
    clip
}

/// The wheel step that moves a span `[start, start + len)` toward `[lo, lo + room)`: at most
/// `notch`, and never more than the part still outside, so a span that fits is never overshot.
fn step(start: f32, len: f32, lo: f32, room: f32, notch: f32) -> f32 {
    if start < lo - SLACK {
        (lo - start).min(notch)
    } else if start + len > lo + room + SLACK {
        -(start + len - lo - room).min(notch)
    } else {
        0.0
    }
}

/// One real wheel event at a point of the viewport over `e`, then rest. Panics, naming `what`,
/// when it moves nothing: the rest of `e` cannot be brought into sight.
fn turn<S>(rt: &mut Runtime<S>, e: &ElementInfo, clip: [f32; 4], dx: f32, dy: f32, what: &str) {
    let x = (e.rect[0] + e.rect[2] / 2.0).clamp(clip[0] + 1.0, clip[0] + clip[2] - 1.0);
    let y = (e.rect[1] + e.rect[3] / 2.0).clamp(clip[1] + 1.0, clip[1] + clip[3] - 1.0);
    if dx == 0.0 {
        rt.wheel(x, y, dy);
    } else {
        rt.wheel_xy(x, y, dx, 0.0);
    }
    rest(rt);
    let key = e.key.as_deref().unwrap_or("?");
    let after = live(rt, key);
    assert!(
        (0..2).any(|i| (after.rect[i] - e.rect[i]).abs() > 0.01),
        "{what}: the wheel no longer moves {key} ({:?} in viewport {clip:?})",
        after.rect
    );
}

/// Scrolls the control keyed `key` into view whole: it must fit its viewport, and each step is
/// adjusted to what is still hidden.
pub fn reveal_control<S>(rt: &mut Runtime<S>, key: &str, size: (f32, f32)) {
    let what = format!("{key} cannot be reached whole at {size:?}");
    for _ in 0..200 {
        let e = live(rt, key);
        if whole(&e, size) {
            return;
        }
        let clip = viewport(rt, &e, size);
        assert!(
            e.rect[2] <= clip[2] + SLACK && e.rect[3] <= clip[3] + SLACK,
            "{what}: its box {:?} is larger than its viewport {clip:?}",
            e.rect
        );
        let dx = step(e.rect[0], e.rect[2], clip[0], clip[2], SIDE_NOTCH);
        let dy = step(e.rect[1], e.rect[3], clip[1], clip[3], NOTCH);
        assert!(dx != 0.0 || dy != 0.0, "{what}: inside its viewport {clip:?} yet clipped: {e:?}");
        turn(rt, &e, clip, dx, dy, &what);
    }
    panic!("{what}: still clipped after 200 wheel steps: {:?}", live(rt, key));
}

/// Opacity of `e` and of everything that holds it.
fn opaque<S>(rt: &Runtime<S>, e: &ElementInfo) -> bool {
    e.opacity >= 0.99
        && e.text_color.is_none_or(|c| c.a >= 0.99)
        && rt
            .elements()
            .iter()
            .filter(|o| e.path.starts_with(&format!("{}/", o.path)))
            .all(|o| o.opacity >= 0.99)
}

/// The whole text of the passage keyed `key` is laid out inside its box, uncut and opaque.
fn laid_out_whole<S>(rt: &Runtime<S>, e: &ElementInfo, what: &str) {
    assert!(e.text.as_deref().is_some_and(|t| !t.is_empty()), "{what}: it has no text: {e:?}");
    assert!(!e.truncate && !e.text_cut, "{what}: its text is truncated or clamped: {e:?}");
    let [w, h] = e.text_size.unwrap_or_else(|| panic!("{what}: no laid-out text size: {e:?}"));
    let inner = [e.rect[2] - e.inset[0] - e.inset[2], e.rect[3] - e.inset[1] - e.inset[3]];
    assert!(
        w <= inner[0] + SLACK && h <= inner[1] + SLACK,
        "{what}: its text ({w} x {h}) overflows its box ({inner:?})"
    );
    assert!(opaque(rt, e), "{what}: it or what holds it is not opaque: {e:?}");
}

/// Reads the passage of text keyed `key` from its first line to its last through its scroll
/// viewport: in one view when it fits, else from its top in overlapping views, each one showing
/// its full width, until its bottom is shown. Every band of its box must be seen in some view.
pub fn read_passage<S>(rt: &mut Runtime<S>, key: &str, size: (f32, f32)) -> Reading {
    let what = format!("{key} cannot be read whole at {size:?}");
    let e = live(rt, key);
    laid_out_whole(rt, &e, &what);
    let clip = viewport(rt, &e, size);
    assert!(
        e.rect[2] <= clip[2] + SLACK && e.rect[0] >= -SLACK && e.rect[2] <= size.0,
        "{what}: wider than its viewport {clip:?}: {e:?}"
    );
    if e.rect[3] <= clip[3] + SLACK {
        reveal_control(rt, key, size);
        let e = live(rt, key);
        laid_out_whole(rt, &e, &what);
        return Reading { views: 1, height: e.rect[3], viewport: clip[3] };
    }
    // Taller than its viewport: bring its top line to the viewport's top, horizontally whole.
    for _ in 0..200 {
        let e = live(rt, key);
        let clip = viewport(rt, &e, size);
        let dx = step(e.rect[0], e.rect[2], clip[0], clip[2], SIDE_NOTCH);
        let dy = (clip[1] - e.rect[1]).clamp(-NOTCH, NOTCH);
        if dx == 0.0 && dy.abs() <= SLACK {
            break;
        }
        turn(rt, &e, clip, dx, if dx == 0.0 { dy } else { 0.0 }, &what);
    }
    let mut seen: Vec<[f32; 2]> = Vec::new();
    for _ in 0..200 {
        let e = live(rt, key);
        let clip = viewport(rt, &e, size);
        assert!(
            (e.visible[0] - e.rect[0]).abs() <= SLACK
                && (e.visible[2] - e.rect[2]).abs() <= SLACK
                && e.rect[0] + e.rect[2] <= size.0 + SLACK,
            "{what}: a view cuts it sideways: {e:?}"
        );
        if e.visible[3] > 0.0 {
            let top = e.visible[1] - e.rect[1];
            seen.push([top, top + e.visible[3]]);
        }
        let hidden = e.rect[1] + e.rect[3] - (clip[1] + clip[3]);
        if hidden <= SLACK {
            break;
        }
        turn(rt, &e, clip, 0.0, -hidden.min(NOTCH), &what);
    }
    let e = live(rt, key);
    laid_out_whole(rt, &e, &what);
    let mut reached = 0.0_f32;
    seen.sort_by(|a, b| a[0].total_cmp(&b[0]));
    for band in &seen {
        assert!(band[0] <= reached + SLACK, "{what}: rows {reached}..{} are never shown: {seen:?}", band[0]);
        reached = reached.max(band[1]);
    }
    assert!(reached >= e.rect[3] - SLACK, "{what}: rows {reached}..{} are never shown: {seen:?}", e.rect[3]);
    Reading { views: seen.len(), height: e.rect[3], viewport: viewport(rt, &e, size)[3] }
}
