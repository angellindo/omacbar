//! Drives the real configurator view headless through `rune::Runtime`, as a user would: taps,
//! drags, Alt+arrows and typing into fields. Each configurator edits its own config file under
//! cargo's temp dir (never `$HOME`), drawn in the bundled Tokyo Night.
#![allow(dead_code)]

use std::fs;
use std::path::{Path, PathBuf};

use rune::{Env, Key, KeyEvent, Mods, Runtime};
use tb_app::State;
use tb_core::{BarTheme, Chord, Layer};

/// An empty directory for one test, under cargo's temp dir.
pub fn fresh_dir(suite: &str, test: &str) -> PathBuf {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join(suite).join(test);
    if dir.exists() {
        fs::remove_dir_all(&dir).unwrap();
    }
    fs::create_dir_all(&dir).unwrap();
    dir
}

pub fn theme() -> BarTheme {
    BarTheme::omarchy_fallback()
}

/// The configurator opened on the config file at `path`, laid out and at rest.
pub fn configurator(path: PathBuf) -> Runtime<State> {
    let mut rt =
        Runtime::new(State::open(Some(path), theme()), tb_app::view, tb_app::WINDOW, Env::new(true, 1.0));
    rt.on_key = Some(tb_app::on_key);
    rt.frame(0.0);
    settle(&mut rt);
    rt
}

/// Lets every animation land.
pub fn settle(rt: &mut Runtime<State>) {
    for _ in 0..30 {
        rt.frame(1.0 / 60.0);
    }
    rt.settle_all();
    rt.frame(0.0);
}

/// Centre of the element keyed `key`, in window points.
pub fn centre(rt: &Runtime<State>, key: &str) -> (f32, f32) {
    rt.find(key).unwrap_or_else(|| panic!("no element keyed {key}"))
}

pub fn shown(rt: &Runtime<State>, key: &str) -> bool {
    rt.find(key).is_some()
}

/// The drawn rect `[x, y, w, h]` of the element keyed `key`.
pub fn rect(rt: &Runtime<State>, key: &str) -> [f32; 4] {
    rt.presented(key).unwrap_or_else(|| panic!("no element keyed {key}")).0
}

/// A press and release on the element keyed `key`.
pub fn tap(rt: &mut Runtime<State>, key: &str) {
    let (x, y) = centre(rt, key);
    rt.pointer_move(x, y);
    rt.pointer_down(x, y);
    rt.frame(1.0 / 60.0);
    rt.pointer_up(x, y);
    settle(rt);
}

/// Presses the element keyed `key`, drags it along the row to window x `to_x` and releases it.
pub fn drag_to(rt: &mut Runtime<State>, key: &str, to_x: f32) {
    let (x, y) = centre(rt, key);
    rt.pointer_move(x, y);
    rt.pointer_down(x, y);
    rt.frame(1.0 / 60.0);
    let steps = 24;
    for i in 1..=steps {
        let f = i as f32 / steps as f32;
        rt.pointer_move(x + (to_x - x) * f, y);
        rt.frame(1.0 / 120.0);
    }
    rt.pointer_up(to_x, y);
    settle(rt);
}

/// A key press with `mods`.
pub fn press(rt: &mut Runtime<State>, key: Key, mods: Mods) {
    rt.key(KeyEvent { key, mods });
    settle(rt);
}

/// Alt (Option) plus `key`.
pub fn alt(rt: &mut Runtime<State>, key: Key) {
    press(rt, key, Mods { alt: true, ..Mods::default() });
}

/// Focuses the field keyed `key`, selects all of it, deletes it and types `text`.
pub fn type_into(rt: &mut Runtime<State>, key: &str, text: &str) {
    tap(rt, key);
    rt.key(KeyEvent { key: Key::Command('a'), mods: Mods { cmd: true, ..Mods::default() } });
    rt.frame(0.0);
    rt.key(KeyEvent { key: Key::Backspace, mods: Mods::default() });
    rt.frame(0.0);
    if !text.is_empty() {
        rt.key(KeyEvent { key: Key::Text(text.to_owned()), mods: Mods::default() });
    }
    settle(rt);
}

/// What the element keyed `key` shows: its own text, else the texts inside it joined by spaces.
pub fn text_of(rt: &Runtime<State>, key: &str) -> Option<String> {
    let elements = rt.elements();
    let e = elements.iter().find(|e| e.key.as_deref() == Some(key) && !e.exiting)?;
    if let Some(t) = &e.text {
        return Some(t.clone());
    }
    let inside = format!("{}/", e.path);
    let texts: Vec<&str> = elements
        .iter()
        .filter(|d| d.path.starts_with(&inside) && !d.exiting)
        .filter_map(|d| d.text.as_deref())
        .collect();
    (!texts.is_empty()).then(|| texts.join(" "))
}

/// Whether some text drawn inside the element keyed `key` reads exactly `text`.
pub fn shows_text_inside(rt: &Runtime<State>, key: &str, text: &str) -> bool {
    let elements = rt.elements();
    let Some(e) = elements.iter().find(|e| e.key.as_deref() == Some(key) && !e.exiting) else {
        return false;
    };
    let inside = format!("{}/", e.path);
    e.text.as_deref() == Some(text)
        || elements
            .iter()
            .any(|d| d.path.starts_with(&inside) && !d.exiting && d.text.as_deref() == Some(text))
}

/// Button ids per group, groups split by `|`: `"esc | f1 f2"`.
pub fn groups(spec: &str) -> Vec<Vec<String>> {
    spec.split('|').map(|g| g.split_whitespace().map(str::to_owned).collect()).collect()
}

pub fn ids(layer: &Layer) -> Vec<Vec<String>> {
    layer.groups.iter().map(|g| g.buttons.iter().map(|b| b.id.to_string()).collect()).collect()
}

/// The default layer the configurator edits.
pub fn default_layer(rt: &Runtime<State>) -> &Layer {
    rt.state.config.layout.layer(Chord::Default).expect("the config has a default layer")
}

/// Button ids per group of the default layer the configurator edits.
pub fn default_ids(rt: &Runtime<State>) -> Vec<Vec<String>> {
    ids(default_layer(rt))
}
