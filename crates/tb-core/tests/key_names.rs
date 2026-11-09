//! A key is named by its Linux `KEY_*` name without `KEY_`, spelled exactly (case-sensitive, no
//! aliases), and carries the kernel's key code. Every expected code below was read from
//! `/usr/include/linux/input-event-codes.h`, never from the implementation.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::collections::BTreeSet;

use tb_core::{Action, Key, presets};

/// The whole key vocabulary: name, then its `KEY_*` code from `linux/input-event-codes.h`.
const VOCABULARY: [(&str, u16); 111] = [
    ("Esc", 1),
    ("F1", 59),
    ("F2", 60),
    ("F3", 61),
    ("F4", 62),
    ("F5", 63),
    ("F6", 64),
    ("F7", 65),
    ("F8", 66),
    ("F9", 67),
    ("F10", 68),
    ("F11", 87),
    ("F12", 88),
    ("F13", 183),
    ("F14", 184),
    ("F15", 185),
    ("F16", 186),
    ("F17", 187),
    ("F18", 188),
    ("F19", 189),
    ("F20", 190),
    ("F21", 191),
    ("F22", 192),
    ("F23", 193),
    ("F24", 194),
    ("1", 2),
    ("2", 3),
    ("3", 4),
    ("4", 5),
    ("5", 6),
    ("6", 7),
    ("7", 8),
    ("8", 9),
    ("9", 10),
    ("0", 11),
    ("A", 30),
    ("B", 48),
    ("C", 46),
    ("D", 32),
    ("E", 18),
    ("F", 33),
    ("G", 34),
    ("H", 35),
    ("I", 23),
    ("J", 36),
    ("K", 37),
    ("L", 38),
    ("M", 50),
    ("N", 49),
    ("O", 24),
    ("P", 25),
    ("Q", 16),
    ("R", 19),
    ("S", 31),
    ("T", 20),
    ("U", 22),
    ("V", 47),
    ("W", 17),
    ("X", 45),
    ("Y", 21),
    ("Z", 44),
    ("Minus", 12),
    ("Equal", 13),
    ("Backspace", 14),
    ("Tab", 15),
    ("LeftBrace", 26),
    ("RightBrace", 27),
    ("Enter", 28),
    ("Semicolon", 39),
    ("Apostrophe", 40),
    ("Grave", 41),
    ("Backslash", 43),
    ("Comma", 51),
    ("Dot", 52),
    ("Slash", 53),
    ("Space", 57),
    ("CapsLock", 58),
    ("LeftCtrl", 29),
    ("RightCtrl", 97),
    ("LeftShift", 42),
    ("RightShift", 54),
    ("LeftAlt", 56),
    ("RightAlt", 100),
    ("LeftMeta", 125),
    ("RightMeta", 126),
    ("SysRq", 99),
    ("Print", 210),
    ("Home", 102),
    ("End", 107),
    ("PageUp", 104),
    ("PageDown", 109),
    ("Up", 103),
    ("Down", 108),
    ("Left", 105),
    ("Right", 106),
    ("Insert", 110),
    ("Delete", 111),
    ("Mute", 113),
    ("VolumeDown", 114),
    ("VolumeUp", 115),
    ("MicMute", 248),
    ("PlayPause", 164),
    ("NextSong", 163),
    ("PreviousSong", 165),
    ("StopCD", 166),
    ("BrightnessDown", 224),
    ("BrightnessUp", 225),
    ("KbdIllumToggle", 228),
    ("KbdIllumDown", 229),
    ("KbdIllumUp", 230),
    ("Scale", 120),
];

fn code_of(name: &str) -> Option<u16> {
    Key::named(name).map(Key::code)
}

#[test]
fn f5_left_ctrl_and_brightness_up_carry_their_kernel_codes() {
    assert_eq!(code_of("F5"), Some(63));
    assert_eq!(code_of("LeftCtrl"), Some(29));
    assert_eq!(code_of("BrightnessUp"), Some(225));
}

#[test]
fn every_name_of_the_vocabulary_carries_its_kernel_code() {
    for (name, code) in VOCABULARY {
        assert_eq!(code_of(name), Some(code), "key `{name}`");
    }
}

#[test]
fn the_known_keys_are_exactly_the_vocabulary() {
    let known: BTreeSet<&str> = Key::all().map(Key::name).collect();
    let vocabulary: BTreeSet<&str> = VOCABULARY.iter().map(|(name, _)| *name).collect();
    assert_eq!(known, vocabulary);
    assert_eq!(Key::all().count(), 111, "no key is listed twice");
}

#[test]
fn every_known_key_is_found_again_by_its_name() {
    for key in Key::all() {
        assert_eq!(Key::named(key.name()), Some(key), "{key:?}");
    }
}

#[test]
fn a_name_in_another_spelling_is_unknown() {
    for name in ["f5", "KEY_F5", "F55", "Ctrl", "leftctrl", "LEFTCTRL", "Escape", " F5", "F5 ", "Fn", ""] {
        assert_eq!(Key::named(name), None, "`{name}` must be unknown");
    }
}

#[test]
fn every_key_name_in_the_default_preset_is_known() {
    let layout = presets::default_layout();
    let mut checked = 0;
    for button in layout.layers.iter().flat_map(tb_core::Layer::buttons) {
        if let Action::Key { keys } = &button.action {
            for name in keys {
                assert!(Key::named(name).is_some(), "button `{}` presses unknown key `{name}`", button.id);
                checked += 1;
            }
        }
    }
    assert!(checked >= 13, "the default layer alone presses esc and F1-F12, checked {checked}");
}
