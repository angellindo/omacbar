//! The layout a user edits: layers of grouped buttons.

use serde::{Deserialize, Serialize};

use crate::chord::Chord;

/// Stable, user-facing button identifier (kebab-case, unique within a layer).
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(transparent)]
pub struct ButtonId(pub String);

impl ButtonId {
    pub fn new(id: impl Into<String>) -> Self {
        Self(id.into())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// Lowercase ASCII letters, digits and single dashes; not empty.
    pub fn is_valid(&self) -> bool {
        let s = self.0.as_str();
        !s.is_empty()
            && !s.starts_with('-')
            && !s.ends_with('-')
            && !s.contains("--")
            && s.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
    }
}

impl std::fmt::Display for ButtonId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

/// How wide a button is. `Span` shares the free space; `Px` is fixed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Width {
    /// Same as `Span(1)`.
    #[default]
    Auto,
    /// Takes `n` shares of the space left after fixed buttons and gaps.
    Span(u8),
    /// Exactly this many device pixels.
    Px(u16),
}

impl Width {
    /// Shares of free space this button takes (0 for fixed widths).
    pub const fn shares(self) -> u32 {
        match self {
            Width::Auto => 1,
            Width::Span(n) => n as u32,
            Width::Px(_) => 0,
        }
    }
}

/// Optional per-button colour overrides. Values are theme token names (`accent`, `red`) or `#rrggbb`.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct ButtonStyle {
    pub fill: Option<String>,
    pub text: Option<String>,
    pub border: Option<String>,
}

/// What a tap does.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "kebab-case", deny_unknown_fields)]
pub enum Action {
    /// Press and release a key or chord, e.g. `["LeftCtrl", "F5"]` (evdev key names).
    Key { keys: Vec<String> },
    /// Run a command line through `sh -c`.
    Command { run: String },
    /// Decorative or not yet wired.
    #[default]
    None,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Button {
    pub id: ButtonId,
    pub label: String,
    #[serde(default)]
    pub width: Width,
    #[serde(default)]
    pub style: ButtonStyle,
    #[serde(default)]
    pub action: Action,
}

impl Button {
    /// A span-1 button with no style override.
    pub fn new(id: &str, label: &str, action: Action) -> Self {
        Self {
            id: ButtonId::new(id),
            label: label.to_owned(),
            width: Width::Auto,
            style: ButtonStyle::default(),
            action,
        }
    }

    #[must_use]
    pub fn with_width(mut self, width: Width) -> Self {
        self.width = width;
        self
    }
}

/// Buttons laid out together; groups are separated by the wider group gap.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Group {
    #[serde(default, rename = "button")]
    pub buttons: Vec<Button>,
}

/// Everything the bar shows while one chord is held.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Layer {
    pub chord: Chord,
    #[serde(default, rename = "group")]
    pub groups: Vec<Group>,
}

impl Layer {
    pub fn buttons(&self) -> impl Iterator<Item = &Button> {
        self.groups.iter().flat_map(|g| g.buttons.iter())
    }
}

/// The full user layout: one layer per chord.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Layout {
    #[serde(default, rename = "layer")]
    pub layers: Vec<Layer>,
}

impl Layout {
    pub fn layer(&self, chord: Chord) -> Option<&Layer> {
        self.layers.iter().find(|l| l.chord == chord)
    }

    pub fn layer_mut(&mut self, chord: Chord) -> Option<&mut Layer> {
        self.layers.iter_mut().find(|l| l.chord == chord)
    }

    pub fn chords(&self) -> Vec<Chord> {
        self.layers.iter().map(|l| l.chord).collect()
    }
}
