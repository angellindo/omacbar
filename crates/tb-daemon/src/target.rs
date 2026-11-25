//! Opened node identity and sysfs attributes shared by the device adapters.

use std::fs::File;
use std::io;
use std::path::{Path, PathBuf};

use rustix::fs::FileType;
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

/// A short sysfs attribute, trimmed. Attributes are a few bytes; more than 64 is refused.
pub(crate) fn read_trimmed(path: &Path) -> Result<String, TargetError> {
    let io_err = |source| TargetError::Io { path: path.to_owned(), source };
    let text = std::fs::read_to_string(path).map_err(io_err)?;
    if text.len() > 64 {
        return Err(io_err(io::Error::new(
            io::ErrorKind::InvalidData,
            "sysfs attribute longer than 64 bytes",
        )));
    }
    Ok(text.trim().to_owned())
}

/// `major:minor` as sysfs writes a `dev` attribute.
pub(crate) fn parse_dev(text: &str) -> Option<(u32, u32)> {
    let (major, minor) = text.split_once(':')?;
    Some((major.parse().ok()?, minor.parse().ok()?))
}
