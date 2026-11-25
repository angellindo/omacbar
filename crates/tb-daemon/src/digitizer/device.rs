//! Linux evdev ioctls and the lifetime of the exclusive grab (ADR-0014).
#![allow(unsafe_code, reason = "The evdev ioctl boundary in ADR-0014")]

use std::ffi::CStr;
use std::fs::File;
use std::io;
use std::path::Path;

use rustix::fs::{Mode, OFlags};
use rustix::ioctl::{IntegerSetter, Opcode, Updater, ioctl, opcode};

use super::{AbsAxis, DigitizerMeta, MT_SLOT, MT_TRACKING_ID, MT_X, MT_Y};
use crate::target::NodeId;

// linux/input.h; the ABS bitmap occupies one native unsigned long on x86_64.
const GROUP: u8 = b'E';
const GET_ID: Opcode = opcode::read::<InputId>(GROUP, 0x02);
const GET_NAME: Opcode = opcode::read::<[u8; 256]>(GROUP, 0x06);
const GET_ABS_BITS: Opcode = opcode::read::<u64>(GROUP, 0x23);
const GRAB: Opcode = opcode::write::<i32>(GROUP, 0x90);

const fn abs_opcode(code: u16) -> Opcode {
    let [code, high] = code.to_le_bytes();
    assert!(high == 0 && code <= 0x3f);
    opcode::read::<InputAbsInfo>(GROUP, 0x40 + code)
}

#[repr(C)]
#[derive(Default)]
struct InputId {
    bus: u16,
    vendor: u16,
    product: u16,
    version: u16,
}

#[repr(C)]
#[derive(Default)]
struct InputAbsInfo {
    value: i32,
    min: i32,
    max: i32,
    fuzz: i32,
    flat: i32,
    resolution: i32,
}

const _: () = assert!(size_of::<InputId>() == 8);
const _: () = assert!(size_of::<InputAbsInfo>() == 24);

/// An opened evdev node. Closing the file releases its grab if one was taken.
#[derive(Debug)]
pub struct DigitizerNode {
    file: File,
}

impl DigitizerNode {
    /// Opens read-only, close-on-exec, without following a final symlink or creating a file.
    pub fn open(path: &Path) -> io::Result<Self> {
        let flags = OFlags::RDONLY | OFlags::CLOEXEC | OFlags::NOFOLLOW;
        let fd = rustix::fs::open(path, flags, Mode::empty())?;
        Ok(Self { file: File::from(fd) })
    }

    /// Identifies the actual file descriptor, even if the path has been replaced.
    pub fn node_id(&self) -> io::Result<NodeId> {
        NodeId::of(&self.file)
    }

    pub fn metadata(&self) -> io::Result<DigitizerMeta> {
        let mut id = InputId::default();
        // SAFETY: EVIOCGID writes at most one InputId, whose layout is asserted above.
        unsafe { ioctl(&self.file, Updater::<GET_ID, _>::new(&mut id)) }?;
        let mut name = [0_u8; 256];
        // SAFETY: EVIOCGNAME's opcode encodes the length of this initialized buffer.
        unsafe { ioctl(&self.file, Updater::<GET_NAME, _>::new(&mut name)) }?;
        let name = CStr::from_bytes_until_nul(&name)
            .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?
            .to_str()
            .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?
            .to_owned();

        let mut bits = 0_u64;
        // SAFETY: EVIOCGBIT writes at most the encoded length into this initialized bitmap.
        unsafe { ioctl(&self.file, Updater::<GET_ABS_BITS, _>::new(&mut bits)) }?;
        // EVIOCGABS can read an unused index when the device has an absinfo array.
        let axes = [MT_SLOT, MT_TRACKING_ID, MT_X, MT_Y]
            .into_iter()
            .filter(|code| bits & (1 << code) != 0)
            .map(|code| self.axis(code))
            .collect::<io::Result<_>>()?;
        Ok(DigitizerMeta { bus: id.bus, vendor: id.vendor, product: id.product, name, axes })
    }

    fn axis(&self, code: u16) -> io::Result<AbsAxis> {
        let info = match code {
            MT_SLOT => self.read_axis::<{ abs_opcode(MT_SLOT) }>(),
            MT_TRACKING_ID => self.read_axis::<{ abs_opcode(MT_TRACKING_ID) }>(),
            MT_X => self.read_axis::<{ abs_opcode(MT_X) }>(),
            MT_Y => self.read_axis::<{ abs_opcode(MT_Y) }>(),
            _ => {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidInput,
                    format!("unknown MT axis {code:#x}"),
                ));
            }
        }?;
        Ok(AbsAxis { code, min: info.min, max: info.max })
    }

    fn read_axis<const OP: Opcode>(&self) -> io::Result<InputAbsInfo> {
        let mut info = InputAbsInfo::default();
        // SAFETY: the only callers supply EVIOCGABS opcodes encoding one InputAbsInfo.
        unsafe { ioctl(&self.file, Updater::<OP, _>::new(&mut info)) }?;
        Ok(info)
    }

    pub(super) fn grab(self) -> io::Result<File> {
        // SAFETY: evdev treats the third argument to EVIOCGRAB as a boolean value, not a pointer
        // to read. The value 1 requests a grab (Linux v6.15, drivers/input/evdev.c).
        unsafe { ioctl(&self.file, IntegerSetter::<GRAB>::new_usize(1)) }?;
        Ok(self.file)
    }
}
