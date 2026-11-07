use super::*;

#[allow(clippy::fn_params_excessive_bools)]
fn held(fn_key: bool, ctrl: bool, alt: bool, shift: bool, super_key: bool) -> Modifiers {
    Modifiers { fn_key, ctrl, alt, shift, super_key }
}

#[test]
fn nothing_held_shows_the_default_f_key_layer() {
    assert_eq!(Chord::resolve(Modifiers::default(), &Chord::ALL), Chord::Default);
}

#[test]
fn modifiers_without_fn_keep_the_default_layer() {
    assert_eq!(Chord::resolve(held(false, true, true, false, false), &Chord::ALL), Chord::Default);
}

#[test]
fn fn_plus_one_modifier_selects_that_layer() {
    assert_eq!(Chord::resolve(held(true, true, false, false, false), &Chord::ALL), Chord::FnCtrl);
    assert_eq!(Chord::resolve(held(true, false, true, false, false), &Chord::ALL), Chord::FnAlt);
    assert_eq!(Chord::resolve(held(true, false, false, true, false), &Chord::ALL), Chord::FnShift);
    assert_eq!(Chord::resolve(held(true, false, false, false, true), &Chord::ALL), Chord::FnSuper);
}

#[test]
fn an_unconfigured_chord_falls_back_to_fn_then_default() {
    let fn_only = [Chord::Default, Chord::Fn];
    assert_eq!(Chord::resolve(held(true, true, false, false, false), &fn_only), Chord::Fn);
    let default_only = [Chord::Default];
    assert_eq!(Chord::resolve(held(true, true, false, false, false), &default_only), Chord::Default);
}

#[test]
fn fn_with_several_modifiers_falls_back_to_fn() {
    assert_eq!(Chord::resolve(held(true, true, true, false, false), &Chord::ALL), Chord::Fn);
}
