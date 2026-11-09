//! The virtual keyboard: key presses written as evdev records to uinput or to a file (ADR-0012).

use std::fs::File;
use std::io::{self, Write};
use std::path::{Path, PathBuf};

use tb_core::Key;
use thiserror::Error;

use crate::action::KeyEmitter;

/// One `x86_64` `struct input_event`: 16 bytes of time, `u16` type, `u16` code, `i32` value.
const RECORD: usize = 24;
const EV_SYN: u16 = 0;
const EV_KEY: u16 = 1;
const SYN_REPORT: u16 = 0;

#[derive(Debug, Error)]
pub enum EmitError {
    #[error("cannot open {}: {source}", path.display())]
    Open { path: PathBuf, source: io::Error },
    #[error("cannot write to the virtual keyboard: {0}")]
    Write(#[source] io::Error),
}

/// Writes key presses as `input_event` records to `W`: the uinput device, a file, or memory.
#[derive(Debug)]
pub struct VirtualKeyboard<W: Write> {
    out: W,
}

impl<W: Write> VirtualKeyboard<W> {
    pub fn new(out: W) -> Self {
        Self { out }
    }
}

impl VirtualKeyboard<File> {
    /// Writes the records to `path`, created or truncated (`--emit PATH`); no directory is made.
    pub fn record_to(path: &Path) -> Result<Self, EmitError> {
        File::create(path).map(Self::new).map_err(|source| EmitError::Open { path: path.to_owned(), source })
    }
}

impl<W: Write> KeyEmitter for VirtualKeyboard<W> {
    /// Each key pressed in order, then released in reverse, a `SYN_REPORT` after each, all in one
    /// write: no key stays down between taps.
    fn emit(&mut self, keys: &[Key]) -> Result<(), EmitError> {
        let mut records = Vec::with_capacity(keys.len() * 4 * RECORD);
        let presses = keys.iter().map(|k| (k, 1));
        let releases = keys.iter().rev().map(|k| (k, 0));
        for (key, value) in presses.chain(releases) {
            push_record(&mut records, EV_KEY, key.code(), value);
            push_record(&mut records, EV_SYN, SYN_REPORT, 0);
        }
        self.out.write_all(&records).and_then(|()| self.out.flush()).map_err(EmitError::Write)
    }
}

/// Appends one record with zero time.
fn push_record(records: &mut Vec<u8>, kind: u16, code: u16, value: i32) {
    records.extend_from_slice(&[0; 16]);
    records.extend_from_slice(&kind.to_ne_bytes());
    records.extend_from_slice(&code.to_ne_bytes());
    records.extend_from_slice(&value.to_ne_bytes());
}
