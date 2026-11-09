//! The keys a `key` action can press: a fixed vocabulary of Linux key names and codes.

use std::fmt;

/// One key a `key` action presses, named by its Linux `KEY_*` name without `KEY_` (`F5`,
/// `LeftCtrl`). Names are case-sensitive and have no aliases; `Fn` is not a key (it picks layers).
#[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Key(u16);

/// Every known key: its name, then its code from `linux/input-event-codes.h`.
const KEYS: [(&str, u16); 111] = [
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

impl Key {
    /// The key spelled exactly `name`, or `None` for a name outside the vocabulary.
    pub fn named(name: &str) -> Option<Key> {
        KEYS.iter().find(|(n, _)| *n == name).map(|&(_, code)| Key(code))
    }

    /// The key's name, as [`Key::named`] reads it.
    pub fn name(self) -> &'static str {
        KEYS.iter().find(|(_, code)| *code == self.0).map_or("", |(name, _)| name)
    }

    /// The kernel key code (`KEY_*` in `linux/input-event-codes.h`).
    pub const fn code(self) -> u16 {
        self.0
    }

    /// Every key of the vocabulary, each once.
    pub fn all() -> impl Iterator<Item = Key> {
        KEYS.iter().map(|&(_, code)| Key(code))
    }
}

impl fmt::Debug for Key {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Key({})", self.name())
    }
}
