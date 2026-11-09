//! Finding the Fn keyboard by its `KEY_FN` capability, never by event number (research 0002,
//! ADR-0009).

use std::path::PathBuf;

/// `KEY_FN` in `linux/input-event-codes.h`.
const KEY_FN: usize = 464;

/// The input device whose held Fn and modifier keys pick the layer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FnKeyboard {
    /// The device name, as `N: Name=` gives it.
    pub name: String,
    /// Its event device, `/dev/input/eventN`.
    pub path: PathBuf,
}

/// The first device in `devices` (the text of `/proc/bus/input/devices`) whose key capability
/// bitmap has `KEY_FN` and that has an `eventN` handler. Malformed devices are skipped.
pub fn find_fn_keyboard(devices: &str) -> Option<FnKeyboard> {
    let mut block = Vec::new();
    for line in devices.lines().chain([""]) {
        if line.trim().is_empty() {
            if let Some(kb) = fn_keyboard(&block) {
                return Some(kb);
            }
            block.clear();
        } else {
            block.push(line);
        }
    }
    None
}

/// The device described by the lines of one block, if it is a Fn keyboard with an event handler.
fn fn_keyboard(block: &[&str]) -> Option<FnKeyboard> {
    let field = |prefix: &str| block.iter().find_map(|line| line.strip_prefix(prefix));
    if !has_key(field("B: KEY=")?, KEY_FN) {
        return None;
    }
    let event = field("H: Handlers=")?.split_whitespace().find(|h| {
        h.strip_prefix("event").is_some_and(|n| !n.is_empty() && n.bytes().all(|b| b.is_ascii_digit()))
    })?;
    let name = field("N: Name=")?.trim().trim_matches('"').to_owned();
    Some(FnKeyboard { name, path: PathBuf::from("/dev/input").join(event) })
}

/// Whether bit `key` is set in a capability bitmap: 64-bit hex words, most significant first,
/// leading zero words left out. A malformed bitmap has no keys.
fn has_key(bitmap: &str, key: usize) -> bool {
    let words: Option<Vec<u64>> = bitmap
        .split_whitespace()
        .map(|w| if w.len() <= 16 { u64::from_str_radix(w, 16).ok() } else { None })
        .collect();
    let Some(words) = words else { return false };
    words.iter().rev().nth(key / 64).is_some_and(|word| word >> (key % 64) & 1 == 1)
}
