//! The daemon loop: applies events, renders only when the scene changed, presents its damage.

use tb_core::hit::hit;
use tb_core::{
    BarSize, BarTheme, Button, ButtonId, Chord, Layer, Layout, LayoutError, Modifiers, Placed, SpacingRules,
    place,
};
use tb_render::{BarScene, build_scene};
use thiserror::Error;

use crate::action::Actions;
use crate::damage::damage;
use crate::display::{Display, DisplayError};
use crate::render::SceneRenderer;
use crate::touch::Touch;

const SIZE: BarSize = BarSize::MACBOOK_PRO;
const RULES: SpacingRules = SpacingRules::STANDARD;

/// Something that may change what the bar shows.
#[derive(Debug, Clone)]
pub enum Event {
    Touch(Touch),
    /// The modifier keys held now; they pick the shown layer.
    Modifiers(Modifiers),
    Theme(BarTheme),
    Layout(Layout),
    /// A saved config's layout and selected theme, accepted together.
    Config {
        layout: Layout,
        theme: BarTheme,
    },
}

#[derive(Debug, Error)]
pub enum DaemonError {
    #[error("the layout has no layer for the held keys")]
    NoLayer,
    #[error(transparent)]
    Layout(#[from] LayoutError),
    #[error(transparent)]
    Display(#[from] DisplayError),
}

/// Renders the shown layer and presents it on `display` whenever an event changes it.
pub struct Daemon<R: SceneRenderer, D: Display> {
    layout: Layout,
    theme: BarTheme,
    modifiers: Modifiers,
    pressed: Option<ButtonId>,
    /// The touch now down, from its first `Down` to its `Up`.
    touch: Option<Landing>,
    actions: Actions,
    /// The scene on the display now; `None` until the first present.
    shown: Option<BarScene>,
    renderer: R,
    display: D,
}

impl<R: SceneRenderer, D: Display> Daemon<R, D> {
    /// Shows `Chord::Default` with nothing pressed. Presents nothing yet. Taps run no action
    /// ([`Actions::none`]) until [`Daemon::with_actions`].
    pub fn new(layout: Layout, theme: BarTheme, renderer: R, display: D) -> Self {
        Self {
            layout,
            theme,
            modifiers: Modifiers::default(),
            pressed: None,
            touch: None,
            actions: Actions::none(),
            shown: None,
            renderer,
            display,
        }
    }

    /// Runs `actions` on every tap: a touch that lands on a button and lifts while over it. The
    /// action is the one of the button and layer it landed on, even if the modifiers or the
    /// layout changed meanwhile; it runs once, on the lift.
    #[must_use]
    pub fn with_actions(mut self, actions: Actions) -> Self {
        self.actions = actions;
        self
    }

    pub fn display(&self) -> &D {
        &self.display
    }

    /// Applies one event; renders and presents only if the scene changed, with its damage.
    /// On error the daemon keeps the last good state and presents nothing.
    pub fn handle(&mut self, event: Event) -> Result<(), DaemonError> {
        let pressed = self.pressed.as_ref();
        match event {
            Event::Config { layout, theme } => {
                let scene = scene(&layout, &theme, self.modifiers, pressed)?;
                self.present(scene)?;
                self.layout = layout;
                self.theme = theme;
            }
            Event::Modifiers(m) => {
                let scene = scene(&self.layout, &self.theme, m, pressed)?;
                self.present(scene)?;
                self.modifiers = m;
            }
            Event::Theme(theme) => {
                let scene = scene(&self.layout, &theme, self.modifiers, pressed)?;
                self.present(scene)?;
                self.theme = theme;
            }
            Event::Layout(layout) => {
                let scene = scene(&layout, &self.theme, self.modifiers, pressed)?;
                self.present(scene)?;
                self.layout = layout;
            }
            Event::Touch(Touch::Down { x }) => {
                let layer = shown_layer(&self.layout, self.modifiers)?;
                let placed = place(layer, SIZE, &RULES)?;
                let id = hit(&placed, x, &RULES).cloned();
                match &mut self.touch {
                    Some(landing) => landing.slide(x),
                    None => self.touch = Some(Landing::new(layer, placed, id.as_ref())),
                }
                // A miss (bar edge, group gap) changes nothing.
                let Some(id) = id else { return Ok(()) };
                let scene = scene(&self.layout, &self.theme, self.modifiers, Some(&id))?;
                self.present(scene)?;
                self.pressed = Some(id);
            }
            Event::Touch(Touch::Up) => {
                if let Some((button, chord)) = self.touch.take().and_then(Landing::tapped) {
                    self.actions.run(&button, chord);
                }
                let scene = scene(&self.layout, &self.theme, self.modifiers, None)?;
                self.present(scene)?;
                self.pressed = None;
            }
        }
        Ok(())
    }

    /// Presents the current bar (whole-bar damage the first time), then handles every event in
    /// order; blocks between events (idle). Returns when the events end, or with the first error.
    pub fn run(&mut self, events: impl IntoIterator<Item = Event>) -> Result<(), DaemonError> {
        let start = scene(&self.layout, &self.theme, self.modifiers, self.pressed.as_ref())?;
        self.present(start)?;
        events.into_iter().try_for_each(|event| self.handle(event))
    }

    /// Renders and presents `scene` with its damage, unless it equals the scene shown.
    fn present(&mut self, scene: BarScene) -> Result<(), DaemonError> {
        let damage = damage(self.shown.as_ref(), &scene);
        if damage.is_empty() {
            return Ok(());
        }
        let frame = self.renderer.render(&scene);
        self.display.present(&frame, &damage)?;
        self.shown = Some(scene);
        Ok(())
    }
}

/// Where a touch landed: the button hit on the shown layer (`None` on a miss), and whether the
/// touch is still over it.
struct Landing {
    chord: Chord,
    button: Option<Button>,
    /// The landing layer's placement, to follow the touch as it slides.
    placed: Vec<Placed>,
    over: bool,
}

impl Landing {
    fn new(layer: &Layer, placed: Vec<Placed>, id: Option<&ButtonId>) -> Self {
        let button = id.and_then(|id| layer.buttons().find(|b| &b.id == id)).cloned();
        Self { chord: layer.chord, over: button.is_some(), button, placed }
    }

    fn slide(&mut self, x: u16) {
        let id = self.button.as_ref().map(|b| &b.id);
        self.over = id.is_some() && hit(&self.placed, x, &RULES) == id;
    }

    /// The button and its layer, if the touch landed on a button and lifted over it.
    fn tapped(self) -> Option<(Button, Chord)> {
        let chord = self.chord;
        self.button.filter(|_| self.over).map(|button| (button, chord))
    }
}

/// The layer for `modifiers`, with `Chord::resolve`'s fallback (requested, then Fn, then Default).
fn shown_layer(layout: &Layout, modifiers: Modifiers) -> Result<&Layer, DaemonError> {
    layout.layer(Chord::resolve(modifiers, &layout.chords())).ok_or(DaemonError::NoLayer)
}

fn scene(
    layout: &Layout,
    theme: &BarTheme,
    modifiers: Modifiers,
    pressed: Option<&ButtonId>,
) -> Result<BarScene, DaemonError> {
    Ok(build_scene(shown_layer(layout, modifiers)?, theme, SIZE, &RULES, pressed)?)
}
