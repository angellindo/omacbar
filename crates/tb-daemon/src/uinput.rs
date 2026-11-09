//! The virtual keyboard: key presses written as evdev records to uinput or to a file (ADR-0012).
//! Creating the uinput device takes four ioctls; this is the only module allowed `unsafe`.
#![allow(unsafe_code, reason = "the uinput setup ioctls, ADR-0012")]

use std::ffi::c_int;
use std::fs::{File, OpenOptions};
use std::io::{self, Write};
use std::path::{Path, PathBuf};

use rustix::ioctl::{IntegerSetter, NoArg, Opcode, Setter, ioctl, opcode};
use tb_core::Key;
use thiserror::Error;

use crate::action::KeyEmitter;

/// One `x86_64` `struct input_event`: 16 bytes of time, `u16` type, `u16` code, `i32` value.
const RECORD: usize = 24;
const EV_SYN: u16 = 0;
const EV_KEY: u16 = 1;
const SYN_REPORT: u16 = 0;

/// The uinput device node (`misc/uinput.c`).
const UINPUT: &str = "/dev/uinput";
/// The device's name, as `hyprctl devices` lists it.
const NAME: &[u8] = b"touchbard";
/// `BUS_VIRTUAL` in `linux/input.h`.
const BUS_VIRTUAL: u16 = 0x06;
/// `UINPUT_MAX_NAME_SIZE` in `linux/uinput.h`.
const MAX_NAME: usize = 80;

/// The setup ioctls of `linux/uinput.h` (`UINPUT_IOCTL_BASE` is `'U'`).
const UI_DEV_CREATE: Opcode = opcode::none(b'U', 1);
const UI_DEV_SETUP: Opcode = opcode::write::<UinputSetup>(b'U', 3);
const UI_SET_EVBIT: Opcode = opcode::write::<c_int>(b'U', 100);
const UI_SET_KEYBIT: Opcode = opcode::write::<c_int>(b'U', 101);

/// `struct input_id` in `linux/input.h`.
#[repr(C)]
struct InputId {
    bustype: u16,
    vendor: u16,
    product: u16,
    version: u16,
}

/// `struct uinput_setup` in `linux/uinput.h`.
#[repr(C)]
struct UinputSetup {
    id: InputId,
    name: [u8; MAX_NAME],
    ff_effects_max: u32,
}

#[derive(Debug, Error)]
pub enum EmitError {
    #[error("cannot open {}: {source}", path.display())]
    Open { path: PathBuf, source: io::Error },
    #[error("cannot set up the virtual keyboard on {UINPUT}: {0}")]
    Setup(#[source] io::Error),
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

    /// Creates the uinput keyboard `touchbard`, able to press every [`Key`] (`--emit uinput`).
    /// Closing the file destroys the device and releases its keys.
    pub fn create() -> Result<Self, EmitError> {
        let path = Path::new(UINPUT);
        let file = OpenOptions::new()
            .write(true)
            .open(path)
            .map_err(|source| EmitError::Open { path: path.to_owned(), source })?;
        set_up(&file).map_err(|e| EmitError::Setup(e.into()))?;
        Ok(Self::new(file))
    }
}

/// `UI_SET_EVBIT`, `UI_SET_KEYBIT` per key, `UI_DEV_SETUP`, `UI_DEV_CREATE` on an open `/dev/uinput`.
fn set_up(file: &File) -> rustix::io::Result<()> {
    // SAFETY: UI_SET_EVBIT is `_IOW('U', 100, int)` and the kernel reads the argument as the
    // integer itself (EV_KEY, in range), not as a pointer, so nothing has to outlive the call.
    unsafe { ioctl(file, IntegerSetter::<UI_SET_EVBIT>::new_usize(usize::from(EV_KEY))) }?;
    for key in Key::all() {
        // SAFETY: UI_SET_KEYBIT is `_IOW('U', 101, int)`, the argument is the key code itself
        // (below KEY_MAX), passed by value, so nothing has to outlive the call.
        unsafe { ioctl(file, IntegerSetter::<UI_SET_KEYBIT>::new_usize(usize::from(key.code()))) }?;
    }
    let mut name = [0; MAX_NAME];
    name[..NAME.len()].copy_from_slice(NAME);
    let setup = UinputSetup {
        id: InputId { bustype: BUS_VIRTUAL, vendor: 0, product: 0, version: 1 },
        name,
        ff_effects_max: 0,
    };
    // SAFETY: UI_DEV_SETUP is `_IOW('U', 3, struct uinput_setup)`; `UinputSetup` is that struct
    // with `repr(C)`, its name NUL-terminated. `Setter` owns it and passes a pointer to it that
    // lives until `ioctl` returns; the kernel only reads it.
    unsafe { ioctl(file, Setter::<UI_DEV_SETUP, UinputSetup>::new(setup)) }?;
    // SAFETY: UI_DEV_CREATE is `_IO('U', 1)` and takes no argument.
    unsafe { ioctl(file, NoArg::<UI_DEV_CREATE>::new()) }
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
