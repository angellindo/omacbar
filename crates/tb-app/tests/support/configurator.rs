//! Shared public Runtime driver for the configurator acceptance contracts.
#![allow(dead_code)]

use rune::{ElementInfo, Env, Key, Mods, Runtime};
use std::{fs, path::PathBuf};
use tb_app::State;
use tb_core::config::{Config, to_toml};
use tb_core::{Action, Button, Chord, Group, Layer, Width};

pub use super::ui::*;

pub fn fixture() -> Config {
    let group =
        |ids: &[&str]| Group { buttons: ids.iter().map(|id| Button::new(id, id, Action::None)).collect() };
    let mut config = Config::default();
    config.layout.layers = vec![
        Layer {
            chord: Chord::Default,
            group_gap: Some(28),
            groups: vec![group(&["a", "b"]), group(&["c"]), group(&["d", "e"])],
        },
        Layer { chord: Chord::Fn, group_gap: Some(44), groups: vec![group(&["a", "other"])] },
        Layer {
            chord: Chord::FnCtrl,
            group_gap: None,
            groups: vec![Group {
                buttons: vec![
                    Button::new("esc", "esc", Action::Key { keys: vec!["Esc".into()] })
                        .with_width(Width::Px(116)),
                    Button::new("legacy", "legacy", Action::Key { keys: vec!["F5".into()] }),
                    Button::new("ordinary", "ordinary", Action::None),
                ],
            }],
        },
    ];
    config
}

pub fn open(test: &str) -> Runtime<State> {
    open_config(test, &fixture(), (1200.0, 760.0)).0
}

pub fn open_config(test: &str, config: &Config, size: (f32, f32)) -> (Runtime<State>, PathBuf) {
    let path = fresh_dir("configurator_contract", test).join("config.toml");
    fs::write(&path, to_toml(config).unwrap()).unwrap();
    let state = State::open(Some(path.clone()), theme());
    assert_eq!(&state.config, config, "the hand-authored fixture was accepted");
    (runtime(state, size), path)
}

pub fn runtime(state: State, size: (f32, f32)) -> Runtime<State> {
    let mut rt = Runtime::new(state, tb_app::view, size, Env::new(true, 1.0));
    rt.on_key = Some(tb_app::on_key);
    rt.frame(0.0);
    settle(&mut rt);
    rt
}

pub fn element(rt: &Runtime<State>, key: &str) -> ElementInfo {
    let mut matches = rt.elements().into_iter().filter(|e| e.key.as_deref() == Some(key) && !e.exiting);
    let found = matches.next().unwrap_or_else(|| panic!("missing public Runtime key {key}"));
    assert!(matches.next().is_none(), "ambiguous Runtime key {key}");
    found
}

pub fn words(rt: &Runtime<State>, key: &str) -> String {
    text_of(rt, key).unwrap_or_else(|| panic!("{key} needs readable text"))
}

pub fn all_words(rt: &Runtime<State>) -> String {
    rt.elements()
        .iter()
        .filter(|e| !e.exiting)
        .filter_map(|e| e.text.as_deref())
        .collect::<Vec<_>>()
        .join(" ")
}

pub fn button(rt: &Runtime<State>, chord: Chord, id: &str) -> Button {
    rt.state.config.layout.layer(chord).unwrap().buttons().find(|b| b.id.as_str() == id).unwrap().clone()
}

pub fn near(actual: [f32; 4], expected: [f32; 4], tolerance: f32) {
    for i in 0..4 {
        assert!(
            (actual[i] - expected[i]).abs() <= tolerance,
            "geometry {actual:?} versus literal {expected:?}"
        );
    }
}

pub fn fully_visible(rt: &Runtime<State>, key: &str, size: (f32, f32)) -> bool {
    let e = element(rt, key);
    e.rect[2] > 0.0
        && e.rect[3] > 0.0
        && e.rect[0] >= -1.0
        && e.rect[1] >= -1.0
        && e.rect[0] + e.rect[2] <= size.0 + 1.0
        && e.rect[1] + e.rect[3] <= size.1 + 1.0
        && (0..4).all(|i| (e.rect[i] - e.visible[i]).abs() <= 1.0)
}

