//! Which layer is showing: resolved from the held Fn and modifier keys.

use serde::{Deserialize, Serialize};

/// A layer selector. `Default` (nothing held) shows the F-keys.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Chord {
    Default,
    Fn,
    FnCtrl,
    FnAlt,
    FnShift,
    FnSuper,
}

impl Chord {
    pub const ALL: [Chord; 6] =
        [Chord::Default, Chord::Fn, Chord::FnCtrl, Chord::FnAlt, Chord::FnShift, Chord::FnSuper];

    /// Human label, e.g. for configurator tabs.
    pub const fn label(self) -> &'static str {
        match self {
            Chord::Default => "Default",
            Chord::Fn => "Fn",
            Chord::FnCtrl => "Fn + Ctrl",
            Chord::FnAlt => "Fn + Alt",
            Chord::FnShift => "Fn + Shift",
            Chord::FnSuper => "Fn + Super",
        }
    }

    /// The chord the held keys ask for, before checking which layers exist.
    pub fn requested(m: Modifiers) -> Chord {
        if !m.fn_key {
            return Chord::Default;
        }
        match (m.ctrl, m.alt, m.shift, m.super_key) {
            (true, false, false, false) => Chord::FnCtrl,
            (false, true, false, false) => Chord::FnAlt,
            (false, false, true, false) => Chord::FnShift,
            (false, false, false, true) => Chord::FnSuper,
            // Fn alone, or Fn with several modifiers (multi-modifier layers are out of MVP scope).
            _ => Chord::Fn,
        }
    }

    /// The layer to show: the requested one if configured, else `Fn`, else `Default`.
    pub fn resolve(m: Modifiers, configured: &[Chord]) -> Chord {
        [Chord::requested(m), Chord::Fn, Chord::Default]
            .into_iter()
            .find(|c| configured.contains(c))
            .unwrap_or(Chord::Default)
    }
}

/// Modifier keys currently held on the physical keyboard.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
#[allow(clippy::struct_excessive_bools)]
pub struct Modifiers {
    pub fn_key: bool,
    pub ctrl: bool,
    pub alt: bool,
    pub shift: bool,
    pub super_key: bool,
}

#[cfg(test)]
mod tests;
