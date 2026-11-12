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
