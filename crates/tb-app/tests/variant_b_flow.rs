//! Mirror of scripts/variant-b-flow.json: group add, reorder, edits and focused save.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod support {
    pub mod ui;
}
use rune::{Key, Mods};
use support::ui::*;
use tb_core::{Action, ButtonId};

#[test]
fn variant_b_group_edit_reorder_and_focused_save_round_trip_without_running_actions() {
    let path = fresh_dir("variant_b_flow", "round_trip").join("config.toml");
    let mut rt = configurator(path.clone());
    tap(&mut rt, "row-f3");
    alt(&mut rt, Key::Left);
    tap(&mut rt, "add-button-group-0");
    type_into(&mut rt, "field-label", "launch");
    type_into(&mut rt, "field-width", "px 180");
    tap(&mut rt, "action-command");
    type_into(&mut rt, "field-action", "printf configured");
    tap(&mut rt, "row-button-1");
    alt(&mut rt, Key::Left);
    assert_eq!(rt.state.selected, Some(ButtonId::new("button-1")));
    assert_eq!(text_of(&rt, "field-action").as_deref(), Some("printf configured"));
    tap(&mut rt, "field-action");
    press(&mut rt, Key::Command('s'), Mods { cmd: true, ctrl: true, ..Mods::default() });
    let mut file = tb_config::ConfigFile::new(path);
    assert_eq!(file.load(&theme()).unwrap(), tb_config::Loaded::Read);
    assert_eq!(file.config(), &rt.state.config);
    assert_eq!(default_ids(&rt), groups("button-1 esc | f1 f3 f2 f4 f5 f6 f7 f8 f9 f10 f11 f12"));
    assert_eq!(
        default_layer(&rt).groups[0].buttons[0].action,
        Action::Command { run: "printf configured".to_owned() }
    );
    assert!(text_of(&rt, "status").unwrap().starts_with("saved"));
}
