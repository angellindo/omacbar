//! Configurator state and view: the config file it edits, the shown layer, the selected button,
//! and what a key, a press or the save button does to them.

use std::path::PathBuf;

use rune::tok::TextStyle;
use rune::{AppSpec, Cx, El, Family, Key, KeyEvent, Tx, col, el, row, text};
use tb_config::{ActiveTheme, ConfigFile, ConfigFileError, Loaded, default_path};
use tb_core::config::Config;
use tb_core::reorder::{Toward, step_button};
use tb_core::{BarSize, BarTheme, Button, ButtonId, Chord, Layer, SpacingRules, place};
use tb_render::view::color;

use crate::panel::{Fields, issues, panel, toolbar};
use crate::preview::{Dragging, PREVIEW_SCALE, preview};

/// The window size: the bar preview on top, the button list and edit panel below it.
pub const WINDOW: (f32, f32) = (1200.0, 760.0);

pub struct State {
    /// What is shown, edited and saved.
    pub config: Config,
    pub theme: BarTheme,
    /// The shown layer.
    pub chord: Chord,
    /// The selected button on the shown layer.
    pub selected: Option<ButtonId>,
    /// The file `save` writes; `None` when `HOME` is unset.
    file: Option<ConfigFile>,
    /// The edit panel's text buffers for the selected button.
    pub(crate) fields: Fields,
    pub(crate) drag: Option<Dragging>,
    status: String,
}

impl State {
    /// The configurator on the config file at `path` (None: `HOME` unset), drawn in `theme`.
    /// Loads it with `ConfigFile::load` (writes the default when missing); an invalid or
    /// unreadable file leaves the default config in the editor, the file untouched, and says so
    /// in the status. Without a path the default config is edited and saving is refused.
    pub fn open(path: Option<PathBuf>, theme: BarTheme) -> State {
        let mut s = State {
            config: Config::default(),
            theme,
            chord: Chord::Default,
            selected: None,
            file: None,
            fields: Fields::default(),
            drag: None,
            status: String::new(),
        };
        let Some(path) = path else {
            NO_HOME.clone_into(&mut s.status);
            return s;
        };
        let mut file = ConfigFile::new(path);
        let shown = file.path().display().to_string();
        s.status = match file.load(&s.theme) {
            Ok(Loaded::Read | Loaded::WroteDefault) => {
                s.config = file.config().clone();
                format!("editing {shown}")
            }
            Ok(Loaded::Rejected(issues)) => {
                format!("{shown} has {} issue(s): editing the default; saving replaces it", issues.len())
            }
            Err(e) => format!("{e}: editing the default; saving replaces it"),
        };
        s.file = Some(file);
        s
    }

    /// The shown layer.
    pub(crate) fn layer(&self) -> Option<&Layer> {
        self.config.layout.layer(self.chord)
    }

    pub(crate) fn layer_mut(&mut self) -> Option<&mut Layer> {
        self.config.layout.layer_mut(self.chord)
    }

    /// The selected button, when it is on the shown layer.
    pub(crate) fn selected_button(&self) -> Option<&Button> {
        let id = self.selected.as_ref()?;
        self.layer()?.buttons().find(|b| &b.id == id)
    }

    pub(crate) fn selected_button_mut(&mut self) -> Option<&mut Button> {
        let id = self.selected.clone()?;
        self.layer_mut()?.groups.iter_mut().flat_map(|g| g.buttons.iter_mut()).find(|b| b.id == id)
    }

    /// Makes `id` the selected button and fills the edit panel with its fields.
    pub(crate) fn select(&mut self, id: Option<ButtonId>) {
        self.selected = id;
        self.fields = Fields::of(self.selected_button());
    }

    /// Moves the selected button one step toward `toward`; false when nothing is selected.
    fn step_selected(&mut self, toward: Toward) -> bool {
        let Some(id) = self.selected.clone() else { return false };
        if let Some(layer) = self.layer_mut() {
            step_button(layer, &id, toward);
        }
        true
    }

    fn show_layer(&mut self, chord: Chord) {
        self.chord = chord;
        self.select(None);
    }

    /// Writes the config through `ConfigFile::save`, which refuses a config with issues.
    pub(crate) fn save(&mut self) {
        let Some(file) = &mut self.file else {
            NO_HOME.clone_into(&mut self.status);
            return;
        };
        self.status = match file.save(self.config.clone(), &self.theme) {
            Ok(()) => format!("saved {}", file.path().display()),
            Err(ConfigFileError::Invalid(issues)) => format!("not saved: {} issue(s)", issues.len()),
            Err(e) => format!("not saved: {e}"),
        };
    }
}

const NO_HOME: &str = "HOME is not set: the config cannot be saved";

/// Alt+Left / Alt+Right move the selected button one step toward the bar's left / right edge.
/// Other keys, other modifiers, or no selection: not consumed.
pub fn on_key(s: &mut State, ev: &KeyEvent, _: &mut Tx) -> bool {
    let m = ev.mods;
    if !m.alt || m.ctrl || m.cmd || m.logo {
        return false;
    }
    let toward = match ev.key {
        Key::Left => Toward::Left,
        Key::Right => Toward::Right,
        _ => return false,
    };
    s.step_selected(toward)
}

/// Startup: `$HOME` is read once, here. The config file is `default_path(home)`, drawn in the
/// active Omarchy theme of that home (`ActiveTheme::load`); without `HOME`, the bundled Tokyo
/// Night and no file.
fn init() -> State {
    let home = std::env::var_os("HOME").map(PathBuf::from);
    let theme = home
        .as_deref()
        .map_or_else(BarTheme::omarchy_fallback, |home| ActiveTheme::under_home(home).load().theme);
    State::open(home.as_deref().map(default_path), theme)
}

