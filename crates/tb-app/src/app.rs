//! Configurator navigation and edits; drafts and successful-save baseline have separate owners.

use std::collections::BTreeMap;
use std::path::PathBuf;

use rune::tok::TextStyle;
use rune::{AppSpec, Cx, El, Family, Key, KeyEvent, Tx, el, text};
use tb_config::{ActiveTheme, ConfigFile, Loaded, default_path};
use tb_core::config::Config;
use tb_core::edit::{add_button, add_button_to_group, add_group, remove_button};
use tb_core::reorder::{Toward, step_button};
use tb_core::{BarSize, BarTheme, Button, ButtonId, Chord, Layer, SpacingRules, place};
use tb_render::view::color;

use crate::drafts::Fields;
use crate::preview::{Dragging, PREVIEW_SCALE};
use crate::saving::Saving;

/// Logical window size of configurator variant B.
pub const WINDOW: (f32, f32) = (1200.0, 760.0);

pub struct State {
    pub config: Config,
    pub theme: BarTheme,
    pub chord: Chord,
    pub selected: Option<ButtonId>,
    pub(crate) file: Option<ConfigFile>,
    pub(crate) fields: Fields,
    pub(crate) drafts: BTreeMap<(Chord, ButtonId), Fields>,
    pub(crate) drag: Option<Dragging>,
    pub(crate) saving: Saving,
    pub(crate) status: String,
}

impl State {
    /// Loads the given file or edits defaults with an explicit replacement/unavailable notice.
    /// Missing files are created by `ConfigFile`; rejected files remain untouched until a save.
    pub fn open(path: Option<PathBuf>, theme: BarTheme) -> Self {
        let mut s = Self {
            config: Config::default(),
            theme,
            chord: Chord::Default,
            selected: None,
            file: None,
            fields: Fields::default(),
            drafts: BTreeMap::new(),
            drag: None,
            saving: Saving::default(),
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
                s.saving.baseline = Some(s.config.clone());
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

    pub(crate) fn layer(&self) -> Option<&Layer> {
        self.config.layout.layer(self.chord)
    }
    pub(crate) fn layer_mut(&mut self) -> Option<&mut Layer> {
        self.config.layout.layer_mut(self.chord)
    }

    pub(crate) fn selected_button(&self) -> Option<&Button> {
        let id = self.selected.as_ref()?;
        self.layer()?.buttons().find(|b| &b.id == id)
    }

    pub(crate) fn selected_button_mut(&mut self) -> Option<&mut Button> {
        let id = self.selected.clone()?;
        self.layer_mut()?.groups.iter_mut().flat_map(|g| &mut g.buttons).find(|b| b.id == id)
    }

    fn remember(&mut self) {
        if let Some(id) = &self.selected {
            self.drafts.insert((self.chord, id.clone()), std::mem::take(&mut self.fields));
        }
    }

    pub(crate) fn select(&mut self, id: Option<ButtonId>) {
        if self.selected == id {
            return;
        }
        self.remember();
        self.selected = id;
        self.restore_fields();
    }

    fn restore_fields(&mut self) {
        let cached = self.selected.as_ref().and_then(|id| self.drafts.remove(&(self.chord, id.clone())));
        self.fields = cached.unwrap_or_else(|| Fields::of(self.selected_button()));
    }

    pub(crate) fn show_layer(&mut self, chord: Chord) {
        if self.chord == chord {
            return;
        }
        self.remember();
        self.chord = chord;
        self.selected = None;
        self.fields = Fields::default();
        self.drag = None;
    }

    fn step_selected(&mut self, toward: Toward) -> bool {
        let Some(id) = self.selected.clone() else { return false };
        if let Some(layer) = self.layer_mut() {
            step_button(layer, &id, toward);
        }
        true
    }

    fn ensure_layer(&mut self) {
        if self.layer().is_none() {
            self.config.layout.layers.push(Layer { chord: self.chord, group_gap: None, groups: vec![] });
        }
    }

    pub(crate) fn add(&mut self, group: bool) {
        self.ensure_layer();
        let after = self.selected.clone();
        let Some(layer) = self.layer_mut() else { return };
        let id = if group { add_group(layer, after.as_ref()) } else { add_button(layer, after.as_ref()) };
        self.select(Some(id));
    }

    pub(crate) fn add_to_group(&mut self, group: usize) {
        let Some(layer) = self.layer_mut() else { return };
        if let Some(id) = add_button_to_group(layer, group) {
            self.select(Some(id));
        }
    }

    pub(crate) fn remove(&mut self) {
        let Some(id) = self.selected.take() else { return };
        self.drafts.remove(&(self.chord, id.clone()));
        self.fields = Fields::default();
        if let Some(layer) = self.layer_mut() {
            remove_button(layer, &id);
        }
        let originally_absent =
            self.saving.baseline.as_ref().is_some_and(|b| b.layout.layer(self.chord).is_none());
        if originally_absent && self.layer().is_some_and(|l| l.groups.is_empty()) {
            self.config.layout.layers.retain(|l| l.chord != self.chord);
        }
    }
}

pub(crate) const NO_HOME: &str = "HOME is not set: the config cannot be saved";

/// The same save shortcut is captured at the root before focused text-input handling.
pub(crate) fn save_key(s: &mut State, ev: &KeyEvent, _: &mut Tx) -> bool {
    let save = ev.is_command('s') || matches!(&ev.key, Key::Text(t) if t.eq_ignore_ascii_case("s"));
    if save && (ev.mods.ctrl || ev.mods.cmd) && !ev.mods.alt && !ev.mods.shift {
        s.save();
        return true;
    }
    false
}

/// Alt+arrows reorder the selected button; all physical modifiers leave editing selection alone.
pub fn on_key(s: &mut State, ev: &KeyEvent, tx: &mut Tx) -> bool {
    if save_key(s, ev, tx) {
        return true;
    }
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
                s.fields = Fields::of(s.selected_button());
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

pub fn view(s: &State, cx: &mut Cx<State>) -> El {
    crate::shell::view(s, cx)
}

#[cfg(test)]
mod tests;
