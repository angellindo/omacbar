//! The binary's only output: messages on stderr (R11).

use std::fmt::Display;

/// Writes `message` to stderr, prefixed with the binary's name.
#[expect(clippy::print_stderr, reason = "the daemon's single log sink")]
pub fn error(message: impl Display) {
    eprintln!("touchbard: {message}");
}
