//! Choosing the Touch Bar card from sysfs before any device node is opened, and confirming that an
//! opened node is still that card. Nothing here opens `/dev`; only sysfs text and links are read.
//!
//! Identity (research 0001, 0010; appletbdrm.c L817–820): the card's `device` is a USB interface
//! bound to the `appletbdrm` driver, and the USB device above that interface is the T1 `iBridge`
//! display `05ac:8600`. The driver name alone would also match a T2 (`05ac:8302`). Only these
//! attributes and the card's `dev` (major:minor) are read; serials never are.

use std::io;
use std::path::{Path, PathBuf};

use thiserror::Error;

/// The upstream DRM driver of the Touch Bar panel. That the patched T1 driver keeps this name is
/// unverified on hardware (TB-10); a mismatch is refused.
pub(crate) const DRIVER: &str = "appletbdrm";
/// USB vendor and product of the T1 `iBridge` display interface (research 0001).
const T1_VENDOR: &str = "05ac";
const T1_PRODUCT: &str = "8600";
/// Where userspace opens DRM cards; a target path must be exactly this plus a number.
const CARD_PREFIX: &str = "/dev/dri/card";
const DRI_DIR: &str = "/dev/dri";

/// The identity of an opened node, as `fstat` reports it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct NodeId {
    pub major: u32,
    pub minor: u32,
    /// `S_ISCHR`: a DRM card is a character device; an ordinary file or a directory is not.
    pub char_device: bool,
}

#[derive(Debug, Error)]
pub enum TargetError {
    #[error("no Touch Bar card (appletbdrm on USB 05ac:8600) in sysfs")]
    NotFound,
    #[error("more than one Touch Bar card, choose one explicitly: {0:?}")]
    Ambiguous(Vec<PathBuf>),
    #[error("{} is not a /dev/dri/cardN path", path.display())]
    NotACardPath { path: PathBuf },
    #[error("{card} has no driver bound")]
    Unbound { card: String },
    #[error("the card's driver is {driver:?}, not {DRIVER}")]
    WrongDriver { driver: String },
    #[error("the card's USB device is {vendor}:{product}, not the T1 Touch Bar {T1_VENDOR}:{T1_PRODUCT}")]
    WrongDevice { vendor: String, product: String },
    #[error("the opened node {found:?} is not {card} ({expected}) in sysfs")]
    Changed { card: String, expected: String, found: NodeId },
    #[error("cannot read {}: {source}", path.display())]
    Io { path: PathBuf, source: io::Error },
}

/// `auto`: the one Touch Bar card among `sysfs/class/drm/card*`. Connector (`cardN-*`) and render
/// (`renderD*`) entries are never considered; a card that is not the T1 bar is skipped without
/// opening anything. No match is `NotFound`, more than one is `Ambiguous` (never a guess).
pub fn find_touch_bar_card(sysfs: &Path) -> Result<PathBuf, TargetError> {
    let class = sysfs.join("class/drm");
    let entries =
        std::fs::read_dir(&class).map_err(|source| TargetError::Io { path: class.clone(), source })?;
    let mut found = Vec::new();
    for entry in entries {
        let entry = entry.map_err(|source| TargetError::Io { path: class.clone(), source })?;
        let Some(name) = entry.file_name().to_str().map(str::to_owned) else { continue };
        if is_card_name(&name) && identify(sysfs, &name).is_ok() {
            found.push(Path::new(DRI_DIR).join(&name));
        }
    }
    found.sort();
    match found.len() {
        0 => Err(TargetError::NotFound),
        1 => Ok(found.remove(0)),
        _ => Err(TargetError::Ambiguous(found)),
    }
}

/// An explicit `card`: it must be lexically `/dev/dri/card<digits>` (checked before sysfs is read)
/// and sysfs must name it the Touch Bar card. Returns `card` unchanged.
pub fn check_touch_bar_card(sysfs: &Path, card: &Path) -> Result<PathBuf, TargetError> {
    let name = card_name(card)?;
    identify(sysfs, name)?;
    Ok(card.to_owned())
}

