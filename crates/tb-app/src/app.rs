//! Configurator state and view. Editing (add, edit, drag to reorder) lands in TB tickets; this is
//! the tracer: pick a layer, see it drawn by the real bar renderer at preview scale.

use rune::tok::TextStyle;
use rune::{AppSpec, Cx, El, col, el, row, text};
use tb_core::{BarSize, BarTheme, Chord, Layout, SpacingRules, presets};
use tb_render::{BarScene, build_scene, view::color};

/// Preview scale: the 2170 px bar shown at half size fits a 1200 pt window.
const PREVIEW_SCALE: f32 = 0.5;

pub struct State {
    pub layout: Layout,
    pub theme: BarTheme,
    pub chord: Chord,
}

fn init() -> State {
    State { layout: presets::default_layout(), theme: BarTheme::omarchy_fallback(), chord: Chord::Default }
}

pub fn spec() -> AppSpec<State> {
    AppSpec {
        scenes: &[("fn", |s| s.chord = Chord::Fn), ("fn-ctrl", |s| s.chord = Chord::FnCtrl)],
        ..AppSpec::new("Touch Bar Customizer", (1200.0, 360.0), init, view)
    }
}

fn scene(s: &State) -> Option<BarScene> {
    let layer = s.layout.layer(s.chord)?;
    build_scene(layer, &s.theme, BarSize::MACBOOK_PRO, &SpacingRules::STANDARD, None).ok()
}

fn view(s: &State, _: &mut Cx<State>) -> El {
    let t = &s.theme;
    let body = TextStyle::new(14.0, 500.0).mono();
    let tabs = row().gap(8.0).children(s.layout.chords().into_iter().map(|chord| {
        let active = chord == s.chord;
        el().key(format!("tab-{chord:?}").to_lowercase())
            .role("tab")
            .label(chord.label())
            .padding(8.0)
            .fill(color(if active { t.accent } else { t.button_fill }))
            .border(1.0, color(t.button_border))
            .on_press(move |s: &mut State| s.chord = chord)
            .child(text(chord.label()).font(body).color(color(if active {
                t.pressed_text
            } else {
                t.button_text
            })))
    }));
    let list = s.layout.layer(s.chord).map(|layer| {
        col().key("button-list").gap(4.0).children(layer.buttons().map(|b| {
            text(format!("{:<18} {:<8} {:?}", b.id, b.label, b.width)).font(body).color(color(t.button_text))
        }))
    });
    col()
        .w_full()
        .h_full()
        .padding(24.0)
        .gap(16.0)
        .fill(color(t.background))
        .child(tabs)
        .child(preview(s))
        .children(list)
}

/// The bar exactly as the renderer lays it out, scaled down.
fn preview(s: &State) -> El {
    let Some(scene) = scene(s) else {
        return text("this layer does not fit the bar").color(color(s.theme.accent));
    };
    let k = PREVIEW_SCALE;
    let label = TextStyle::new(f32::from(scene.font_size) * k, 600.0).mono();
    el().key("preview")
        .size(f32::from(scene.size.width) * k, f32::from(scene.size.height) * k)
        .fill(color(scene.background))
        .children(scene.buttons.iter().map(|b| {
            el().key(format!("preview-{}", b.id))
                .absolute(f32::from(b.rect.x) * k, f32::from(b.rect.y) * k)
                .size(f32::from(b.rect.width) * k, f32::from(b.rect.height) * k)
                .fill(color(b.fill))
                .border(1.0, color(b.border))
                .center()
                .child(text(b.label.clone()).font(label).color(color(b.text)))
        }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use rune::{Env, Runtime};

    #[test]
    fn tapping_a_layer_tab_switches_the_previewed_layer() {
        let mut rt = Runtime::new(init(), view, (1200.0, 360.0), Env::new(true, 1.0));
        rt.frame(0.0);
        assert!(rt.presented("preview-f1").is_some(), "starts on the F-key layer");
        let (x, y) = rt.find("tab-fnctrl").unwrap();
        rt.pointer_down(x, y);
        rt.pointer_up(x, y);
        for _ in 0..30 {
            rt.frame(1.0 / 60.0);
        }
        assert_eq!(rt.state.chord, Chord::FnCtrl);
        assert!(rt.find("preview-lock").is_some(), "Fn + Ctrl buttons are previewed");
        assert!(rt.find("preview-f1").is_none(), "F-keys are gone (lookups skip exiting elements)");
    }
}
