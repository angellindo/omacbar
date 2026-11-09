//! Built-in layouts. The default layer is always the F-key row.

use crate::chord::Chord;
use crate::model::{Action, Button, ButtonStyle, Group, Layer, Layout, Width};

fn key(id: &str, label: &str, keys: &[&str]) -> Button {
    Button::new(id, label, Action::Key { keys: keys.iter().map(|k| (*k).to_owned()).collect() })
}

fn cmd(id: &str, label: &str, run: &str) -> Button {
    Button::new(id, label, Action::Command { run: run.to_owned() })
}

/// Sends a plain Escape through Hyprland: on a modifier layer a pressed key would combine with the
/// held modifier, so an exact key is a `send_shortcut` command (ADR-0011).
const SEND_ESCAPE: &str = r#"hyprctl dispatch 'hl.dsp.send_shortcut({ mods = "", key = "Escape" })'"#;

/// Esc | F1..F12 by default; media keys on Fn; Omarchy shortcuts on Fn + Ctrl.
pub fn default_layout() -> Layout {
    let esc = key("esc", "esc", &["Esc"]).with_width(Width::Px(120));
    let f_keys = (1..=12).map(|n| key(&format!("f{n}"), &format!("F{n}"), &[&format!("F{n}")])).collect();
    let default = Layer {
        chord: Chord::Default,
        groups: vec![Group { buttons: vec![esc.clone()] }, Group { buttons: f_keys }],
    };

    let media = Layer {
        chord: Chord::Fn,
        groups: vec![
            Group { buttons: vec![esc] },
            Group {
                buttons: vec![
                    key("brightness-down", "bri-", &["BrightnessDown"]),
                    key("brightness-up", "bri+", &["BrightnessUp"]),
                ],
            },
            Group {
                buttons: vec![
                    key("previous", "prev", &["PreviousSong"]),
                    key("play-pause", "play", &["PlayPause"]),
                    key("next", "next", &["NextSong"]),
                ],
            },
            Group {
                buttons: vec![
                    key("mute", "mute", &["Mute"]),
                    key("volume-down", "vol-", &["VolumeDown"]),
                    key("volume-up", "vol+", &["VolumeUp"]),
                ],
            },
        ],
    };

    let mut lock = cmd("lock", "lock", "omarchy lock");
    lock.style =
        ButtonStyle { fill: Some("red".to_owned()), text: Some("background".to_owned()), border: None };
    let omarchy = Layer {
        chord: Chord::FnCtrl,
        groups: vec![
            Group { buttons: vec![cmd("esc", "esc", SEND_ESCAPE).with_width(Width::Px(120))] },
            Group {
                buttons: vec![
                    cmd("theme-next", "theme", "omarchy theme switcher").with_width(Width::Span(2)),
                    cmd("screenshot", "shot", "omarchy screenshot"),
                    cmd("background-next", "bg", "omarchy theme bg next"),
                ],
            },
            Group { buttons: vec![lock] },
        ],
    };

    Layout { layers: vec![default, media, omarchy] }
}