/// After opening `card`: re-reads sysfs (driver, USB ids, `dev`) and checks that `node` is the
/// character device sysfs names for it. A card renumbered or replaced since discovery is refused.
pub fn confirm_touch_bar_node(sysfs: &Path, card: &Path, node: NodeId) -> Result<PathBuf, TargetError> {
    let name = card_name(card)?;
    identify(sysfs, name)?;
    let dev_path = sysfs.join("class/drm").join(name).join("dev");
    let expected = read_trimmed(&dev_path)?;
    let matches = parse_dev(&expected) == Some((node.major, node.minor));
    if !(node.char_device && matches) {
        return Err(TargetError::Changed { card: name.to_owned(), expected, found: node });
    }
    Ok(card.to_owned())
}

/// `cardN` from `/dev/dri/cardN`, lexically; anything else (relative, `..`, render or connector
/// names, other dirs) is `NotACardPath`.
fn card_name(card: &Path) -> Result<&str, TargetError> {
    let refuse = || TargetError::NotACardPath { path: card.to_owned() };
    let text = card.to_str().ok_or_else(refuse)?;
    let digits = text.strip_prefix(CARD_PREFIX).ok_or_else(refuse)?;
    let name = text.strip_prefix("/dev/dri/").ok_or_else(refuse)?;
    if is_digits(digits) { Ok(name) } else { Err(refuse()) }
}

fn is_card_name(name: &str) -> bool {
    name.strip_prefix("card").is_some_and(is_digits)
}

/// One to four ASCII digits: DRM minors are below 1024 (`DRM_MINOR_LIMIT`-sized ranges).
fn is_digits(text: &str) -> bool {
    (1..=4).contains(&text.len()) && text.bytes().all(|b| b.is_ascii_digit())
}

/// `Ok` when `class/drm/<name>` is a card whose driver is `appletbdrm` and whose USB device (the
/// bound interface's parent) is `05ac:8600`. The driver is checked first, so a desktop GPU is
/// refused as `WrongDriver` without any USB attribute read.
fn identify(sysfs: &Path, name: &str) -> Result<(), TargetError> {
    let card_dir = sysfs.join("class/drm").join(name);
    std::fs::symlink_metadata(&card_dir)
        .map_err(|source| TargetError::Io { path: card_dir.clone(), source })?;
    let device = card_dir.join("device");
    let driver_link = device.join("driver");
    let driver = match std::fs::read_link(&driver_link) {
        Ok(target) => target.file_name().and_then(|n| n.to_str()).unwrap_or_default().to_owned(),
        Err(e) if e.kind() == io::ErrorKind::NotFound => {
            return Err(TargetError::Unbound { card: name.to_owned() });
        }
        Err(source) => return Err(TargetError::Io { path: driver_link, source }),
    };
    if driver != DRIVER {
        return Err(TargetError::WrongDriver { driver });
    }
    let interface =
        std::fs::canonicalize(&device).map_err(|source| TargetError::Io { path: device.clone(), source })?;
    let usb = interface.parent().ok_or_else(|| TargetError::Io {
        path: interface.clone(),
        source: io::Error::new(io::ErrorKind::NotFound, "the USB interface has no parent device"),
    })?;
    let vendor = read_trimmed(&usb.join("idVendor"))?;
    let product = read_trimmed(&usb.join("idProduct"))?;
    if (vendor.as_str(), product.as_str()) != (T1_VENDOR, T1_PRODUCT) {
        return Err(TargetError::WrongDevice { vendor, product });
    }
    Ok(())
}

/// A short sysfs attribute, trimmed. Attributes are a few bytes; more than 64 is refused.
fn read_trimmed(path: &Path) -> Result<String, TargetError> {
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
fn parse_dev(text: &str) -> Option<(u32, u32)> {
    let (major, minor) = text.split_once(':')?;
    Some((major.parse().ok()?, minor.parse().ok()?))
}
