use tb_core::{BarSize, ButtonId, Rgba};

use super::*;

const WHOLE: Rect = Rect { x: 0, y: 0, width: 2170, height: 60 };

fn button(id: &str, x: u16) -> SceneButton {
    let grey = Rgba::opaque(40, 40, 40);
    SceneButton {
        id: ButtonId::new(id),
        label: id.to_owned(),
        rect: Rect { x, y: 8, width: 100, height: 44 },
        fill: grey,
        text: grey,
        border: grey,
    }
}

fn scene() -> BarScene {
    BarScene {
        size: BarSize::MACBOOK_PRO,
        background: Rgba::opaque(0, 0, 0),
        radius: 0,
        border_width: 2,
        font_size: 20,
        buttons: vec![button("a", 8), button("b", 112), button("c", 216)],
    }
}

#[test]
fn the_first_scene_damages_the_whole_bar() {
    assert_eq!(damage(None, &scene()), [WHOLE]);
}

#[test]
fn an_equal_scene_damages_nothing() {
    assert_eq!(damage(Some(&scene()), &scene()), []);
}

#[test]
fn a_new_text_colour_or_border_damages_only_that_button() {
    let mut new = scene();
    new.buttons[2].text = Rgba::opaque(9, 9, 9);
    assert_eq!(damage(Some(&scene()), &new), [new.buttons[2].rect]);
    let mut new = scene();
    new.buttons[0].border = Rgba::opaque(9, 9, 9);
    assert_eq!(damage(Some(&scene()), &new), [new.buttons[0].rect]);
}

#[test]
fn a_new_border_width_or_font_size_damages_the_whole_bar() {
    let mut new = scene();
    new.border_width = 3;
    assert_eq!(damage(Some(&scene()), &new), [WHOLE]);
    let mut new = scene();
    new.font_size = 18;
    assert_eq!(damage(Some(&scene()), &new), [WHOLE]);
}

#[test]
fn a_renamed_or_removed_button_damages_the_whole_bar() {
    let mut new = scene();
    new.buttons[1].id = ButtonId::new("z");
    assert_eq!(damage(Some(&scene()), &new), [WHOLE]);
    let mut new = scene();
    new.buttons.pop();
    assert_eq!(damage(Some(&scene()), &new), [WHOLE]);
}
