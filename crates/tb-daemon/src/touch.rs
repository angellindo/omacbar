//! Touches on the bar. Only x matters: the bar is one row of buttons.

use std::io::BufRead;
use std::sync::mpsc::Sender;

use thiserror::Error;

use crate::daemon::Event;

/// One touch report, in bar pixels.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Touch {
    /// A finger is down at column `x`.
    Down { x: u16 },
    /// No finger on the bar.
    Up,
}

/// Where touches come from: a device, stdin, or a script in tests.
pub trait TouchSource {
    /// Blocks for the next touch; Ok(None) when the source has closed.
    fn next_touch(&mut self) -> Result<Option<Touch>, TouchError>;
}

#[derive(Debug, Error)]
pub enum TouchError {
    #[error("bad touch line `{line}`: expected `down <x>` (x in 0..=65535) or `up`")]
    BadLine { line: String },
    #[error("cannot read touches: {0}")]
    Io(#[from] std::io::Error),
}

/// Touches as text lines: `down <x>` or `up`; empty lines are skipped.
#[derive(Debug)]
pub struct LineTouches<R: BufRead> {
    reader: R,
}

impl<R: BufRead> LineTouches<R> {
    pub fn new(reader: R) -> Self {
        Self { reader }
    }
}

impl<R: BufRead> TouchSource for LineTouches<R> {
    fn next_touch(&mut self) -> Result<Option<Touch>, TouchError> {
        let mut line = String::new();
        loop {
            line.clear();
            if self.reader.read_line(&mut line)? == 0 {
                return Ok(None);
            }
            let words: Vec<&str> = line.split_whitespace().collect();
            let touch = match words.as_slice() {
                [] => continue,
                ["up"] => Some(Touch::Up),
                ["down", x] => x.parse().ok().map(|x| Touch::Down { x }),
                _ => None,
            };
            return touch.map(Some).ok_or_else(|| TouchError::BadLine { line: line.trim().to_owned() });
        }
    }
}

/// Sends `Event::Touch` for each touch until the source closes (Ok) or fails (that error).
/// A dropped receiver ends it with Ok: nobody is left to show the touches.
pub fn forward_touches(source: &mut impl TouchSource, events: &Sender<Event>) -> Result<(), TouchError> {
    while let Some(touch) = source.next_touch()? {
        if events.send(Event::Touch(touch)).is_err() {
            break;
        }
    }
    Ok(())
}
