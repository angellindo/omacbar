//! The binary's only output: messages on stderr (R11).

use std::fmt::Display;

/// Writes `message` to stderr, prefixed with the binary's name.
#[expect(clippy::print_stderr, reason = "the daemon's single log sink")]
pub fn error(message: impl Display) {
    eprintln!("touchbard: {message}");
}

/// Writes a notice that is not an error (which keyboard is followed) to the same sink.
#[expect(clippy::print_stderr, reason = "the daemon's single log sink")]
pub fn info(message: impl Display) {
    eprintln!("touchbard: {message}");
}
