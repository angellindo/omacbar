//! Finding the Fn keyboard by its `KEY_FN` capability, never by event number (research 0002,
//! ADR-0009).

use std::path::PathBuf;

use crate::evdev::{has_bit, is_event_name};

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
    if !has_bit(field("B: KEY=")?, KEY_FN) {
        return None;
    }
    let event = field("H: Handlers=")?.split_whitespace().find(|h| is_event_name(h))?;
    let name = field("N: Name=")?.trim().trim_matches('"').to_owned();
    Some(FnKeyboard { name, path: PathBuf::from("/dev/input").join(event) })
}