/// Real wheel input, including horizontal input; never move private scroll offsets.
pub fn reveal(rt: &mut Runtime<State>, key: &str, size: (f32, f32)) {
    for _ in 0..100 {
        if fully_visible(rt, key, size) {
            return;
        }
        let e = element(rt, key);
        let mut clip = [0.0, 0.0, size.0, size.1];
        // The spec keys each scroll owner. Its visible viewport, rather than the window,
        // bounds what its descendants can reach (e.g. body ends at 643 above the footer).
        for owner in rt.elements().iter().filter(|owner| {
            owner.key.as_deref().is_some_and(|k| k.ends_with("scroll"))
                && e.path.starts_with(&format!("{}/", owner.path))
                && owner.visible[2] > 0.0
                && owner.visible[3] > 0.0
        }) {
            let right = (clip[0] + clip[2]).min(owner.visible[0] + owner.visible[2]);
            let bottom = (clip[1] + clip[3]).min(owner.visible[1] + owner.visible[3]);
            clip[0] = clip[0].max(owner.visible[0]);
            clip[1] = clip[1].max(owner.visible[1]);
            clip[2] = (right - clip[0]).max(0.0);
            clip[3] = (bottom - clip[1]).max(0.0);
        }
        assert!(clip[2] > 2.0 && clip[3] > 2.0, "no reachable viewport for {key}: {clip:?}");
        let x = (e.rect[0] + e.rect[2] / 2.0).clamp(clip[0] + 1.0, clip[0] + clip[2] - 1.0);
        let y = (e.rect[1] + e.rect[3] / 2.0).clamp(clip[1] + 1.0, clip[1] + clip[3] - 1.0);
        let dx = if e.rect[0] < clip[0] - 1.0 {
            80.0
        } else if e.rect[0] + e.rect[2] > clip[0] + clip[2] + 1.0 {
            -80.0
        } else {
            0.0
        };
        let dy = if e.rect[1] < clip[1] - 1.0 {
            60.0
        } else if e.rect[1] + e.rect[3] > clip[1] + clip[3] + 1.0 {
            -60.0
        } else {
            0.0
        };
        if dx == 0.0 {
            rt.wheel(x, y, dy);
        } else {
            rt.wheel_xy(x, y, dx, 0.0);
        }
        settle(rt);
    }
    assert!(fully_visible(rt, key, size), "{key} remains clipped: {:?}", element(rt, key));
}

pub fn operate(rt: &mut Runtime<State>, key: &str) {
    reveal(rt, key, (1200.0, 760.0));
    tap(rt, key);
}

pub fn edit(rt: &mut Runtime<State>, key: &str, value: &str) {
    reveal(rt, key, (1200.0, 760.0));
    type_into(rt, key, value);
}

pub fn drag(rt: &mut Runtime<State>, from: &str, to: &str, fraction: f32) {
    let (x, y) = centre(rt, from);
    let target = rect(rt, to);
    let (tx, ty) = (target[0] + target[2] * fraction, target[1] + target[3] / 2.0);
    rt.pointer_move(x, y);
    rt.pointer_down(x, y);
    rt.frame(1.0 / 60.0);
    for n in 1..=24 {
        let f = n as f32 / 24.0;
        rt.pointer_move(x + (tx - x) * f, y + (ty - y) * f);
        rt.frame(1.0 / 120.0);
    }
    rt.pointer_up(tx, ty);
    settle(rt);
}

pub fn save_shortcut(rt: &mut Runtime<State>) {
    press(rt, Key::Command('s'), Mods { cmd: true, ctrl: true, ..Mods::default() });
}

pub fn disk(path: &std::path::Path) -> Config {
    let mut file = tb_config::ConfigFile::new(path);
    assert_eq!(file.load(&theme()).unwrap(), tb_config::Loaded::Read);
    file.config().clone()
}

pub fn assert_clean(rt: &Runtime<State>) {
    let summary = words(rt, "dirty-summary").to_lowercase();
    assert!(
        summary.contains('0') || summary.contains("no pending") || summary.contains("no changes"),
        "{summary}"
    );
    assert!(!summary.contains("1 layer"), "{summary}");
}

pub fn assert_dirty(rt: &Runtime<State>, layers: usize) {
    let summary = words(rt, "dirty-summary").to_lowercase();
    assert!(summary.contains(&layers.to_string()) && summary.contains("layer"), "{summary}");
}

pub fn disabled(rt: &Runtime<State>, key: &str) -> bool {
    rt.accessibility_tree().nodes.iter().any(|(_, n)| n.author_id() == Some(key) && n.is_disabled())
}

pub fn assert_blocked(rt: &Runtime<State>, targets: &[&str]) {
    assert!(disabled(rt, "save"), "the actual Save element needs disabled accessibility semantics");
    assert!(!element(rt, "save").focusable, "blocked Save cannot receive keyboard focus");
    let reason = words(rt, "save-blocked-reason");
    for target in targets {
        assert!(reason.contains(target), "missing actionable target {target}: {reason}");
    }
}
