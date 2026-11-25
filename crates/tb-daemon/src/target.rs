//! Opened node identity and sysfs attributes shared by the device adapters.

use std::ffi::OsStr;
use std::fs::File;
use std::io::{self, Read};
use std::os::fd::{AsFd, OwnedFd};
use std::os::unix::ffi::OsStrExt;
use std::path::{Path, PathBuf};

use rustix::fs::{self, AtFlags, CWD, FileType, Mode, OFlags};
use thiserror::Error;

/// The upstream DRM driver of the Touch Bar panel. That the patched T1 driver keeps this name is
/// unverified on hardware (TB-10); a mismatch is refused.
pub(crate) const DRM_DRIVER: &str = "appletbdrm";
/// USB vendor and product of the T1 `iBridge` display interface (research 0001).
pub(crate) const T1_VENDOR: &str = "05ac";
pub(crate) const T1_PRODUCT: &str = "8600";

/// The identity of an opened node, as `fstat` reports it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct NodeId {
    pub major: u32,
    pub minor: u32,
    /// `S_ISCHR`: both DRM and evdev nodes are character devices.
    pub char_device: bool,
}

impl NodeId {
    pub(crate) fn of(file: &File) -> io::Result<Self> {
        let stat = rustix::fs::fstat(file)?;
        let char_device = FileType::from_raw_mode(stat.st_mode) == FileType::CharacterDevice;
        let dev = stat.st_rdev;
        Ok(Self { major: rustix::fs::major(dev), minor: rustix::fs::minor(dev), char_device })
    }
}

#[derive(Debug, Error)]
pub enum TargetError {
    #[error("no matching T1 Touch Bar device found")]
    NotFound,
    #[error("more than one T1 Touch Bar device matched, choose one explicitly: {0:?}")]
    Ambiguous(Vec<PathBuf>),
    #[error("{} is not a /dev/dri/cardN path", path.display())]
    NotACardPath { path: PathBuf },
    #[error("{} is not a /dev/input/eventN path", path.display())]
    NotAnEventPath { path: PathBuf },
    #[error("{} is not a T1 digitizer", path.display())]
    NotADigitizer { path: PathBuf },
    #[error("the opened node {found:?} does not match {} ({expected}) in sysfs", path.display())]
    DigitizerChanged { path: PathBuf, expected: String, found: NodeId },
    #[error("{card} has no driver bound")]
    Unbound { card: String },
    #[error("the card's driver is {driver:?}, not {DRM_DRIVER}")]
    WrongDriver { driver: String },
    #[error("the card's USB device is {vendor}:{product}, not the T1 Touch Bar {T1_VENDOR}:{T1_PRODUCT}")]
    WrongDevice { vendor: String, product: String },
    #[error("the opened node {found:?} is not {card} ({expected}) in sysfs")]
    Changed { card: String, expected: String, found: NodeId },
    #[error("cannot read {}: {source}", path.display())]
    Io { path: PathBuf, source: io::Error },
}

/// Pins a sysfs directory so later lookups cannot follow a replacement at its original path.
pub(crate) struct SysfsDir {
    fd: OwnedFd,
    path: PathBuf,
}

impl SysfsDir {
    pub(crate) fn open(path: &Path) -> Result<Self, TargetError> {
        Self::open_at(CWD, path, path.to_owned())
    }

    pub(crate) fn child(&self, name: &str) -> Result<Self, TargetError> {
        Self::open_at(&self.fd, Path::new(name), self.path.join(name))
    }

    fn open_at(base: impl AsFd, name: &Path, path: PathBuf) -> Result<Self, TargetError> {
        match fs::openat(base, name, OFlags::PATH | OFlags::DIRECTORY | OFlags::CLOEXEC, Mode::empty()) {
            Ok(fd) => Ok(Self { fd, path }),
            Err(source) => Err(TargetError::Io { path, source: source.into() }),
        }
    }

    pub(crate) fn read(&self, name: &str) -> Result<String, TargetError> {
        let at = |source| TargetError::Io { path: self.path.join(name), source };
        let flags = OFlags::RDONLY | OFlags::CLOEXEC;
        let fd = fs::openat(&self.fd, name, flags, Mode::empty()).map_err(|e| at(e.into()))?;
        let mut text = String::new();
        File::from(fd).take(65).read_to_string(&mut text).map_err(at)?;
        if text.len() > 64 {
            return Err(at(io::Error::new(
                io::ErrorKind::InvalidData,
                "sysfs attribute longer than 64 bytes",
            )));
        }
        Ok(text.trim().to_owned())
    }

    pub(crate) fn link(&self, name: &str) -> Result<PathBuf, TargetError> {
        let link = fs::readlinkat(&self.fd, name, Vec::new())
            .map_err(|source| TargetError::Io { path: self.path.join(name), source: source.into() })?;
        Ok(PathBuf::from(OsStr::from_bytes(link.to_bytes())))
    }

    pub(crate) fn read_usb_id(&self, name: &str) -> Result<String, TargetError> {
        let id = self.read(name)?;
        if id.len() != 4 || !id.bytes().all(|byte| byte.is_ascii_hexdigit()) {
            return Err(TargetError::Io {
                path: self.path.join(name),
                source: io::Error::new(
                    io::ErrorKind::InvalidData,
                    "expected a four-digit hexadecimal USB ID",
                ),
            });
        }
        Ok(id)
    }

    pub(crate) fn read_hex(&self, name: &str) -> Result<u16, TargetError> {
        u16::from_str_radix(&self.read(name)?, 16).map_err(|source| TargetError::Io {
            path: self.path.join(name),
            source: io::Error::new(io::ErrorKind::InvalidData, source),
        })
    }

    /// Reject a removed or renumbered entry even if the old directory is still readable.
    pub(crate) fn confirm_current(&self) -> Result<(), TargetError> {
        let at = |source| TargetError::Io { path: self.path.clone(), source };
        let pinned = fs::fstat(&self.fd).map_err(|e| at(e.into()))?;
        let named = fs::statat(CWD, &self.path, AtFlags::empty()).map_err(|e| at(e.into()))?;
        if (pinned.st_dev, pinned.st_ino) != (named.st_dev, named.st_ino) {
            return Err(at(io::Error::new(
                io::ErrorKind::NotFound,
                "sysfs entry changed during confirmation",
            )));
        }
        Ok(())
    }
}

/// `major:minor` as sysfs writes a `dev` attribute.
pub(crate) fn parse_dev(text: &str) -> Option<(u32, u32)> {
    let (major, minor) = text.split_once(':')?;
    Some((major.parse().ok()?, minor.parse().ok()?))
}