pub fn spec() -> AppSpec<State> {
    AppSpec {
        scenes: &[
            ("fn", |s| s.show_layer(Chord::Fn)),
            ("fn-ctrl", |s| s.show_layer(Chord::FnCtrl)),
            ("selected", |s| s.select(Some(ButtonId::new("f3")))),
            ("dragging", |s| {
                s.select(Some(ButtonId::new("f3")));
                lift(s, "f3");
            }),
            ("reordered", |s| {
                s.select(Some(ButtonId::new("f3")));
                s.step_selected(Toward::Left);
                s.step_selected(Toward::Left);
            }),
            ("added", |s| {
                s.select(Some(ButtonId::new("f3")));
                s.add(false);
            }),
            ("issues", |s| {
                let f3 = ButtonId::new("f3");
                s.select(Some(f3.clone()));
                if let Some(b) = s.selected_button_mut() {
                    b.label.clear();
                    b.style.fill = Some("nope".to_owned());
                }
                s.select(Some(f3));
            }),
        ],
        on_key: Some(on_key),
        ..AppSpec::new("Touch Bar Customizer", WINDOW, init, view)
    }
}

/// The `dragging` scene: button `id` lifted and carried left by its own width, not yet dropped.
fn lift(s: &mut State, id: &str) {
    let width = s
        .layer()
        .and_then(|layer| place(layer, BarSize::MACBOOK_PRO, &SpacingRules::STANDARD).ok())
        .and_then(|placed| placed.into_iter().find(|p| p.id.as_str() == id))
        .map(|p| f32::from(p.rect.width) * PREVIEW_SCALE);
    if let Some(width) = width {
        s.drag = Some(Dragging { id: ButtonId::new(id), dx: -width });
    }
}

pub(crate) fn body() -> TextStyle {
    TextStyle::new(14.0, 500.0).family(Family::SystemMono)
}

/// A pressable, keyed text button in the configurator's look; `active` draws it in the accent.
pub(crate) fn pressable(t: &BarTheme, key: &str, label: &str, active: bool) -> El {
    el().key(key)
        .role("button")
        .label(label)
        .padding(8.0)
        .fill(color(if active { t.accent } else { t.button_fill }))
        .border(1.0, color(t.button_border))
        .child(text(label).font(body()).color(color(if active { t.pressed_text } else { t.button_text })))
}

pub fn view(s: &State, _: &mut Cx<State>) -> El {
    let t = &s.theme;
    let tabs = row().gap(8.0).children(s.config.layout.chords().into_iter().map(|chord| {
        let key = format!("tab-{chord:?}").to_lowercase();
        pressable(t, &key, chord.label(), chord == s.chord).on_press(move |s: &mut State| s.show_layer(chord))
    }));
    col()
        .w_full()
        .h_full()
        .padding(24.0)
        .gap(16.0)
        .fill(color(t.background))
        .child(tabs)
        .child(preview(s))
        .child(toolbar(s))
        .child(row().gap(32.0).children(button_list(s)).child(panel(s)))
        .child(issues(s))
        .child(text(s.status.clone()).key("status").w_full().wrap().font(body()).color(color(t.button_text)))
}

/// One row per button of the shown layer; pressing a row selects its button.
fn button_list(s: &State) -> Option<El> {
    let t = &s.theme;
    let layer = s.layer()?;
    Some(col().key("button-list").gap(2.0).children(layer.buttons().map(|b| {
        let selected = s.selected.as_ref() == Some(&b.id);
        let id = b.id.clone();
        el().key(format!("row-{}", b.id))
            .role("button")
            .padding(2.0)
            .fill(color(if selected { t.accent } else { t.background }))
            .on_press_tx(move |s: &mut State, tx: &mut Tx| {
                s.select(Some(id.clone()));
                tx.blur();
            })
            .child(
                text(format!("{:<12} {:<10} {}", b.id, b.label, b.width))
                    .font(body())
                    .color(color(if selected { t.pressed_text } else { t.button_text })),
            )
    })))
}

#[cfg(test)]
mod tests {
    use super::*;
    use rune::{Env, Runtime};

    #[test]
    fn tapping_a_layer_tab_switches_the_previewed_layer_and_clears_the_selection() {
        let mut state = State::open(None, BarTheme::omarchy_fallback());
        state.select(Some(ButtonId::new("f1")));
        let mut rt = Runtime::new(state, view, WINDOW, Env::new(true, 1.0));
        rt.frame(0.0);
        assert!(rt.presented("preview-f1").is_some(), "starts on the F-key layer");
        let (x, y) = rt.find("tab-fnctrl").unwrap();
        rt.pointer_down(x, y);
        rt.pointer_up(x, y);
        for _ in 0..30 {
            rt.frame(1.0 / 60.0);
        }
        assert_eq!(rt.state.chord, Chord::FnCtrl);
        assert_eq!(rt.state.selected, None);
        assert!(rt.find("preview-lock").is_some(), "Fn + Ctrl buttons are previewed");
        assert!(rt.find("preview-f1").is_none(), "F-keys are gone (lookups skip exiting elements)");
    }

    #[test]
    fn without_home_the_default_config_is_edited_and_saving_is_refused() {
        let mut s = State::open(None, BarTheme::omarchy_fallback());
        assert_eq!(s.config, Config::default());
        s.save();
        assert_eq!(s.status, NO_HOME);
    }
}
