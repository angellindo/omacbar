//! A modifier layer is a layer whose chord holds a modifier besides Fn (`GLOSSARY.md`): its keys
//! would combine with the held modifier (ADR-0011).
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use tb_core::Chord;

#[test]
fn fn_ctrl_fn_alt_fn_shift_and_fn_super_hold_a_modifier() {
    for chord in [Chord::FnCtrl, Chord::FnAlt, Chord::FnShift, Chord::FnSuper] {
        assert!(chord.holds_modifier(), "{chord:?}");
    }
}

#[test]
fn the_default_and_fn_chords_hold_no_modifier() {
    for chord in [Chord::Default, Chord::Fn] {
        assert!(!chord.holds_modifier(), "{chord:?}");
    }
}
