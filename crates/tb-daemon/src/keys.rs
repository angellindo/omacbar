//! The modifier source: which Fn and modifier keys are held on the Fn keyboard (ADR-0009).

use std::fs::File;
use std::io::{ErrorKind, Read};
use std::path::{Path, PathBuf};
use std::sync::mpsc::Sender;

use tb_core::Modifiers;
use thiserror::Error;

use crate::daemon::Event;

/// Where the daemon learns the held modifiers: the Fn keyboard, or a script in tests.
pub trait ModifierSource {
    /// Blocks until the held modifiers change and returns them; Ok(None) once the source closed.
    fn next_modifiers(&mut self) -> Result<Option<Modifiers>, KeyError>;
}

#[derive(Debug, Error)]
pub enum KeyError {
    #[error("cannot open the keyboard {}: {source}", path.display())]
    Open { path: PathBuf, source: std::io::Error },
    #[error("cannot read the keyboard: {source}")]
    Io {
        #[from]
        source: std::io::Error,
    },
}

/// One `x86_64` `struct input_event`: 16 bytes of time, `u16` type, `u16` code, `i32` value.
const RECORD: usize = 24;
const EV_SYN: u16 = 0;
const EV_KEY: u16 = 1;
const SYN_REPORT: u16 = 0;

/// The modifier keys (`linux/input-event-codes.h`): `KEY_FN`, then the left and right key of
/// Ctrl, Alt, Shift and Meta (super).
const MODIFIER_KEYS: [u16; 9] = [464, 29, 97, 56, 100, 42, 54, 125, 126];

/// Held modifiers from a stream of evdev records. A change is reported only at `SYN_REPORT`, so
/// keys pressed in one keyboard report are one change. Key value 0 is up, 1 (press) and 2
/// (autorepeat) are down: a key already held when reading starts is picked up at its first repeat.
#[derive(Debug)]
pub struct EvdevModifiers<R: Read> {
    reader: R,
    /// Whether each key of `MODIFIER_KEYS` is down, as of the records read so far.
    down: [bool; MODIFIER_KEYS.len()],
    reported: Modifiers,
}

impl<R: Read> EvdevModifiers<R> {
    pub fn new(reader: R) -> Self {
        Self { reader, down: [false; MODIFIER_KEYS.len()], reported: Modifiers::default() }
    }

    /// A flag is held while either key of its pair is down.
    fn held(&self) -> Modifiers {
        let d = self.down;
        Modifiers {
            fn_key: d[0],
            ctrl: d[1] || d[2],
            alt: d[3] || d[4],
            shift: d[5] || d[6],
            super_key: d[7] || d[8],
        }
    }

    /// The next whole record; Ok(None) at the end of the stream on a record boundary.
    fn next_record(&mut self) -> Result<Option<[u8; RECORD]>, KeyError> {
        let mut record = [0; RECORD];
        let mut filled = 0;
        while filled < RECORD {
            match self.reader.read(&mut record[filled..]) {
                Ok(0) if filled == 0 => return Ok(None),
                Ok(0) => return Err(std::io::Error::from(ErrorKind::UnexpectedEof).into()),
                Ok(n) => filled += n,
                Err(e) if e.kind() == ErrorKind::Interrupted => {}
                Err(e) => return Err(e.into()),
            }
        }
        Ok(Some(record))
    }
}

impl EvdevModifiers<File> {
    /// Reads the keyboard device at `path` (an `/dev/input/eventN`).
    pub fn open(path: &Path) -> Result<Self, KeyError> {
        File::open(path).map(Self::new).map_err(|source| KeyError::Open { path: path.to_path_buf(), source })
    }
}

impl<R: Read> ModifierSource for EvdevModifiers<R> {
    fn next_modifiers(&mut self) -> Result<Option<Modifiers>, KeyError> {
        while let Some(r) = self.next_record()? {
            let kind = u16::from_ne_bytes([r[16], r[17]]);
            let code = u16::from_ne_bytes([r[18], r[19]]);
            let value = i32::from_ne_bytes([r[20], r[21], r[22], r[23]]);
            match (kind, code) {
                (EV_KEY, _) => {
                    let Some(i) = MODIFIER_KEYS.iter().position(|c| *c == code) else { continue };
                    match value {
                        0 => self.down[i] = false,
                        1 | 2 => self.down[i] = true,
                        _ => {}
                    }
                }
                (EV_SYN, SYN_REPORT) => {
                    let held = self.held();
                    if held != self.reported {
                        self.reported = held;
                        return Ok(Some(held));
                    }
                }
                _ => {}
            }
        }
        Ok(None)
    }
}

/// Sends `Event::Modifiers` for each change until the source closes (Ok) or fails (that error).
/// Then, unless the default modifiers were the last sent, sends them once: held keys are unknown
/// once the keyboard is gone, so the bar goes back to the default layer. A dropped receiver ends
/// it with Ok: nobody is left to show the layer.
pub fn forward_modifiers<T: From<Event>>(
    source: &mut impl ModifierSource,
    events: &Sender<T>,
) -> Result<(), KeyError> {
    let mut last = Modifiers::default();
    let ended = loop {
        match source.next_modifiers() {
            Ok(Some(held)) => {
                if events.send(Event::Modifiers(held).into()).is_err() {
                    return Ok(());
                }
                last = held;
            }
            Ok(None) => break Ok(()),
            Err(e) => break Err(e),
        }
    };
    if last != Modifiers::default() {
        // A dropped receiver: the daemon already stopped, nothing to return to.
        let _ = events.send(Event::Modifiers(Modifiers::default()).into());
    }
    ended
}
